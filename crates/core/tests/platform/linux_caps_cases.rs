//! Linux 探测：`/etc/os-release`、内核 release、会话类型、glibc 版本。

use std::collections::HashMap;

use crate::prelude::*;

const UBUNTU_OS_RELEASE: &str = "\
PRETTY_NAME=\"Ubuntu 24.04.1 LTS\"
NAME=\"Ubuntu\"
VERSION_ID=\"24.04\"
ID=ubuntu
";

#[test]
fn os_release_prefers_pretty_name() {
    assert_eq!(
        linux_caps::parse_os_release(UBUNTU_OS_RELEASE),
        Some("Ubuntu 24.04.1 LTS".to_string())
    );
    // 没有 PRETTY_NAME 的裁剪发行版：NAME + VERSION_ID 拼一个（不够精确但够日志用）。
    let minimal = "NAME=\"Alpine Linux\"\nVERSION_ID=3.19\n";
    assert_eq!(
        linux_caps::parse_os_release(minimal),
        Some("Alpine Linux 3.19".to_string())
    );
    assert_eq!(linux_caps::parse_os_release(""), None);
    assert_eq!(linux_caps::parse_os_release("ID=arch\n"), None);
}

#[test]
fn kernel_release_yields_major_minor() {
    assert_eq!(
        linux_caps::parse_kernel_version("6.8.0-45-generic"),
        Some((6, 8))
    );
    assert_eq!(linux_caps::parse_kernel_version("5.15.0"), Some((5, 15)));
    assert_eq!(linux_caps::parse_kernel_version("3.2"), Some((3, 2)));
    assert_eq!(linux_caps::parse_kernel_version("6.8"), Some((6, 8)));
    assert_eq!(linux_caps::parse_kernel_version(""), None);
    assert_eq!(linux_caps::parse_kernel_version("not-a-version"), None);
}

#[test]
fn session_detection_order_is_wayland_then_x11_then_headless() {
    fn detect(pairs: &[(&str, &str)]) -> SessionKind {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect();
        linux_caps::session_from_env(|key| map.get(key).cloned())
    }

    assert_eq!(
        detect(&[("WAYLAND_DISPLAY", "wayland-0")]),
        SessionKind::Wayland
    );
    assert_eq!(
        detect(&[("XDG_SESSION_TYPE", "wayland"), ("DISPLAY", ":0")]),
        SessionKind::Wayland
    );
    assert_eq!(
        detect(&[("XDG_SESSION_TYPE", "x11"), ("DISPLAY", ":0")]),
        SessionKind::X11
    );
    assert_eq!(detect(&[("DISPLAY", ":0")]), SessionKind::X11);
    // 空值不算有会话（SSH 转发里 `WAYLAND_DISPLAY=` 是常见的空串形态）。
    assert_eq!(
        detect(&[("WAYLAND_DISPLAY", "  "), ("DISPLAY", ":0")]),
        SessionKind::X11
    );
    assert_eq!(detect(&[]), SessionKind::Headless);
}

#[test]
fn session_kind_helpers() {
    assert!(SessionKind::Wayland.has_display());
    assert!(SessionKind::X11.has_display());
    assert!(!SessionKind::Headless.has_display());
    assert!(!SessionKind::Unknown.has_display());
    assert_eq!(SessionKind::Wayland.as_str(), "wayland");
    assert_eq!(SessionKind::Headless.as_str(), "headless");
    assert_eq!(GlProbe::Unknown.as_str(), "unknown");
    assert_eq!(GlProbe::Missing.as_str(), "missing");
}

#[test]
fn glibc_version_comes_from_the_first_ldd_line() {
    assert_eq!(
        linux_caps::parse_glibc_version(
            "ldd (Ubuntu GLIBC 2.39-0ubuntu8.3) 2.39\nCopyright (C) 2024\n"
        ),
        Some((2, 39))
    );
    assert_eq!(
        linux_caps::parse_glibc_version("ldd (GNU libc) 2.31"),
        Some((2, 31))
    );
    assert_eq!(
        linux_caps::parse_glibc_version("ldd (Debian GLIBC 2.36-9+deb12u4) 2.36"),
        Some((2, 36))
    );
    // musl / 命令不存在：不能瞎猜，交给上层「不阻塞」。
    assert_eq!(linux_caps::parse_glibc_version("ldd: not found"), None);
    assert_eq!(linux_caps::parse_glibc_version(""), None);
}

#[test]
fn format_version_is_dotted() {
    assert_eq!(linux_caps::format_version((2, 31)), "2.31");
    assert_eq!(linux_caps::format_version((6, 8)), "6.8");
}

#[test]
fn gl_probe_is_a_hook_returning_unknown_for_now() {
    // P1 只留钩子：真探测要 dlopen（FFI），core 禁 unsafe，故恒为 Unknown 且不阻塞。
    assert_eq!(linux_caps::probe_gl(), GlProbe::Unknown);
}

#[test]
fn caps_probe_matches_the_host() {
    let caps: LinuxCaps = linux_caps::probe();
    if cfg!(target_os = "linux") {
        // 容器里可能全空，但会话类型至少是「探过了」。
        assert_ne!(caps.session, SessionKind::Unknown);
        if let Some(kernel) = &caps.kernel {
            assert!(
                caps.kernel_version.is_some(),
                "内核串 {kernel} 应能取 major.minor"
            );
        }
    } else {
        assert_eq!(caps.session, SessionKind::Unknown);
        assert!(caps.kernel.is_none());
        assert!(caps.distro.is_none());
    }
}
