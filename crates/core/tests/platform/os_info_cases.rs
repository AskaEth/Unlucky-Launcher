//! 支持等级映射与双语文案：全部用注入式假数据，不改机器真实版本。
//!
//! 基线（用户拍板）：Windows 10+ / macOS 12+ / Linux 达标 → 一等，其余一律不支持；
//! Win8.1/8/7 也不承诺 CLI，所以没有中间档。

use crate::prelude::*;

fn windows_info(major_minor: Option<(u32, u32)>) -> OsInfo {
    let product = match major_minor {
        Some(version) if version >= os_info::MIN_WINDOWS => "Windows 10",
        Some(_) => "Windows 7",
        None => "Windows",
    };
    OsInfo {
        family: OsFamily::Windows,
        arch: "x86_64".to_string(),
        major_minor,
        build: Some(19_045),
        product: product.to_string(),
        version_string: "10.0.19045".to_string(),
        session: SessionKind::Unknown,
        gl: GlProbe::Unknown,
        vk: VkProbe::Unknown,
        kernel: None,
        glibc: None,
        support_level: os_info::support_level_for(OsFamily::Windows, major_minor, None, None),
    }
}

fn linux_info(kernel: Option<(u32, u32)>, glibc: Option<(u32, u32)>) -> OsInfo {
    OsInfo {
        family: OsFamily::Linux,
        arch: "x86_64".to_string(),
        major_minor: kernel,
        build: None,
        product: "Ubuntu 24.04.1 LTS".to_string(),
        version_string: "6.8.0-45-generic".to_string(),
        session: SessionKind::Wayland,
        gl: GlProbe::Unknown,
        vk: VkProbe::Unknown,
        kernel,
        glibc,
        support_level: os_info::support_level_for(OsFamily::Linux, kernel, kernel, glibc),
    }
}

#[test]
fn windows_levels_require_windows_10() {
    // Win10 / Win11（都报 10.0，靠 build 区分产品名）→ 一等。
    assert_eq!(os_info::windows_level(Some((10, 0))), SupportLevel::One);
    // Win8.1 / Win8 / Win7：低于基线，GUI 与 CLI 都不承诺。
    assert_eq!(
        os_info::windows_level(Some((6, 3))),
        SupportLevel::Unsupported
    );
    assert_eq!(
        os_info::windows_level(Some((6, 2))),
        SupportLevel::Unsupported
    );
    assert_eq!(
        os_info::windows_level(Some((6, 1))),
        SupportLevel::Unsupported
    );
    // Vista / XP。
    assert_eq!(
        os_info::windows_level(Some((6, 0))),
        SupportLevel::Unsupported
    );
    assert_eq!(
        os_info::windows_level(Some((5, 1))),
        SupportLevel::Unsupported
    );
    // 探测失败 → 保守（无法确认 Win10+ 就不放行）。
    assert_eq!(os_info::windows_level(None), SupportLevel::Unsupported);
}

#[test]
fn macos_levels_use_twelve_as_the_floor() {
    assert_eq!(os_info::macos_level(Some(12)), SupportLevel::One);
    assert_eq!(os_info::macos_level(Some(15)), SupportLevel::One);
    assert_eq!(os_info::macos_level(Some(11)), SupportLevel::Unsupported);
    assert_eq!(os_info::macos_level(None), SupportLevel::Unsupported);
}

#[test]
fn linux_levels_gate_on_kernel_and_glibc() {
    assert_eq!(
        os_info::linux_level(Some((6, 8)), Some((2, 39))),
        SupportLevel::One
    );
    assert_eq!(
        os_info::linux_level(Some((2, 6)), Some((2, 39))),
        SupportLevel::Unsupported
    );
    assert_eq!(
        os_info::linux_level(Some((6, 8)), Some((2, 16))),
        SupportLevel::Unsupported
    );
    // musl / Termux / 容器里取不到 glibc 与内核 → 不阻塞。
    assert_eq!(os_info::linux_level(None, None), SupportLevel::One);
    assert_eq!(os_info::linux_level(Some((3, 2)), None), SupportLevel::One);
    // 恰好等于下限也算支持。
    assert_eq!(
        os_info::linux_level(Some((6, 8)), Some(os_info::MIN_GLIBC)),
        SupportLevel::One
    );
}

#[test]
fn support_level_for_dispatches_by_family() {
    assert_eq!(
        os_info::support_level_for(OsFamily::Windows, Some((10, 0)), None, None),
        SupportLevel::One
    );
    assert_eq!(
        os_info::support_level_for(OsFamily::Windows, Some((6, 1)), None, None),
        SupportLevel::Unsupported
    );
    assert_eq!(
        os_info::support_level_for(OsFamily::MacOs, Some((12, 3)), None, None),
        SupportLevel::One
    );
    assert_eq!(
        os_info::support_level_for(OsFamily::MacOs, Some((11, 7)), None, None),
        SupportLevel::Unsupported
    );
    // Linux 的 major_minor 不参与判定，看 kernel/glibc。
    assert_eq!(
        os_info::support_level_for(OsFamily::Linux, Some((10, 0)), Some((2, 6)), None),
        SupportLevel::Unsupported
    );
    assert_eq!(
        os_info::support_level_for(OsFamily::Linux, None, Some((6, 8)), Some((2, 39))),
        SupportLevel::One
    );
}

#[test]
fn windows_7_is_reported_as_unsupported_with_the_new_baseline() {
    let info = windows_info(Some((6, 1)));
    assert_eq!(info.support_level, SupportLevel::Unsupported);
    let text = info.describe();
    assert!(text.zh.contains("不支持"), "zh: {}", text.zh);
    assert!(text.zh.contains("Windows 10"), "zh: {}", text.zh);
    // 不再有「受限模式 / HeadlessOnly」文案。
    assert!(!text.zh.contains("受限模式"), "zh: {}", text.zh);
    assert!(text.en.contains("Unsupported"), "en: {}", text.en);
    assert!(text.en.contains("Windows 10 or newer"), "en: {}", text.en);
    // combined() 是 CLI 打印形态：中文在前、英文在后。
    let combined = text.combined();
    let (zh, en) = combined.split_once('\n').expect("两行");
    assert!(zh.contains("不支持"));
    assert!(en.contains("Unsupported"));
}

#[test]
fn unsupported_macos_and_linux_name_their_own_minimums() {
    let mac = OsInfo {
        family: OsFamily::MacOs,
        arch: "arm64".to_string(),
        major_minor: Some((11, 7)),
        build: None,
        product: "macOS 11.7.0 (arm64)".to_string(),
        version_string: "11.7.0".to_string(),
        session: SessionKind::Unknown,
        gl: GlProbe::Unknown,
        vk: VkProbe::Unknown,
        kernel: None,
        glibc: None,
        support_level: SupportLevel::Unsupported,
    };
    let text = mac.describe();
    assert!(text.zh.contains("macOS 12"), "zh: {}", text.zh);
    assert!(text.en.contains("macOS 12"), "en: {}", text.en);
    assert!(!text.zh.contains("Windows"), "zh: {}", text.zh);

    let linux = linux_info(Some((2, 6)), None);
    assert_eq!(linux.support_level, SupportLevel::Unsupported);
    let text = linux.describe();
    assert!(text.zh.contains("内核"), "zh: {}", text.zh);
    assert!(text.en.contains("kernel"), "en: {}", text.en);
}

#[test]
fn unknown_windows_version_is_unsupported_and_says_so() {
    let info = windows_info(None);
    assert_eq!(info.support_level, SupportLevel::Unsupported);
    let text = info.describe();
    assert!(text.zh.contains("未能探测到系统版本"), "zh: {}", text.zh);
    assert!(text.zh.contains("Windows 10"), "zh: {}", text.zh);
}

#[test]
fn supported_linux_without_display_adds_a_hint() {
    let mut info = linux_info(Some((6, 8)), Some((2, 39)));
    info.session = SessionKind::Headless;
    let text = info.describe();
    assert!(text.zh.contains("图形会话"), "zh: {}", text.zh);
    assert!(text.en.contains("graphical session"), "en: {}", text.en);

    // 有显示服务器时不加提示。
    let text = linux_info(Some((6, 8)), Some((2, 39))).describe();
    assert!(!text.zh.contains("图形会话"), "zh: {}", text.zh);
}

#[test]
fn summary_line_carries_the_diagnostic_fields() {
    let line = linux_info(Some((6, 8)), Some((2, 39))).summary_line();
    assert!(line.contains("os=linux"), "{line}");
    assert!(line.contains("version=6.8.0-45-generic"), "{line}");
    assert!(line.contains("arch=x86_64"), "{line}");
    assert!(line.contains("level=one"), "{line}");
    assert!(line.contains("session=wayland"), "{line}");
    assert!(line.contains("gl=unknown"), "{line}");
    assert!(line.contains("vk=unknown"), "{line}");
}

#[test]
fn summary_line_reports_the_vulkan_probe() {
    let mut info = linux_info(Some((6, 8)), Some((2, 39)));
    info.vk = VkProbe::LoaderOnly;
    assert!(info.summary_line().contains("vk=loader_only"));
    info.vk = VkProbe::Available;
    assert!(info.summary_line().contains("vk=available"));
    info.vk = VkProbe::Missing;
    assert!(info.summary_line().contains("vk=missing"));
}

#[test]
fn summary_line_marks_missing_version_as_unknown() {
    let mut info = windows_info(None);
    info.version_string = String::new();
    let line = info.summary_line();
    assert!(line.contains("version=unknown"), "{line}");
    assert!(line.contains("level=unsupported"), "{line}");
}

#[test]
fn support_level_helpers() {
    assert_eq!(SupportLevel::One.as_str(), "one");
    assert_eq!(SupportLevel::Unsupported.as_str(), "unsupported");
    assert!(SupportLevel::One.is_usable());
    assert!(!SupportLevel::Unsupported.is_usable());
}

#[test]
fn os_family_facts_and_launch_reexport_are_the_same_type() {
    // launch::platform 只是把 OsFamily 从 platform::os_info 再导出（避免两套枚举）。
    let family: unlucky_core::platform::os_info::OsFamily = OsFamily::Windows;
    assert_eq!(family, OsFamily::Windows);
    assert_eq!(OsFamily::Windows.manifest_name(), "windows");
    assert_eq!(OsFamily::MacOs.manifest_name(), "osx");
    assert_eq!(OsFamily::Linux.manifest_name(), "linux");
    assert!(!OsFamily::Windows.is_posix());
    assert!(OsFamily::Linux.is_posix());
    assert!(!OsFamily::current().manifest_arch().is_empty());
    assert_eq!(
        OsFamily::current().manifest_arch(),
        OsFamily::Linux.manifest_arch()
    );
}

#[test]
fn detect_reports_the_host_it_runs_on() {
    let info = os_info::detect();
    // 家族与当前编译目标一致。
    assert_eq!(info.family, OsFamily::current());
    assert!(!info.arch.is_empty());
    assert!(!info.product.is_empty());
    // 缓存入口返回同一份事实。
    let cached = os_info::current();
    assert_eq!(cached, &info);
    assert_eq!(cached.family, info.family);
}

#[test]
fn windows_host_probes_a_real_version() {
    if !cfg!(target_os = "windows") {
        return;
    }
    let info = os_info::detect();
    let (major, _minor) = info.major_minor.expect("Windows 上必须探到 major.minor");
    assert!(major >= 6, "探到的版本不合理: {:?}", info.major_minor);
    assert!(!info.os_version().is_empty(), "os_version 不能是空串");
    assert!(info.build.is_some(), "Windows 上应有 build 号");
    // 非 Windows 分支的事实不应泄漏进来。
    assert!(info.kernel.is_none());
    assert!(info.glibc.is_none());
    assert_eq!(info.session, SessionKind::Unknown);
}

#[test]
fn check_returns_level_message_and_os_info() {
    let check: SupportCheck = os_info::check();
    assert_eq!(check.level, os_info::current().support_level);
    assert_eq!(check.is_usable(), check.level.is_usable());
    assert!(check.message.contains('\n'), "文案应为中英双语多行");
    assert_eq!(check.message, check.os.describe().combined());
}
