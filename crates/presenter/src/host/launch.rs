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
use unlucky_core::launch::request::{LaunchRequest, MemoryOptions};
use unlucky_core::launch::time::{format_rfc3339_millis, now};
use unlucky_core::launch::tree_kill;
use unlucky_core::launch::{Launcher, LauncherConfig};

use crate::dto::{
    InstanceId, InstanceRuntime, LaunchFailureStage, LaunchFailureView, LaunchFinished,
    LaunchStage, LogOutcome, Progress,
};

use super::env::HostEnv;

mod java;

use super::job::{HostTask, JobSender};
pub use java::pick_java;

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

/// 装配 [`LaunchRequest`]（与 CLI `snapshot::prepare` 同序）。
///
/// # Errors
///
/// 任一步失败都返回 [`LaunchFailureView`]：**阶段**（版本 JSON / 继承链 / 实例设置 / 隔离目录 /
/// 账户）+ 那一步的原文。文案不在这一层拼（presenter 不依赖 `unlucky-i18n`）。
pub fn build_request(
    env: &HostEnv,
    target: &LaunchTarget,
) -> Result<(LaunchRequest, Vec<String>), LaunchFailureView> {
    let version_id = target.version_id.clone();
    let manifest_path = env.game.version_json(&version_id);
    let text = std::fs::read_to_string(&manifest_path).map_err(|error| {
        LaunchFailureView::new(
            LaunchFailureStage::VersionManifest,
            format!("{}：{error}", manifest_path.display()),
        )
    })?;
    let manifest = VersionJson::from_json_str(&text).map_err(|error| {
        LaunchFailureView::new(
            LaunchFailureStage::VersionManifest,
            format!("{}：{error}", manifest_path.display()),
        )
    })?;
    let source = |id: &str| -> Option<VersionJson> {
        std::fs::read_to_string(env.game.version_json(id))
            .ok()
            .and_then(|text| VersionJson::from_json_str(&text).ok())
    };
    let resolved = game::resolve(&source, &manifest).map_err(|error| {
        LaunchFailureView::new(LaunchFailureStage::Inheritance, error.to_string())
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
            env.game.version_library(&version_id, &coordinate)
        } else {
            env.game.library(&coordinate)
        };
        libraries.push(LibraryInput::new(path, name));
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
    // 装配的门限 / 分支按**继承链的 MC 根版本**判，不是带加载器后缀的 version_id：
    // `26w14a-fabric-0.16.5` 这种 id 在 `GameVersion::is_weekly_snapshot`（6 字符形态）上判不出来，
    // 周快照的 `--graphicsBackend` 会在拼装链里被静默跳过——而详情页的门槛
    // （`host::launch_args::backend_gate`）与 CLI 用的都是根版本，两边必须同一口径。
    let (mc_version, _) = super::game::describe_version(env, &version_id);
    let snapshot = SnapshotInput {
        version_id: &version_id,
        version_name: Some(&target.name),
        version_type: None,
        game_version: Some(mc_version.as_str()),
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

    let overrides = super::launch_args::resolved(env, &target.id);
    let java = pick_java(env, &version_id, overrides.java_path.as_deref());
    let (auth, identity_line) = super::identity::resolve(env, target.account.as_deref())?;
    let options = AdapterOptions {
        java: java.selection,
        memory: MemoryOptions {
            min_mb: None,
            max_mb: Some(overrides.memory_mb),
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
    // 实例级启动参数（三级回落后的生效值）最后落位：user.* / memory / graphics_backend。
    unlucky_core::launch::overrides::apply(&mut request, &overrides);
    Ok((request, warnings))
}

/// 派生运行目录（D20 覆盖链：实例 `isolation` > 全局 `instances.mode`）。
fn run_dir(env: &HostEnv, target: &LaunchTarget) -> Result<PathBuf, LaunchFailureView> {
    let global = super::game::global_mode(env);
    let config = read_instance_config(&env.layout, &target.id).map_err(|error| {
        LaunchFailureView::new(LaunchFailureStage::InstanceConfig, error.to_string())
    })?;
    let derivation = derive_game_dir(&env.game, &target.version_id, global, config.as_ref(), None)
        .map_err(|error| {
            LaunchFailureView::new(LaunchFailureStage::Isolation, error.to_string())
        })?;
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
            Err(failure) => {
                let _ = tx.send(HostTask::LaunchEnd(Box::new(LaunchFinished {
                    id,
                    session: String::new(),
                    outcome: LogOutcome::Crashed,
                    exit_code: None,
                    uptime_ms: 0,
                    failure: Some(failure),
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
                    exit_code: None,
                    uptime_ms: 0,
                    failure: Some(LaunchFailureView::new(
                        LaunchFailureStage::Spawn,
                        error.to_string(),
                    )),
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
                    exit_code: Some(handle.exit.code),
                    uptime_ms: handle.uptime_ms,
                    // 跑起来再崩不算「装配失败」：结局由 `outcome` 表达，这里留 `None`。
                    failure: None,
                })));
            }
            Err(error) => {
                let _ = tx.send(HostTask::LaunchEnd(Box::new(LaunchFinished {
                    id,
                    session: session_id,
                    outcome: LogOutcome::SilentFail,
                    exit_code: None,
                    uptime_ms: 0,
                    failure: Some(LaunchFailureView::new(
                        LaunchFailureStage::Spawn,
                        error.to_string(),
                    )),
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

/// 隔离档位的展示串（`isolated` / `shared`）。
pub fn isolation_label(mode: IsolationMode) -> &'static str {
    mode.as_str()
}

#[cfg(test)]
mod tests;
