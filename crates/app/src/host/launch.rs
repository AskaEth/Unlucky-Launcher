//! 启动全链：实例 → `LaunchRequest` → 子进程 → JSONL 事件流（GUI 版）。
//!
//! 与 CLI `ull launch` 同源同序（`core` 的 `game::resolve` / `arguments.expand` /
//! `launch::adapter::build_request` / `Launcher::spawn`），差异只有两处：
//! - **不阻塞 UI**：装配与等待都在后台线程里，进度/日志经 [`super::job::HostTask`] 回流；
//! - **身份来自 GUI 选中的账户**（`AccountStore` → `AuthPlaceholders`），凭据只在
//!   本模块内被读一次，绝不进 DTO。
//!
//! 停止：`kill_tree(pid)`（core 的三段式进程树回收），`finish()` 会照常收尾写索引。

use std::path::PathBuf;

use unlucky_core::game::arguments::Environment;
use unlucky_core::game::install::instance::read_instance_config;
use unlucky_core::game::isolation::{derive_game_dir, observe_mode, IsolationMode};
use unlucky_core::game::{self, VersionJson};
use unlucky_core::launch::adapter::{
    self, AdapterOptions, AdapterWarning, LibraryInput, SnapshotInput,
};
use unlucky_core::launch::request::{JavaSelection, LaunchRequest, MemoryOptions};
use unlucky_core::launch::time::{format_rfc3339_millis, now};
use unlucky_core::launch::tree_kill;
use unlucky_core::launch::{Launcher, LauncherConfig};
use unlucky_core::platform::paths::ProcessEnv;

use crate::dto::{
    InstanceId, InstanceRuntime, LaunchFinished, LaunchStage, LoaderKind, LogOutcome, Progress,
};

use super::env::HostEnv;
use super::job::{launch_summary, HostTask, JobSender};

/// 启动器名（与 CLI `meta::LAUNCHER_NAME` 同值）。
pub const LAUNCHER_NAME: &str = "Unlucky Launcher";

/// 一次启动需要的实例信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchTarget {
    /// 实例 id
    pub id: InstanceId,
    /// 实例名
    pub name: String,
    /// 绑定的版本 id
    pub version_id: String,
    /// 账户 id（`None` = 离线占位）
    pub account: Option<String>,
}

/// 选中并用起来的 Java（`summary` 给 UI 一行提示）。
pub struct JavaPick {
    /// 交给 adapter 的选择
    pub selection: JavaSelection,
    /// 人读一行
    pub summary: String,
}

/// 选一个能用的 Java：托管 > JAVA_HOME > PATH（`core::game::java` 的探测与优先级）。
pub fn pick_java(env: &HostEnv, version_id: &str) -> JavaPick {
    let store = env.store();
    let install_root = store
        .doc()
        .effective_str("java.installRoot")
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(PathBuf::from);
    let roots = unlucky_core::game::java::location::scan_roots(
        env.mode(),
        &env.layout,
        install_root.as_deref(),
        env.platform,
        &ProcessEnv,
    );
    let candidates = unlucky_core::game::java::candidates_in_roots(&roots);
    let required = unlucky_core::game::java::requirement_for_version(&env.game, version_id);
    let picked = unlucky_core::game::java::pick(&candidates, &required).cloned();
    match picked {
        Some(runtime) => {
            let major = runtime
                .major
                .unwrap_or(unlucky_core::game::java::DEFAULT_MAJOR);
            let summary = format!(
                "Java {}（{}{}）",
                runtime.version.clone().unwrap_or_else(|| major.to_string()),
                runtime.origin.category(),
                runtime
                    .vendor
                    .as_ref()
                    .map(|vendor| format!(" · {vendor}"))
                    .unwrap_or_default()
            );
            JavaPick {
                selection: JavaSelection::new(runtime.binary, major, runtime.bits),
                summary,
            }
        }
        None => {
            let binary = PathBuf::from(unlucky_core::game::java::java_binary_name());
            let major = required.minimum;
            JavaPick {
                selection: JavaSelection::new(binary, major, 64),
                summary: format!("PATH 上的 Java（未探测到版本，按 Java {major} 启动）"),
            }
        }
    }
}

/// 装配 [`LaunchRequest`]（与 CLI `snapshot::prepare` 同序）。
///
/// # Errors
///
/// 版本 JSON 缺失/非法、继承链断裂、隔离派生失败时返回用户可读文案。
pub fn build_request(
    env: &HostEnv,
    target: &LaunchTarget,
) -> Result<(LaunchRequest, Vec<String>), String> {
    let version_id = target.version_id.clone();
    let manifest_path = env.game.version_json(&version_id);
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("读版本 JSON 失败 {}：{error}", manifest_path.display()))?;
    let manifest =
        VersionJson::from_json_str(&text).map_err(|error| format!("版本 JSON 非法：{error}"))?;
    let source = |id: &str| -> Option<VersionJson> {
        std::fs::read_to_string(env.game.version_json(id))
            .ok()
            .and_then(|text| VersionJson::from_json_str(&text).ok())
    };
    let resolved =
        game::resolve(&source, &manifest).map_err(|error| format!("解析继承链失败：{error}"))?;

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
            env.game.version_library(&version_id, &coordinate)
        } else {
            env.game.library(&coordinate)
        };
        libraries.push(LibraryInput::new(path, library.name.clone()));
    }
    let natives = unlucky_core::launch::natives::collect(
        &env.game,
        &version_id,
        resolved.libraries.as_deref().unwrap_or_default(),
        &environment,
    );

    let game_dir = run_dir(env, target)?;
    let jar_id = resolved.jar.clone().unwrap_or_else(|| version_id.clone());
    let assets_root = env.game.assets_dir();
    let assets_index = resolved
        .assets
        .clone()
        .or_else(|| resolved.asset_index.as_ref().map(|index| index.id.clone()));
    let snapshot = SnapshotInput {
        version_id: &version_id,
        version_name: Some(&target.name),
        version_type: None,
        game_version: None,
        main_class: resolved.main_class.as_deref(),
        jvm_arguments: &expanded.jvm,
        game_arguments: &expanded.game,
        legacy_arguments: resolved.minecraft_arguments.as_deref(),
        libraries: &libraries,
        natives: &natives,
        primary_jar: env.game.version_jar(&jar_id),
        game_dir,
        assets_index: assets_index.as_deref(),
        natives_dir: env.game.natives_dir(&version_id),
        library_dir: env.game.libraries_dir(),
        assets_root: assets_root.clone(),
        game_assets: assets_root,
    };

    let java = pick_java(env, &version_id);
    let (auth, identity_line) = super::identity::resolve(env, target.account.as_deref())?;
    let memory = env
        .store()
        .effective("java.default_memory_mb")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    let options = AdapterOptions {
        java: java.selection,
        memory: MemoryOptions {
            min_mb: None,
            max_mb: memory,
        },
        auth,
        launcher_name: LAUNCHER_NAME.to_string(),
        launcher_version: env!("CARGO_PKG_VERSION").to_string(),
        language: "zh-CN".to_string(),
        ..AdapterOptions::strict()
    };
    let output = adapter::build_request(&snapshot, options);
    let mut warnings: Vec<String> = output.warnings.iter().map(describe_warning).collect();
    warnings.push(java.summary);
    warnings.push(identity_line);
    let mut request = output.request;
    request.instance.id = target.id.clone();
    request.instance.name = target.name.clone();
    Ok((request, warnings))
}

/// 派生运行目录（D20 覆盖链：实例 `isolation` > 全局 `instances.mode`）。
fn run_dir(env: &HostEnv, target: &LaunchTarget) -> Result<PathBuf, String> {
    let global = super::game::global_mode(env);
    let config = read_instance_config(&env.layout, &target.id)
        .map_err(|error| format!("读实例设置失败：{error}"))?;
    let derivation = derive_game_dir(&env.game, &target.version_id, global, config.as_ref(), None)
        .map_err(|error| format!("隔离目录派生失败：{error}"))?;
    if let Some(mut config) = config {
        let at = format_rfc3339_millis(now());
        if observe_mode(&mut config, &env.game, &at, derivation.isolation).unwrap_or(false) {
            let _ =
                unlucky_core::game::install::instance::write_instance_config(&env.layout, &config);
        }
    }
    Ok(derivation.game_dir)
}

/// 起一次启动（后台线程；进度/日志/收尾全部走 `HostTask`）。
pub fn start(env: HostEnv, tx: JobSender, target: LaunchTarget) {
    let id = target.id.clone();
    let tx_progress = tx.clone();
    super::job::spawn("launch", move || {
        let _ = tx_progress.send(HostTask::Runtime {
            id: id.clone(),
            runtime: InstanceRuntime::Busy {
                stage: LaunchStage::Launch,
                progress: Progress { done: 0, total: 1 },
            },
        });
        let (request, warnings) = match build_request(&env, &target) {
            Ok(ok) => ok,
            Err(reason) => {
                let _ = tx.send(HostTask::LaunchEnd(Box::new(LaunchFinished {
                    id,
                    session: String::new(),
                    outcome: LogOutcome::Crashed,
                    summary: format!("启动失败：{reason}"),
                })));
                return;
            }
        };
        for warning in &warnings {
            let _ = tx.send(HostTask::Notice(Box::new(crate::dto::NoticeView::new(
                &format!("launch-warn:{}", target.id),
                crate::dto::NoticeLevel::Info,
                "启动提示",
                warning,
            ))));
        }
        let config = LauncherConfig::new(env.sessions_root())
            .with_launcher(LAUNCHER_NAME, env!("CARGO_PKG_VERSION"));
        let session = match Launcher::new(config).spawn(&request) {
            Ok(session) => session,
            Err(error) => {
                let _ = tx.send(HostTask::LaunchEnd(Box::new(LaunchFinished {
                    id,
                    session: String::new(),
                    outcome: LogOutcome::Crashed,
                    summary: format!("启动失败：{error}"),
                })));
                return;
            }
        };
        let session_id = session.session_id().to_string();
        let pid = session.pid();
        let _ = tx.send(HostTask::LaunchStarted {
            id: id.clone(),
            session_id: session_id.clone(),
            pid,
        });
        let _ = tx.send(HostTask::Runtime {
            id: id.clone(),
            runtime: InstanceRuntime::Running { pid },
        });
        match session.finish() {
            Ok(handle) => {
                let outcome = super::identity::outcome_of(&handle);
                let _ = tx.send(HostTask::LaunchEnd(Box::new(LaunchFinished {
                    id,
                    session: session_id,
                    outcome,
                    summary: format!(
                        "{}（{}，{:.1}s）",
                        launch_summary(outcome, Some(handle.exit.code)),
                        handle.exit.exit_type.as_str(),
                        handle.uptime_ms as f64 / 1000.0
                    ),
                })));
            }
            Err(error) => {
                let _ = tx.send(HostTask::LaunchEnd(Box::new(LaunchFinished {
                    id,
                    session: session_id,
                    outcome: LogOutcome::SilentFail,
                    summary: format!("启动收尾失败：{error}"),
                })));
            }
        }
    });
}

/// 停止一次启动（按 pid 回收进程树；`finish()` 会在后台线程里照常收尾）。
pub fn stop(pid: u32) -> Result<(), String> {
    tree_kill::kill_tree(pid, std::time::Duration::from_secs(3))
        .map_err(|error| format!("停止进程 {pid} 失败：{error}"))
}

/// 适配层告警 → 人读一行。
fn describe_warning(warning: &AdapterWarning) -> String {
    match warning {
        AdapterWarning::MissingLibrary { name, path } => {
            format!("缺少库文件 {name}（{}）", path.display())
        }
        AdapterWarning::MissingGameArguments => "版本 JSON 没有游戏参数".to_string(),
        AdapterWarning::MissingAssetsIndex => "版本 JSON 没有资源索引".to_string(),
        AdapterWarning::MissingMainClass => "版本 JSON 没有 mainClass".to_string(),
        AdapterWarning::MissingNativeJar { name, path } => {
            format!("缺少 natives 构件 {name}（{}）", path.display())
        }
    }
}

/// 加载器 → 适配层的模组加载器标记（GUI 只用于显示，不改命令行）。
pub fn loader_label(kind: LoaderKind) -> &'static str {
    match kind {
        LoaderKind::Vanilla => "原版",
        LoaderKind::Fabric => "Fabric",
        LoaderKind::Quilt => "Quilt",
        LoaderKind::Forge => "Forge",
        LoaderKind::NeoForge => "NeoForge",
        LoaderKind::OptiFine => "OptiFine",
        LoaderKind::LiteLoader => "LiteLoader",
    }
}

/// 隔离档位的展示串（`isolated` / `shared`）。
pub fn isolation_label(mode: IsolationMode) -> &'static str {
    mode.as_str()
}
