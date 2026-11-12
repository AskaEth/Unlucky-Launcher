//! 实例 → [`LaunchRequest`] 的装配：launch 命令与 `--dry-run` 共用这一段。
//!
//! 全部走 core 的既有公共面（`game::resolve` / `arguments.expand` / `GameLayout` /
//! `launch::adapter::build_request`），CLI 不复制任何继承/参数展开逻辑。

use std::path::PathBuf;

use crate::context::Ctx;
use crate::error::CliError;
use unlucky_core::game::arguments::Environment;
use unlucky_core::game::{self, ProfileId, VersionJson};
use unlucky_core::launch::adapter::{
    self, AdapterOptions, AdapterWarning, LibraryInput, SnapshotInput,
};
use unlucky_core::launch::request::{AuthPlaceholders, LaunchRequest, MemoryOptions};

/// 装配输入（来自命令行 + 实例档案）。
#[derive(Debug, Clone, Default)]
pub struct PlanInput {
    /// 版本 id（实例绑定的 `versions/<id>/`）。
    pub version_id: String,
    /// 实例 id（写进会话索引；无档案时为空）。
    pub instance_id: String,
    /// 实例显示名。
    pub instance_name: String,
    /// 已派生的运行目录（D20 隔离覆盖链在 `super::launch` 求值完，这里只消费）。
    pub game_dir: Option<PathBuf>,
    /// Java 可执行文件覆盖。
    pub java: Option<PathBuf>,
    /// 离线账户名（无账户时生效；有账户时身份由 `auth` 决定）。
    pub username: Option<String>,
    /// 已解析好的账户占位（`None` = 退回 `username` 的离线占位）。
    pub auth: Option<AuthPlaceholders>,
    /// 最大堆 MB。
    pub max_memory: Option<u32>,
}

/// 装配结果。
#[derive(Debug, Clone)]
pub struct Plan {
    /// 实际使用的版本 id。
    pub version_id: String,
    /// 装配好的启动请求。
    pub request: LaunchRequest,
    /// 非致命告警（缺主类/缺资源索引等）。
    pub warnings: Vec<String>,
}

/// 装配一个 [`LaunchRequest`]。
///
/// # Errors
///
/// 版本 JSON 缺失/非法、继承链断裂（缺父版本/成环）时返回用户错误。
pub fn prepare(ctx: &Ctx, input: &PlanInput) -> Result<Plan, CliError> {
    let game = ctx.game();
    let version_id = input.version_id.clone();
    let manifest_path = game.version_json(&version_id);
    let manifest = read_version_json(&manifest_path)?;

    let source = |id: &str| -> Option<VersionJson> {
        let path = game.version_json(id);
        read_version_json(&path).ok()
    };
    let resolved = game::resolve(&source, &manifest).map_err(|error| {
        CliError::user("inherit", format!("解析继承链失败：{error}"))
            .with_hint(format!("检查 {}", manifest_path.display()))
    })?;

    let environment = Environment::host();
    let expanded = resolved
        .arguments
        .as_ref()
        .map(|arguments| arguments.expand(&environment))
        .unwrap_or_default();

    let mut libraries: Vec<LibraryInput> = Vec::new();
    for library in resolved.libraries.as_deref().unwrap_or_default() {
        if !library.applies(&environment) {
            continue;
        }
        // 1.19+ 的 natives 是独立库条目（name 带分类器），必须进 classpath；
        // ≤1.18 的 `natives` 表条目归 natives::collect，不进 classpath。
        // Linux arm64：独立条目改写成 natives-linux-arm64（路径与展示名一起改）。
        let arm64 = library.linux_arm64_native_name(&environment);
        if arm64.is_none() && library.native_classifier(&environment).is_some() {
            continue;
        }
        let (coordinate, name) = match arm64 {
            Some(rewritten) => {
                let Ok(coordinate) =
                    unlucky_core::game::library::MavenCoordinate::parse(&rewritten)
                else {
                    continue;
                };
                (coordinate, rewritten)
            }
            None => {
                let Ok(coordinate) = library.coordinate() else {
                    continue;
                };
                (coordinate, library.name.clone())
            }
        };
        let path = if library.is_local() {
            game.version_library(&version_id, &coordinate)
        } else {
            game.library(&coordinate)
        };
        libraries.push(LibraryInput::new(path, name));
    }

    // natives 分类器构件（≤1.18.2 才有）：收集口径归 core，CLI 只搬运。
    let natives = unlucky_core::launch::natives::collect(
        &game,
        &version_id,
        resolved.libraries.as_deref().unwrap_or_default(),
        &environment,
    );

    let jar_id = resolved.jar.clone().unwrap_or_else(|| version_id.clone());
    let primary_jar = game.version_jar(&jar_id);
    // 运行目录：调用方（`ull launch`）已按隔离覆盖链派生；`None` 兜底到游戏根
    // （库/其它消费者直接构造 `PlanInput` 时保持旧行为）。
    let game_dir = input
        .game_dir
        .clone()
        .unwrap_or_else(|| ctx.data.game_home.clone());
    let assets_root = game.assets_dir();
    let assets_index = resolved
        .assets
        .clone()
        .or_else(|| resolved.asset_index.as_ref().map(|index| index.id.clone()));

    // 同 presenter：装配门限按**继承链的 MC 根版本**判（loader 实例的 version_id 带加载器后缀，
    // 周快照的 6 字符形态判据在那儿失效，`--graphicsBackend` 会被静默跳过）。
    let root_version = super::instance_launch::root_version_id(&game, &manifest);
    let snapshot = SnapshotInput {
        version_id: &version_id,
        version_name: Some(&input.instance_name),
        version_type: None,
        game_version: Some(root_version.as_str()),
        main_class: resolved.main_class.as_deref(),
        jvm_arguments: &expanded.jvm,
        game_arguments: &expanded.game,
        legacy_arguments: resolved.minecraft_arguments.as_deref(),
        libraries: &libraries,
        natives: &natives,
        primary_jar,
        game_dir,
        assets_index: assets_index.as_deref(),
        natives_dir: game.natives_dir(&version_id),
        library_dir: game.libraries_dir(),
        assets_root: assets_root.clone(),
        game_assets: assets_root,
    };

    // 实例级启动参数的三级回落（实例 → 全局 → 默认）。临时启动参数（`--max-memory` /
    // `--java`）在 D20 的覆盖链里位于实例之上，所以这里先把它们叠上去再落位。
    let mut effective = super::instance_launch::resolved(ctx, &input.instance_id);
    if let Some(mb) = input.max_memory {
        effective.memory_mb = mb;
    }
    if let Some(path) = &input.java {
        effective.java_path = Some(path.clone());
    }

    let options = AdapterOptions {
        java: super::java::selection(effective.java_path.as_deref()),
        memory: MemoryOptions {
            min_mb: None,
            max_mb: Some(effective.memory_mb),
        },
        auth: input.auth.clone().unwrap_or_else(|| AuthPlaceholders {
            player_name: input
                .username
                .clone()
                .unwrap_or_else(|| "Player".to_string()),
            uuid: ProfileId::generate().as_str().replace('-', ""),
            user_type: "legacy".to_string(),
            user_properties: "{}".to_string(),
            ..AuthPlaceholders::default()
        }),
        launcher_name: super::meta::LAUNCHER_NAME.to_string(),
        launcher_version: env!("CARGO_PKG_VERSION").to_string(),
        language: "zh-CN".to_string(),
        ..AdapterOptions::strict()
    };

    let output = adapter::build_request(&snapshot, options);
    let warnings = output.warnings.iter().map(describe_warning).collect();
    let mut request = output.request;
    // 会话索引要用实例身份（adapter 默认拿版本 id/名）。
    if !input.instance_id.is_empty() {
        request.instance.id = input.instance_id.clone();
    }
    request.instance.name = input.instance_name.clone();
    // 实例级启动参数（三级回落后的生效值）最后落位：user.* / memory / graphics_backend。
    unlucky_core::launch::overrides::apply(&mut request, &effective);

    Ok(Plan {
        version_id,
        request,
        warnings,
    })
}

fn read_version_json(path: &std::path::Path) -> Result<VersionJson, CliError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(CliError::user(
                "version_missing",
                format!("版本 JSON 不存在：{}", path.display()),
            )
            .with_hint("先准备好版本本体文件（下载装配属 P1 后续 mission）"));
        }
        Err(error) => return Err(CliError::io("读版本 JSON", path, &error)),
    };
    VersionJson::from_json_str(&text)
        .map_err(|error| CliError::user("version_parse", format!("{}：{error}", path.display())))
}

/// 适配层告警 → 人读一行（`AdapterWarning` 没有 Display，这里集中翻译）。
fn describe_warning(warning: &AdapterWarning) -> String {
    match warning {
        AdapterWarning::MissingLibrary { name, path } => {
            format!("缺少库文件 {name}（{}）", path.display())
        }
        AdapterWarning::MissingGameArguments => {
            "版本 JSON 既没有 arguments.game 也没有 minecraftArguments".to_string()
        }
        AdapterWarning::MissingAssetsIndex => {
            "版本 JSON 没有资源索引 id（${assets_index_name} 为空）".to_string()
        }
        AdapterWarning::MissingMainClass => "版本 JSON 没有 mainClass".to_string(),
        AdapterWarning::MissingNativeJar { name, path } => {
            format!("缺少 natives 构件 {name}（{}）", path.display())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    /// 一次性临时数据根（`--data-dir` 的等价物：custom 模式 → `<root>/.ULL` + `<root>/.minecraft`）。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ull-cli-snapshot-{tag}-{}-{seq}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("建临时目录");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn ctx_at(dir: &TempDir) -> Ctx {
        let globals = crate::cli::Globals {
            data_dir: Some(dir.path().to_path_buf()),
            ..crate::cli::Globals::default()
        };
        Ctx::resolve(&globals).expect("解析数据根")
    }

    fn write_version(ctx: &Ctx, id: &str, json: &str) {
        let path = ctx.game().version_json(id);
        std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
        std::fs::write(&path, json).expect("写版本 JSON");
    }

    /// **周快照 + 加载器实例**（M157 r3 复审 P2-1）：`prepare` 交给装配的游戏版本是
    /// 继承链根的 `26w14a`，不是 `26w14a-fabric-0.16.5`。
    ///
    /// 抓得住：`SnapshotInput.game_version` 回退成 `None`（装配用带后缀的 id 判门限，
    /// `--graphicsBackend` 会被静默跳过，而 CLI 自己的门槛显示「可下发」）。
    /// 抓不住：完整 argv（`crates/core/src/launch/overrides/tests.rs` 的周快照用例）。
    #[test]
    fn weekly_loader_instance_is_assembled_against_the_mc_root_version() {
        let dir = TempDir::new("weekly");
        let ctx = ctx_at(&dir);
        write_version(
            &ctx,
            "26w14a",
            r#"{"id":"26w14a","mainClass":"net.minecraft.client.main.Main",
                "libraries":[{"name":"org.lwjgl:lwjgl-vulkan:3.4.3"}]}"#,
        );
        write_version(
            &ctx,
            "26w14a-fabric-0.16.5",
            r#"{"id":"26w14a-fabric-0.16.5","inheritsFrom":"26w14a",
                "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient"}"#,
        );

        let plan = prepare(
            &ctx,
            &PlanInput {
                version_id: "26w14a-fabric-0.16.5".to_string(),
                instance_name: "周快照 Fabric".to_string(),
                ..PlanInput::default()
            },
        )
        .expect("装配应成功");

        assert_eq!(
            plan.request.instance.version.raw(),
            "26w14a",
            "装配的游戏版本必须是继承链的 MC 根版本（CLI 门槛与拼装必须同一口径）"
        );
        assert!(plan.request.instance.version.is_weekly_snapshot());
        assert!(plan.request.manifest.has_vulkan_backend);
    }
}
