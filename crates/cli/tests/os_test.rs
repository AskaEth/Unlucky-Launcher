//! D19/D21 支持基线：core 的纯判定函数 + CLI 的呈现层（注入样本，不动真机）。

use unlucky_cli::os;
use unlucky_core::platform::linux_caps::{GlProbe, SessionKind};
use unlucky_core::platform::os_info::{
    self, linux_level, macos_level, support_level_for, windows_level, OsFamily, OsInfo,
    SupportLevel,
};
use unlucky_core::platform::vk_probe::VkProbe;

/// 手工构造一份样本系统信息（字段全是探测事实，等级用 core 的纯函数算）。
fn sample(family: OsFamily, version: Option<(u32, u32)>, product: &str) -> OsInfo {
    let version_string =
        version.map_or_else(String::new, |(major, minor)| format!("{major}.{minor}"));
    OsInfo {
        family,
        arch: "x86_64".to_string(),
        major_minor: version,
        build: None,
        product: product.to_string(),
        version_string,
        session: SessionKind::Unknown,
        gl: GlProbe::Unknown,
        vk: VkProbe::Unknown,
        kernel: None,
        glibc: None,
        support_level: support_level_for(family, version, None, None),
    }
}

#[test]
fn windows_samples_below_win10_are_unsupported() {
    // D21 用户拍板「直接 win10 起步」：Win8.1 / 8 / 7 及更旧一律不支持。
    assert_eq!(windows_level(Some((6, 1))), SupportLevel::Unsupported); // Win7
    assert_eq!(windows_level(Some((6, 2))), SupportLevel::Unsupported); // Win8
    assert_eq!(windows_level(Some((6, 3))), SupportLevel::Unsupported); // Win8.1
    assert_eq!(windows_level(Some((5, 1))), SupportLevel::Unsupported); // XP
    assert_eq!(windows_level(Some((10, 0))), SupportLevel::One); // Win10

    // 探不到版本 → 保守判不支持（不能因为"探不到"就放行）。
    assert_eq!(windows_level(None), SupportLevel::Unsupported);
}

#[test]
fn macos_and_linux_samples() {
    assert_eq!(macos_level(Some(11)), SupportLevel::Unsupported);
    assert_eq!(macos_level(Some(12)), SupportLevel::One);
    assert_eq!(macos_level(None), SupportLevel::Unsupported);
    // Linux：内核/glibc 探得到但过低 → 不支持；取不到（容器/musl/Termux）不阻塞。
    assert_eq!(linux_level(Some((2, 6)), None), SupportLevel::Unsupported);
    assert_eq!(linux_level(None, Some((2, 12))), SupportLevel::Unsupported);
    assert_eq!(linux_level(Some((6, 8)), Some((2, 39))), SupportLevel::One);
    assert_eq!(linux_level(None, None), SupportLevel::One);
}

#[test]
fn unsupported_sample_renders_upgrade_hint() {
    let info = sample(OsFamily::Windows, Some((6, 3)), "Windows 8.1");

    assert!(!info.support_level.is_usable());
    let line = os::summary_line(&info);
    assert!(line.contains("Windows 8.1"), "{line}");
    assert!(line.contains("6.3"), "{line}");
    assert!(line.contains("不支持"), "{line}");

    let notes = os::notes(&info);
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains("Windows 10 或更高版本"), "{notes:?}");

    // 文案是中英双语（core 同一份措辞），脚本只读 --json。
    let message = info.describe().combined();
    assert!(message.contains("Windows 10 或更高版本"), "{message}");
    assert!(message.contains("Windows 10 or newer"), "{message}");

    let value = os::to_json(&info);
    assert_eq!(value["family"], "windows");
    assert_eq!(value["product"], "Windows 8.1");
    assert_eq!(value["support"], "unsupported");
    assert_eq!(value["major_minor"], "6.3");
}

#[test]
fn supported_sample_has_no_notes() {
    let info = sample(OsFamily::Windows, Some((10, 0)), "Windows 11");
    assert!(info.support_level.is_usable());
    assert!(os::notes(&info).is_empty());
    assert!(os::summary_line(&info).contains("满足最低要求"));
    assert_eq!(os::to_json(&info)["support"], "one");
}

#[test]
fn missing_version_is_reported_as_null_not_zero() {
    // 探不到版本时 `version` 是 null（脚本按 null 判定"未探测"），而不是空串。
    let info = sample(OsFamily::Windows, None, "Windows");
    let value = os::to_json(&info);
    assert!(value["version"].is_null());
    assert!(value["major_minor"].is_null());
    assert_eq!(value["support"], "unsupported");
    assert!(os::summary_line(&info).contains("版本未知"));
}

#[test]
fn real_detect_matches_core_cache() {
    // 真机上 CLI 的摘要必须就是 core 的缓存快照（同一份事实，不做二次加工）。
    let info = os::detect();
    assert_eq!(&info, os_info::current());
    assert!(!info.arch.is_empty());
    assert!(!info.product.is_empty());
    // 自检形态与事实一致。
    let check = os::check();
    assert_eq!(check.level, info.support_level);
    assert_eq!(check.os, info);
    assert_eq!(check.is_usable(), info.support_level.is_usable());
    assert!(check.message.contains(&info.product), "{}", check.message);
}
