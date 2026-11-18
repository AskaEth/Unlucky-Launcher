//! 目标平台与包形态的清单写法解析。

use crate::platform::os_info::OsFamily;
use crate::update::{Arch, PackageFormat, Target};

#[test]
fn parses_canonical_targets() {
    assert_eq!(
        Target::parse("win-x64"),
        Some(Target::new(OsFamily::Windows, Arch::X64))
    );
    assert_eq!(
        Target::parse("mac-arm64"),
        Some(Target::new(OsFamily::MacOs, Arch::Arm64))
    );
    assert_eq!(
        Target::parse("linux-x64"),
        Some(Target::new(OsFamily::Linux, Arch::X64))
    );
    assert_eq!(
        Target::parse("linux-arm64"),
        Some(Target::new(OsFamily::Linux, Arch::Arm64))
    );
}

#[test]
fn tolerates_common_aliases() {
    assert_eq!(
        Target::parse("windows-amd64"),
        Some(Target::new(OsFamily::Windows, Arch::X64))
    );
    assert_eq!(
        Target::parse("osx-aarch64"),
        Some(Target::new(OsFamily::MacOs, Arch::Arm64))
    );
    assert_eq!(
        Target::parse("darwin-universal"),
        None,
        "认不出的架构必须返回 None，不能瞎猜"
    );
    assert_eq!(Target::parse("win"), None, "少了架构段");
    assert_eq!(Target::parse("solaris-x64"), None);
    assert_eq!(Target::parse(""), None);
}

#[test]
fn current_target_renders_and_round_trips() {
    let current = Target::current();
    assert_eq!(Target::parse(current.as_str()), Some(current));
    assert_eq!(current.to_string(), current.as_str());
    assert_ne!(current.arch, Arch::Unknown, "编译目标架构应能识别");
}

#[test]
fn parses_package_formats() {
    assert_eq!(PackageFormat::parse("msi"), Some(PackageFormat::Msi));
    assert_eq!(PackageFormat::parse("DMG"), Some(PackageFormat::Dmg));
    assert_eq!(
        PackageFormat::parse("appimage"),
        Some(PackageFormat::AppImage)
    );
    assert_eq!(
        PackageFormat::parse("portable-zip"),
        Some(PackageFormat::PortableZip)
    );
    assert_eq!(
        PackageFormat::parse("zip"),
        Some(PackageFormat::PortableZip)
    );
    assert_eq!(PackageFormat::parse("tar.gz"), None);
    assert_eq!(PackageFormat::Msi.to_string(), "msi");
}

#[test]
fn installer_formats_are_flagged() {
    for format in [
        PackageFormat::Msi,
        PackageFormat::Dmg,
        PackageFormat::Deb,
        PackageFormat::Rpm,
    ] {
        assert!(format.needs_installer(), "{format}");
    }
    for format in [
        PackageFormat::PortableZip,
        PackageFormat::PortableExe,
        PackageFormat::AppImage,
    ] {
        assert!(!format.needs_installer(), "{format}");
    }
}
