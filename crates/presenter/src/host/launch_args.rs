//! 实例启动参数：读（三级回落 + 版本门槛）与写（`launch` 段）。
//!
//! 本模块是 M157 的 host 侧唯一入口：
//! - [`view`] → [`LaunchArgsView`]（详情页「启动参数」区）；
//! - [`apply`] → 写回实例文件（`launch.*`），空输入清成「跟随」；
//! - [`resolved`] → 启动装配用的生效值（[`ResolvedLaunch`]），CLI 的 `snapshot::prepare`
//!   与 GUI 的 `host::launch` 都从这里取，避免两条装配链各算一遍回落。
//!
//! 判据都在 core，本文件只做三件事：读设置文档填 [`GlobalLaunchDefaults`]、读实例文件、
//! 把 core 的结论翻成 DTO。

use std::path::PathBuf;

use unlucky_core::game::install::instance::{
    read_instance_config, write_instance_config, InstanceConfig,
};
use unlucky_core::game::launch_overrides::{
    resolve as resolve_overrides, GlobalLaunchDefaults, GraphicsBackend, LaunchOverrides,
    LaunchOverridesPatch, ResolvedLaunch,
};
use unlucky_core::game::{self, VersionCapabilities, VersionJson};
use unlucky_core::launch::game_args::{graphics_backend_allowed, GRAPHICS_BACKEND_MIN_VERSION};
use unlucky_core::launch::game_version::GameVersion;

use crate::dto::{LaunchArgsPatch, LaunchArgsView};

use super::env::HostEnv;

/// 全局层：`java.extra_args` / `java.default_memory_mb` / `java.selected_path`。
///
/// 另外两项（游戏参数、图形后端）本轮没有全局键——回落链照样成立（全局层为 `None`），
/// 理由见 M157 的定案（不为它们单开全局键，免得与实例语义打架）。
pub fn global_defaults(env: &HostEnv) -> GlobalLaunchDefaults {
    let store = env.store();
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
        java_path: text("java.selected_path").map(PathBuf::from),
        graphics_backend: None,
    }
}

/// 读实例文件的 `launch` 段（文件缺失 / 读失败 = 空段，不让详情页因此打不开）。
fn instance_overrides(env: &HostEnv, instance_id: &str) -> LaunchOverrides {
    if instance_id.is_empty() {
        return LaunchOverrides::default();
    }
    read_instance_config(&env.layout, instance_id)
        .ok()
        .flatten()
        .map(|config| config.launch)
        .unwrap_or_default()
}

/// 三级回落后的生效值（启动装配与详情页共用同一份）。
pub fn resolved(env: &HostEnv, instance_id: &str) -> ResolvedLaunch {
    resolve_overrides(
        Some(&instance_overrides(env, instance_id)),
        &global_defaults(env),
    )
}

/// 版本能力位（解析继承链之后取库表；读不到版本 JSON = 什么能力都没有）。
fn capabilities(env: &HostEnv, version_id: &str) -> Option<VersionJson> {
    let text = std::fs::read_to_string(env.game.version_json(version_id)).ok()?;
    let manifest = VersionJson::from_json_str(&text).ok()?;
    let source = |id: &str| -> Option<VersionJson> {
        std::fs::read_to_string(env.game.version_json(id))
            .ok()
            .and_then(|text| VersionJson::from_json_str(&text).ok())
    };
    game::resolve(&source, &manifest).ok()
}

/// 该实例版本能不能下发 `--graphicsBackend`，以及判定所依据的游戏版本。
///
/// 判据是 core 的两件套（库表能力位 + 版本门槛），**UI 不复制**：
/// `graphics_backend_allowed(GameVersion, has_vulkan_backend)`。
pub fn backend_gate(env: &HostEnv, version_id: &str) -> (bool, String) {
    let mc_version = super::game::describe_version(env, version_id).0;
    let has_vulkan = capabilities(env, version_id)
        .map(|json| VersionCapabilities::from_json(&json).has_vulkan_backend)
        .unwrap_or(false);
    let allowed = graphics_backend_allowed(&GameVersion::parse(&mc_version), has_vulkan);
    (allowed, mc_version)
}

/// 详情页「启动参数」区的读取视图。
pub fn view(env: &HostEnv, instance_id: &str, version_id: &str) -> LaunchArgsView {
    let instance = instance_overrides(env, instance_id);
    let effective = resolved(env, instance_id);
    let (allowed, mc_version) = backend_gate(env, version_id);
    LaunchArgsView {
        instance_id: instance_id.to_string(),
        jvm_args: instance.jvm_args.unwrap_or_default(),
        game_args: instance.game_args.unwrap_or_default(),
        memory_mb: instance.memory_mb,
        memory_effective_mb: effective.memory_mb,
        java_path: instance
            .java_path
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        java_effective: effective
            .java_path
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        graphics_backend: instance
            .graphics_backend
            .unwrap_or_default()
            .as_str()
            .to_string(),
        graphics_backend_effective: effective.graphics_backend.as_str().to_string(),
        graphics_backend_allowed: allowed,
        mc_version,
        graphics_backend_min_version: GRAPHICS_BACKEND_MIN_VERSION.to_string(),
        graphics_backend_options: GraphicsBackend::IDS
            .iter()
            .map(|id| (*id).to_string())
            .collect(),
    }
}

/// 写回实例启动参数（`None` = 不改；`Some("")` = 清空回「跟随」）。
///
/// 校验（内存整数/范围、后端词表）在 core 的 [`LaunchOverridesPatch`] 里做一次，
/// GUI 与 CLI 拿同一条错误文案。
///
/// # Errors
///
/// 实例不存在、值非法（内存越界 / 未知图形后端）、读写实例文件失败时返回用户可读文案。
pub fn apply(
    env: &HostEnv,
    instance_id: &str,
    version_id: &str,
    patch: &LaunchArgsPatch,
) -> Result<(), String> {
    let core_patch = LaunchOverridesPatch {
        jvm_args: patch.jvm_args.clone(),
        game_args: patch.game_args.clone(),
        memory_mb: patch.memory_mb.clone(),
        java_path: patch.java_path.clone(),
        graphics_backend: patch.graphics_backend.clone(),
    };
    let overrides = instance_overrides(env, instance_id).patched(&core_patch)?;

    let mut config = read_instance_config(&env.layout, instance_id)
        .map_err(|error| format!("读实例设置失败：{error}"))?
        .unwrap_or_else(|| InstanceConfig::new(instance_id, version_id));
    config.launch = overrides;
    write_instance_config(&env.layout, &config)
        .map(|_| ())
        .map_err(|error| format!("写实例设置失败：{error}"))
}

/// 门槛判据里的最低版本常量（提示文案用；判据本身在 core）。
pub fn min_version_hint() -> &'static str {
    GRAPHICS_BACKEND_MIN_VERSION
}

impl super::CoreHost {
    /// 写实例启动参数（先按 id 找档案拿 `versionId`，再落盘）。
    ///
    /// 实例必须已登记：没有档案时写 `config/instances/<id>.json` 会造出一个孤立的设置文件，
    /// 启动链根本不会读它（它按 `profiles.json` 找实例）。
    pub(super) fn apply_launch_args(
        &mut self,
        id: &str,
        patch: &LaunchArgsPatch,
    ) -> Result<(), String> {
        let profiles = super::game::profiles(&self.env);
        let profile = profiles
            .profiles
            .iter()
            .find(|profile| profile.id.as_str() == id)
            .ok_or_else(|| format!("找不到实例 `{id}`"))?;
        apply(&self.env, id, &profile.version_id, patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> (HostEnv, tempdir::Dir) {
        let dir = tempdir::Dir::new("ull-launch-args");
        let env = HostEnv::at_root(&dir.path);
        std::fs::create_dir_all(env.layout.config()).expect("config 目录");
        (env, dir)
    }

    /// 最小的 tempdir（与本 crate 其它 host 测试同款：不引第三方 tempfile）。
    mod tempdir {
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicU64, Ordering};

        pub struct Dir {
            pub path: PathBuf,
        }

        impl Dir {
            pub fn new(tag: &str) -> Self {
                // 每个用例一个目录：同一进程里跑多个用例时，只用 PID 命名会互相 `remove_dir_all`
                // （并行跑就是「系统找不到指定的路径」——M157 r2 实测）。
                static COUNTER: AtomicU64 = AtomicU64::new(0);
                let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!("{tag}-{}-{seq}", std::process::id()));
                let _ = std::fs::remove_dir_all(&path);
                std::fs::create_dir_all(&path).expect("建临时目录");
                Self { path }
            }
        }

        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(Path::new(&self.path));
            }
        }
    }

    fn write_version(env: &HostEnv, id: &str, json: &str) {
        let path = env.game.version_json(id);
        std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
        std::fs::write(&path, json).expect("写版本 JSON");
    }

    /// 26.2-snapshot-2 的库表里带 `lwjgl-vulkan`（与 `research/05-mc-vulkan.md` 一致）。
    const VULKAN_VERSION: &str = r#"{
        "id": "26.2-snapshot-2",
        "libraries": [{"name": "org.lwjgl:lwjgl-vulkan:3.4.3"}]
    }"#;

    #[test]
    fn gate_uses_core_capability_and_version_threshold() {
        let (env, _dir) = env();
        write_version(&env, "26.2-snapshot-2", VULKAN_VERSION);
        let (allowed, mc) = backend_gate(&env, "26.2-snapshot-2");
        assert!(allowed, "26.2-snapshot-2 应当放行");
        assert_eq!(mc, "26.2-snapshot-2");

        // 26.1：版本不够（即使库表里硬塞了 vulkan 也不放行——精确门限优先）。
        write_version(&env, "26.1", VULKAN_VERSION);
        let (allowed, mc) = backend_gate(&env, "26.1");
        assert!(!allowed, "26.1 不该放行");
        assert_eq!(mc, "26.1");

        // 没有版本 JSON：能力位全假 ⇒ 不放行（不 panic）。
        let (allowed, _) = backend_gate(&env, "1.20.4");
        assert!(!allowed);
    }

    #[test]
    fn write_then_read_round_trips_through_the_instance_file() {
        let (env, _dir) = env();
        let patch = LaunchArgsPatch {
            jvm_args: Some("  -XX:+UseG1GC  ".to_string()),
            game_args: Some("--fullscreen".to_string()),
            memory_mb: Some("6144".to_string()),
            java_path: Some("D:/jdk21/bin/java.exe".to_string()),
            graphics_backend: Some("vulkan".to_string()),
        };
        apply(&env, "inst-1", "1.20.4", &patch).expect("写回");
        let config = read_instance_config(&env.layout, "inst-1")
            .expect("读回")
            .expect("文件应存在");
        assert_eq!(config.launch.jvm_args.as_deref(), Some("-XX:+UseG1GC"));
        assert_eq!(config.launch.memory_mb, Some(6144));
        assert_eq!(
            config.launch.graphics_backend,
            Some(GraphicsBackend::Vulkan)
        );
        assert_eq!(config.version_id, "1.20.4");

        // 清空 = 跟随：`default` 与空串都把键抹掉。
        let clear = LaunchArgsPatch {
            jvm_args: Some(String::new()),
            game_args: Some("   ".to_string()),
            memory_mb: Some(String::new()),
            java_path: Some(String::new()),
            graphics_backend: Some("default".to_string()),
        };
        apply(&env, "inst-1", "1.20.4", &clear).expect("清空");
        let config = read_instance_config(&env.layout, "inst-1")
            .expect("读回")
            .expect("文件应存在");
        assert!(
            config.launch.is_empty(),
            "清空之后 launch 段应为空：{:?}",
            config.launch
        );
        let raw = config.to_json_string().expect("序列化");
        assert!(!raw.contains("\"launch\""), "空段不落盘：{raw}");
    }

    #[test]
    fn invalid_values_are_rejected_not_silently_dropped() {
        let (env, _dir) = env();
        let bad_backend = LaunchArgsPatch::backend("vulkan2");
        let error = apply(&env, "inst-1", "1.20.4", &bad_backend).expect_err("未知后端要报错");
        assert!(error.contains("vulkan2"), "{error}");

        let bad_memory = LaunchArgsPatch {
            memory_mb: Some("999999".to_string()),
            ..LaunchArgsPatch::default()
        };
        let error = apply(&env, "inst-1", "1.20.4", &bad_memory).expect_err("越界内存要报错");
        assert!(error.contains("256"), "{error}");

        let not_a_number = LaunchArgsPatch {
            memory_mb: Some("4g".to_string()),
            ..LaunchArgsPatch::default()
        };
        assert!(apply(&env, "inst-1", "1.20.4", &not_a_number).is_err());

        // 失败的写入不许留下半截文件。
        assert!(read_instance_config(&env.layout, "inst-1")
            .expect("读")
            .is_none());
    }

    #[test]
    fn view_reports_instance_layer_and_effective_values_apart() {
        let (env, _dir) = env();
        write_version(&env, "26.2-snapshot-2", VULKAN_VERSION);
        // 只填图形后端：其余各项都是「跟随」，生效值要给得出来。
        apply(
            &env,
            "inst-1",
            "26.2-snapshot-2",
            &LaunchArgsPatch::backend("vulkan"),
        )
        .expect("写回");
        let view = view(&env, "inst-1", "26.2-snapshot-2");
        assert_eq!(view.graphics_backend, "vulkan");
        assert_eq!(view.graphics_backend_effective, "vulkan");
        assert!(view.graphics_backend_allowed);
        assert_eq!(view.instance_id, "inst-1");
        assert_eq!(view.memory_mb, None);
        assert_eq!(
            view.memory_effective_mb,
            unlucky_core::game::launch_overrides::DEFAULT_MAX_MEMORY_MB
        );
        assert_eq!(view.graphics_backend_options.len(), 3);
    }
}
