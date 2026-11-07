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
        if !library.applies(&environment) || library.native_classifier(&environment).is_some() {
            continue;
        }
        let Ok(coordinate) = library.coordinate() else {
            continue;
        };
        let path = if library.is_local() {
            game.version_library(&version_id, &coordinate)
        } else {
            game.library(&coordinate)
        };
        libraries.push(LibraryInput::new(path, library.name.clone()));
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

    let snapshot = SnapshotInput {
        version_id: &version_id,
        version_name: Some(&input.instance_name),
        version_type: None,
        game_version: None,
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

    let options = AdapterOptions {
        java: super::java::selection(input.java.as_deref()),
        memory: MemoryOptions {
            min_mb: None,
            max_mb: input.max_memory,
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
