//! 装配 [`LaunchRequest`]：实例 → 版本 JSON → 继承链 → 参数展开 → 适配器（M298 从
//! `host/launch.rs` 拆出，纯搬移 + `pub use` 保路径）。
//!
//! 这一层与 CLI 的 `snapshot::prepare` 同源同序，差异只在「实例信息从哪来」：CLI 从命令行，
//! 这里从 GUI 选中的实例（[`LaunchTarget`]）。
//!
//! 拆分的直接原因只有行数（`host/launch.rs` 当时 399 行、贴着 400 行硬线），**没有行为变化**：
//! 函数体与顺序逐行一致，调用点仍是 `super::launch::build_request`（父模块 `pub use` 转出）。

use std::path::PathBuf;

use unlucky_core::game::arguments::Environment;
use unlucky_core::game::install::instance::read_instance_config;
use unlucky_core::game::isolation::{derive_game_dir, observe_mode};
use unlucky_core::game::{self, VersionJson};
use unlucky_core::launch::adapter::{
    self, AdapterOptions, AdapterWarning, LibraryInput, SnapshotInput,
};
use unlucky_core::launch::request::{LaunchRequest, MemoryOptions};
use unlucky_core::launch::time::{format_rfc3339_millis, now};

use crate::dto::{LaunchFailureStage, LaunchFailureView};

use super::super::env::HostEnv;
use super::super::{game as host_game, identity, launch_args};
#[cfg(test)]
use super::pick_java;
use super::{JavaPick, LaunchTarget, LAUNCHER_NAME};

/// 装配 [`LaunchRequest`]（与 CLI `snapshot::prepare` 同序）。
///
/// **只给单测用**（M383 的 `#[cfg(test)]` 是声明，不是消音）：真启动走
/// [`build_request_with`]（Java 在受理那一刻已经挑好并判过，见 `host::launch::accept`），
/// 而装配链的单测（`host::game::tests_multiroot*`）要的正是「自己挑一遍再装配」那条整链。
///
/// # Errors
///
/// 任一步失败都返回 [`LaunchFailureView`]：**阶段**（版本 JSON / 继承链 / 实例设置 / 隔离目录 /
/// 账户）+ 那一步的原文。文案不在这一层拼（presenter 不依赖 `unlucky-i18n`）。
#[cfg(test)]
pub fn build_request(
    env: &HostEnv,
    target: &LaunchTarget,
) -> Result<(LaunchRequest, Vec<String>), LaunchFailureView> {
    let overrides = launch_args::resolved(env, &target.id);
    let game = host_game::game_for(env, &target.id);
    // 挑 Java 的**版本要求**也按实例自己的根读（M274 r1 复审 P1-2）。
    let java = pick_java(
        env,
        &game,
        &target.version_id,
        overrides.java_path.as_deref(),
    );
    build_request_with(env, target, java)
}

/// 装配本体：Java 已经挑好并判过（M331：启动受理那一刻的结果，见 `host::launch::accept`）。
///
/// 分成两个入口而不是改掉 [`build_request`] 的签名：装配链的单测（`host::game::tests_multiroot*`）
/// 要的就是「自己挑一遍再装配」那条整链，而真启动那条不该为一个测试保留第二次探测。
pub fn build_request_with(
    env: &HostEnv,
    target: &LaunchTarget,
    java: JavaPick,
) -> Result<(LaunchRequest, Vec<String>), LaunchFailureView> {
    let version_id = target.version_id.clone();
    // **这个实例自己的根**（M274）：官方根 / 另一个 `.minecraft` 里的版本，清单、库、natives、
    // assets 全在它自己那个根下。拿 `env.game` 去读会读空（甚至读到同名的另一个版本）。
    let game = host_game::game_for(env, &target.id);
    let manifest_path = game.version_json(&version_id);
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
        std::fs::read_to_string(game.version_json(id))
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
            game.version_library(&version_id, &coordinate)
        } else {
            game.library(&coordinate)
        };
        libraries.push(LibraryInput::new(path, name));
    }
    let natives = unlucky_core::launch::natives::collect(
        &game,
        &version_id,
        resolved.libraries.as_deref().unwrap_or_default(),
        &environment,
    );

    let game_dir = run_dir(env, &game, target)?;
    let jar_id = resolved.jar.clone().unwrap_or_else(|| version_id.clone());
    let assets_root = game.assets_dir();
    let assets_index = resolved
        .assets
        .clone()
        .or_else(|| resolved.asset_index.as_ref().map(|index| index.id.clone()));
    // 装配的门限 / 分支按**继承链的 MC 根版本**判，不是带加载器后缀的 version_id：
    // `26w14a-fabric-0.16.5` 这种 id 在 `GameVersion::is_weekly_snapshot`（6 字符形态）上判不出来，
    // 周快照的 `--graphicsBackend` 会在拼装链里被静默跳过——而详情页的门槛
    // （`host::launch_args::backend_gate`）与 CLI 用的都是根版本，两边必须同一口径。
    let (mc_version, _) = host_game::describe_version(&game, &version_id);
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
        primary_jar: game.version_jar(&jar_id),
        game_dir,
        assets_index: assets_index.as_deref(),
        natives_dir: game.natives_dir(&version_id),
        library_dir: game.libraries_dir(),
        assets_root: assets_root.clone(),
        game_assets: assets_root,
    };

    let overrides = launch_args::resolved(env, &target.id);
    let (auth, identity_line) = identity::resolve(env, target.account.as_deref())?;
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

/// 派生运行目录（D20 覆盖链：实例 `isolation` > **基底档**，见 [`super::super::game::base_mode`]）。
///
/// 基底档对自动发现的实例是「盘上有没有痕迹」，对其它实例就是全局 `instances.mode`——
/// 于是这里不需要知道实例是从哪来的，只有一处判据。
fn run_dir(
    env: &HostEnv,
    game: &unlucky_core::game::GameLayout,
    target: &LaunchTarget,
) -> Result<PathBuf, LaunchFailureView> {
    let base = host_game::base_mode(env, game, &target.id, &target.version_id);
    let config = read_instance_config(&env.layout, &target.id).map_err(|error| {
        LaunchFailureView::new(LaunchFailureStage::InstanceConfig, error.to_string())
    })?;
    let derivation = derive_game_dir(game, &target.version_id, base, config.as_ref(), None)
        .map_err(|error| {
            LaunchFailureView::new(LaunchFailureStage::Isolation, error.to_string())
        })?;
    if let Some(mut config) = config {
        let at = format_rfc3339_millis(now());
        if observe_mode(&mut config, game, &at, derivation.isolation).unwrap_or(false) {
            let _ =
                unlucky_core::game::install::instance::write_instance_config(&env.layout, &config);
        }
    }
    Ok(derivation.game_dir)
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
