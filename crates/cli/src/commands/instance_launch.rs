//! `instances launch-args <instance>`（M157）：实例级启动参数的读/写。
//!
//! 五项的**含义与回落规则**都在 core（[`unlucky_core::game::launch_overrides`]），本命令只做
//! 三件事：找实例、把命令行选项拼成 [`LaunchOverridesPatch`]、把生效值印出来。
//!
//! ```text
//! ull instances launch-args 生存                 # 读：实例层 + 生效值 + 后端门槛
//! ull instances launch-args 生存 --jvm "-XX:+UseG1GC" --backend vulkan
//! ull instances launch-args 生存 --memory 8192 --game "--fullscreen"
//! ull instances launch-args 生存 --clear         # 五项全部回「跟随全局」
//! ```
//!
//! 判定「版本够不够 Vulkan」用 core 的组合判据
//! [`unlucky_core::launch::overrides::backend_allowed`]（继承解析后的库表 + 版本门槛），
//! 与 GUI 灰化、拼装链的告警是同一个函数——CLI 不另写一条。
//!
//! 全局层（`java.extra_args` / `java.default_memory_mb` / `java.selected_path`）在这里读一次
//! 设置文档填 [`GlobalLaunchDefaults`]；presenter 的 host 侧有一份同形的读法（两个 crate 各自
//! 依赖 core，没法共享那三行 IO），字段口径以 core 的回落表为准。

use serde_json::json;
use unlucky_core::game::install::instance::{
    read_instance_config, write_instance_config, InstanceConfig,
};
use unlucky_core::game::launch_overrides::{
    resolve, GlobalLaunchDefaults, GraphicsBackend, LaunchOverridesPatch, ResolvedLaunch,
};
use unlucky_core::game::{self, VersionJson};
use unlucky_core::launch::game_args::GRAPHICS_BACKEND_MIN_VERSION;
use unlucky_core::launch::game_version::GameVersion;
use unlucky_core::launch::overrides::backend_allowed;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::cli::InstanceLaunchArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

use super::instances::read_profiles;

/// `ull instances launch-args`。
///
/// # Errors
///
/// 实例不存在、值非法（内存越界 / 未知后端）、实例文件读写失败时返回用户错误。
pub fn run(ctx: &Ctx, args: &InstanceLaunchArgs) -> Result<Report, CliError> {
    let profiles = read_profiles(ctx)?;
    let profile = profiles
        .profiles
        .iter()
        .find(|profile| profile.id.as_str() == args.instance || profile.name == args.instance)
        .cloned()
        .ok_or_else(|| {
            CliError::user("not_found", format!("找不到实例 `{}`", args.instance))
                .with_hint("用 `%prog% instances list` 看已登记的实例")
        })?;
    let id = profile.id.as_str().to_string();
    let layout = ctx.layout();

    let mut config = read_instance_config(&layout, &id)
        .map_err(|error| CliError::user("instance_config", format!("读实例设置失败：{error}")))?
        .unwrap_or_else(|| InstanceConfig::new(&id, &profile.version_id));

    let mut updated = false;
    if !args.is_read_only() {
        config.launch = config
            .launch
            .patched(&patch_from(args))
            .map_err(CliError::usage)?;
        write_instance_config(&layout, &config).map_err(|error| {
            CliError::user("instance_config", format!("写实例设置失败：{error}"))
        })?;
        updated = true;
    }

    let effective = resolve(Some(&config.launch), &global_defaults(ctx));
    let (allowed, mc_version) = backend_gate(ctx, &profile.version_id);
    let layer = &config.launch;

    let data = json!({
        "instance_id": id,
        "name": profile.name,
        "version_id": profile.version_id,
        "updated": updated,
        "launch": {
            "jvm_args": layer.jvm_args.clone().unwrap_or_default(),
            "game_args": layer.game_args.clone().unwrap_or_default(),
            "memory_mb": layer.memory_mb,
            "java_path": layer.java_path.as_ref().map(|path| path.display().to_string()),
            "graphics_backend": layer.graphics_backend.unwrap_or_default().as_str(),
        },
        "effective": {
            "jvm_args": effective.instance_jvm_args.clone().unwrap_or_default(),
            "global_jvm_args": effective.global_jvm_args.clone().unwrap_or_default(),
            "game_args": effective.game_args.clone().unwrap_or_default(),
            "memory_mb": effective.memory_mb,
            "java_path": effective.java_path.as_ref().map(|path| path.display().to_string()),
            "graphics_backend": effective.graphics_backend.as_str(),
            "graphics_backend_allowed": allowed,
            "graphics_backend_min_version": GRAPHICS_BACKEND_MIN_VERSION,
            "mc_version": mc_version,
        },
        "graphics_backend_options": GraphicsBackend::IDS,
    });

    let mut human = vec![format!(
        "{}（{} · {}）{}",
        profile.name,
        profile.version_id,
        id,
        if updated { "  ← 已保存" } else { "" }
    )];
    human.push(format!(
        "  JVM 参数（实例）: {}",
        layer
            .jvm_args
            .clone()
            .unwrap_or_else(|| "（跟随全局）".to_string())
    ));
    human.push(format!(
        "  游戏参数（实例）: {}",
        layer
            .game_args
            .clone()
            .unwrap_or_else(|| "（无）".to_string())
    ));
    human.push(format!(
        "  最大内存: {} MiB{}",
        effective.memory_mb,
        match layer.memory_mb {
            Some(_) => "（实例覆盖）",
            None => "（跟随全局 / 出厂默认）",
        }
    ));
    human.push(format!(
        "  Java: {}",
        effective
            .java_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "（自动挑：托管 > JAVA_HOME > PATH）".to_string())
    ));
    human.push(format!(
        "  图形后端: {}{}",
        effective.graphics_backend.as_str(),
        if effective.graphics_backend.is_default() {
            "（不下发 --graphicsBackend）".to_string()
        } else if allowed {
            format!("（{} 可下发）", mc_version)
        } else {
            format!("（版本 {mc_version} 不支持，不会下发；需 MC {GRAPHICS_BACKEND_MIN_VERSION}+）")
        }
    ));

    Ok(Report::new(data, human))
}

/// 命令行选项 → core 的文本补丁（`--clear` 先清五项，随后给出的项覆盖）。
fn patch_from(args: &InstanceLaunchArgs) -> LaunchOverridesPatch {
    let mut patch = if args.clear {
        LaunchOverridesPatch::clear_all()
    } else {
        LaunchOverridesPatch::default()
    };
    if let Some(value) = &args.jvm_args {
        patch.jvm_args = Some(value.clone());
    }
    if let Some(value) = &args.game_args {
        patch.game_args = Some(value.clone());
    }
    if let Some(value) = &args.memory_mb {
        patch.memory_mb = Some(value.clone());
    }
    if let Some(value) = &args.java {
        patch.java_path = Some(value.clone());
    }
    if let Some(value) = &args.backend {
        patch.graphics_backend = Some(value.clone());
    }
    patch
}

/// 全局层（三个设置键）。
pub(crate) fn global_defaults(ctx: &Ctx) -> GlobalLaunchDefaults {
    let store = SettingStore::load(ctx.launcher_config(), launcher_schema());
    let text = |key: &str| -> Option<String> {
        store
            .doc()
            .effective_str(key)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    GlobalLaunchDefaults {
        jvm_args: text("java.extra_args"),
        game_args: None,
        memory_mb: store
            .effective("java.default_memory_mb")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        java_path: text("java.selected_path").map(std::path::PathBuf::from),
        graphics_backend: None,
    }
}

/// 三级回落后的生效值（启动装配用；`instance_id` 为空 = 没有实例档案，只用全局层）。
///
/// 与 presenter 的 `host::launch_args::resolved` 同义：那边给 GUI 用，这边给 `ull launch` 用，
/// 规则都在 core 的 [`resolve`]，两处只剩「读哪几份文件」的差别。
pub(crate) fn resolved(ctx: &Ctx, instance_id: &str) -> ResolvedLaunch {
    let overrides = if instance_id.is_empty() {
        None
    } else {
        read_instance_config(&ctx.layout(), instance_id)
            .ok()
            .flatten()
            .map(|config| config.launch)
    };
    resolve(overrides.as_ref(), &global_defaults(ctx))
}

/// 版本能不能用 `--graphicsBackend`，以及判定依据的游戏版本（继承链根）。
fn backend_gate(ctx: &Ctx, version_id: &str) -> (bool, String) {
    let game_layout = ctx.game();
    let Ok(text) = std::fs::read_to_string(game_layout.version_json(version_id)) else {
        return (false, version_id.to_string());
    };
    let Ok(manifest) = VersionJson::from_json_str(&text) else {
        return (false, version_id.to_string());
    };
    let mc_version = root_version_id(&game_layout, &manifest);
    let source = |id: &str| -> Option<VersionJson> {
        std::fs::read_to_string(game_layout.version_json(id))
            .ok()
            .and_then(|text| VersionJson::from_json_str(&text).ok())
    };
    let resolved = game::resolve(&source, &manifest).unwrap_or(manifest);
    let allowed = backend_allowed(&GameVersion::parse(&mc_version), &resolved);
    (allowed, mc_version)
}

/// 继承链根（原版层）的 id——能力位与版本门限都以它为准（loader 层的 id 带加载器后缀）。
///
/// `pub(crate)`：`snapshot::prepare` 的装配（`SnapshotInput.game_version`）也要这一份口径，
/// 门槛判定（本模块的 `backend_gate`）与拼装必须用**同一个**根版本，否则会出现
/// 「UI 说可用、命令行里没有这个参数」的静默偏差。
pub(crate) fn root_version_id(
    layout: &unlucky_core::game::GameLayout,
    manifest: &VersionJson,
) -> String {
    let mut current = manifest.id.clone().unwrap_or_default();
    let mut parent = manifest.inherits_from.clone().filter(|id| !id.is_empty());
    for _ in 0..16 {
        let Some(next) = parent else { break };
        let Ok(text) = std::fs::read_to_string(layout.version_json(&next)) else {
            return next;
        };
        let Ok(json) = VersionJson::from_json_str(&text) else {
            return next;
        };
        current = json.id.clone().unwrap_or(next);
        parent = json.inherits_from.clone().filter(|id| !id.is_empty());
    }
    current
}

#[cfg(test)]
mod tests;
