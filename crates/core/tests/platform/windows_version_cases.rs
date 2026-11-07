//! Windows 版本探测：注册表 / `ver` 输出的解析 + 本机真实探测。

use crate::prelude::*;

/// Win11（`CurrentMajorVersionNumber` 是 REG_DWORD，`reg query` 输出 0x..）。
const WIN11_REG: &str = "\
HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion
    CurrentBuildNumber    REG_SZ    22631
    CurrentMajorVersionNumber    REG_DWORD    0xa
    CurrentMinorVersionNumber    REG_DWORD    0x0
    CurrentVersion    REG_SZ    6.3
    ProductName    REG_SZ    Windows 10 Pro
";

/// Win10：数字键存在，`CurrentVersion` 是过期的 6.3，不能拿它当版本。
const WIN10_REG: &str = "\
HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion
    CurrentBuildNumber    REG_SZ    19045
    CurrentMajorVersionNumber    REG_DWORD    0xa
    CurrentMinorVersionNumber    REG_DWORD    0x0
    CurrentVersion    REG_SZ    6.3
";

/// Win7：没有数字键，只有 `CurrentVersion` 字符串（`CurrentBuildNumber` Win7 也有）。
const WIN7_REG: &str = "\
HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion
    CurrentBuildNumber    REG_SZ    7601
    CurrentVersion    REG_SZ    6.1
    ProductName    REG_SZ    Windows 7 Ultimate
";

#[test]
fn reg_query_parses_windows_11() {
    let version = windows_version::parse_reg_query(WIN11_REG).expect("解析失败");
    assert_eq!(version, WindowsVersion::new(10, 0, 22631));
    assert_eq!(version.version_string(), "10.0.22631");
    assert_eq!(version.display_name(), "Windows 11");
}

#[test]
fn reg_query_prefers_numeric_keys_over_stale_current_version() {
    let version = windows_version::parse_reg_query(WIN10_REG).expect("解析失败");
    assert_eq!(version, WindowsVersion::new(10, 0, 19045));
    assert_eq!(version.display_name(), "Windows 10");
}

#[test]
fn reg_query_falls_back_to_current_version_on_windows_7() {
    let version = windows_version::parse_reg_query(WIN7_REG).expect("解析失败");
    assert_eq!(version, WindowsVersion::new(6, 1, 7601));
    assert_eq!(version.major_minor(), (6, 1));
    assert_eq!(version.display_name(), "Windows 7");
}

#[test]
fn reg_query_without_build_number_is_unparsable() {
    let text = "    CurrentVersion    REG_SZ    6.1\n";
    assert_eq!(windows_version::parse_reg_query(text), None);
    // 空输出（reg 不可用 / 被策略禁用）也不能崩。
    assert_eq!(windows_version::parse_reg_query(""), None);
    assert_eq!(
        windows_version::parse_reg_query("ERROR: 找不到指定的注册表项"),
        None
    );
}

#[test]
fn ver_output_parses_english_and_chinese() {
    let english =
        windows_version::parse_ver_output("Microsoft Windows [Version 10.0.19045.4046]\r\n")
            .expect("解析失败");
    assert_eq!(english, WindowsVersion::new(10, 0, 19045));

    let chinese = windows_version::parse_ver_output(
        "Microsoft Windows [\u{7248}\u{672c} 10.0.19045.4046]\r\n",
    )
    .expect("解析失败");
    assert_eq!(chinese, english);

    let win7 = windows_version::parse_ver_output("Microsoft Windows [Version 6.1.7601]")
        .expect("解析失败");
    assert_eq!(win7, WindowsVersion::new(6, 1, 7601));
    assert_eq!(win7.display_name(), "Windows 7");
}

#[test]
fn ver_output_rejects_garbage() {
    assert_eq!(windows_version::parse_ver_output(""), None);
    assert_eq!(windows_version::parse_ver_output("no brackets here"), None);
    assert_eq!(
        windows_version::parse_ver_output("Microsoft Windows [Version ]"),
        None
    );
}

#[test]
fn build_number_maps_to_version_families() {
    // 快路径：单值查询只给 build，major.minor 由 build 族反推。
    assert_eq!(
        windows_version::from_build(7601),
        Some(WindowsVersion::new(6, 1, 7601))
    );
    assert_eq!(
        windows_version::from_build(9200).map(|v| v.major_minor()),
        Some((6, 2))
    );
    assert_eq!(
        windows_version::from_build(9600).map(|v| v.major_minor()),
        Some((6, 3))
    );
    assert_eq!(
        windows_version::from_build(19045).map(|v| v.major_minor()),
        Some((10, 0))
    );
    assert_eq!(
        windows_version::from_build(22631).map(|v| v.display_name()),
        Some("Windows 11")
    );
    assert_eq!(
        windows_version::from_build(26100).map(|v| v.major_minor()),
        Some((10, 0))
    );
    // Vista 之前不猜：交给整键/`ver` 解析兜底。
    assert_eq!(windows_version::from_build(3790), None);
    assert_eq!(windows_version::from_build(0), None);
}

#[test]
fn single_value_query_yields_build_number() {
    let text = "\
HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion
    CurrentBuildNumber    REG_SZ    26100
";
    assert_eq!(windows_version::parse_build_number(text), Some(26100));
    assert_eq!(
        windows_version::parse_build_number("ERROR: \u{627e}\u{4e0d}\u{5230}\u{6307}\u{5b9a}\u{7684}\u{6ce8}\u{518c}\u{8868}\u{9879}"),
        None
    );
    assert_eq!(windows_version::parse_build_number(""), None);
}

#[test]
fn display_name_covers_the_matrix() {
    assert_eq!(
        WindowsVersion::new(10, 0, 22_000).display_name(),
        "Windows 11"
    );
    assert_eq!(
        WindowsVersion::new(10, 0, 21_999).display_name(),
        "Windows 10"
    );
    assert_eq!(
        WindowsVersion::new(6, 3, 9600).display_name(),
        "Windows 8.1"
    );
    assert_eq!(WindowsVersion::new(6, 2, 9200).display_name(), "Windows 8");
    assert_eq!(WindowsVersion::new(6, 1, 7601).display_name(), "Windows 7");
    assert_eq!(
        WindowsVersion::new(6, 0, 6002).display_name(),
        "Windows Vista"
    );
    assert_eq!(WindowsVersion::new(5, 1, 2600).display_name(), "Windows XP");
}

#[test]
fn probe_matches_the_host() {
    let probed = windows_version::probe();
    if cfg!(target_os = "windows") {
        let version = probed.expect("Windows 宿主必须能探到真实版本");
        // 真实的 Windows 至少是 6.x（XP 是 5.x，不会被这个仓库支持）。
        assert!(version.major >= 6, "探到 {version:?}");
        assert!(!version.display_name().is_empty());
        // 重新探一次结果稳定（reg 与 ver 两条路径应给同一个 major.minor）。
        let again = windows_version::probe().expect("二次探测失败");
        assert_eq!(again.major_minor(), version.major_minor());
    } else {
        assert!(probed.is_none(), "非 Windows 平台不应探到 Windows 版本");
    }
}
