//! 测试共用设施（crate 名只出现在这里）。

use std::path::PathBuf;

pub use unlucky_core::launch::args_table::assemble;
pub use unlucky_core::launch::argument::{ArgumentEntry, OsRule, Rule, RuleContext};
pub use unlucky_core::launch::game_version::GameVersion;
pub use unlucky_core::launch::platform::{OsFamily, PlatformCtx};
pub use unlucky_core::launch::request::*;
pub use unlucky_core::platform::linux_caps::{self, GlProbe, LinuxCaps, SessionKind};
pub use unlucky_core::platform::macos_version::{self, MacOsVersion};
pub use unlucky_core::platform::os_info::{self, OsInfo, SupportCheck, SupportLevel};
pub use unlucky_core::platform::vk_probe::{
    classify, count_icd_manifests, env_icd_path, ldconfig_has_libvulkan, loader_candidates,
    probe_vk, registry_has_driver_entry, VkEvidence, VkProbe, ICD_DIRS, ICD_ENV_VARS,
    VULKAN_DRIVERS_KEY,
};
pub use unlucky_core::platform::windows_version::{self, WindowsVersion};

/// 一个最小可组装请求：现代清单，游戏参数由调用方给。
pub fn request_with_game(game: Vec<ArgumentEntry>) -> LaunchRequest {
    LaunchRequest {
        instance: InstanceInfo {
            id: "inst-1".to_string(),
            name: "1.20.1".to_string(),
            version: GameVersion::parse("1.20.1"),
            version_type: "release".to_string(),
        },
        launcher: LauncherInfo {
            name: "UnluckyLauncher".to_string(),
            version: "0.1.0".to_string(),
        },
        java: JavaSelection::new("C:/java/bin/java.exe", 17, 64),
        manifest: ManifestArgs {
            main_class: Some("net.minecraft.client.main.Main".to_string()),
            game,
            ..ManifestArgs::default()
        },
        classpath: vec![PathBuf::from("C:/mc/libraries/a.jar")],
        ..LaunchRequest::default()
    }
}

/// 一条带 `rules.os.version` 的游戏参数项。
pub fn version_ruled_entry(pattern: &str, value: &str) -> ArgumentEntry {
    let mut rule = Rule::allow();
    rule.os = Some(OsRule {
        name: None,
        arch: None,
        version: Some(pattern.to_string()),
    });
    ArgumentEntry::ruled(vec![rule], [value])
}
