//! macOS 探测：`sw_vers -productVersion` 与 `sysctl hw.optional.arm64`。

use crate::prelude::*;

#[test]
fn product_version_parses_sw_vers_output() {
    assert_eq!(
        macos_version::parse_product_version("12.3.1\n"),
        Some((12, 3, 1))
    );
    assert_eq!(
        macos_version::parse_product_version("12.3"),
        Some((12, 3, 0))
    );
    assert_eq!(macos_version::parse_product_version("12"), Some((12, 0, 0)));
    assert_eq!(
        macos_version::parse_product_version("  14.2.1  "),
        Some((14, 2, 1))
    );
    assert_eq!(macos_version::parse_product_version(""), None);
    assert_eq!(macos_version::parse_product_version("ProductVersion"), None);
}

#[test]
fn sysctl_arm64_flag() {
    assert!(macos_version::parse_sysctl_bool("1\n"));
    assert!(!macos_version::parse_sysctl_bool("0\n"));
    // Intel 机器上 sysctl 以非零码退出 → stdout 为空 → 不是 arm64。
    assert!(!macos_version::parse_sysctl_bool(""));
}

#[test]
fn version_strings_and_runtime_arch() {
    let apple_silicon = MacOsVersion::new(12, 3, 1, true);
    assert_eq!(apple_silicon.version_string(), "12.3.1");
    assert_eq!(apple_silicon.runtime_arch(), "arm64");
    assert_eq!(apple_silicon.display_name(), "macOS 12.3.1 (arm64)");

    // Rosetta 2 下编译目标是 x86_64，但真实架构仍是 arm64：
    // 探到 arm64 时 runtime_arch 必须是 arm64（MC 原生库与渲染路径都按它选）。
    let rosetta = MacOsVersion::new(12, 3, 1, true);
    assert_eq!(rosetta.runtime_arch(), "arm64");

    let intel = MacOsVersion::new(15, 0, 0, false);
    assert_eq!(intel.runtime_arch(), "x86_64");
    assert_eq!(intel.display_name(), "macOS 15.0.0 (x86_64)");
}

#[test]
fn probe_matches_the_host() {
    let probed = macos_version::probe();
    if cfg!(target_os = "macos") {
        let version = probed.expect("macOS 宿主必须能探到产品版本");
        assert!(version.major >= 10, "探到 {version:?}");
    } else {
        assert!(probed.is_none(), "非 macOS 平台不应探到 macOS 版本");
    }
}
