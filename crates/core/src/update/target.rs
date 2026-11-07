//! 产物目标平台与包格式：清单里 `target` / `format` 两个字段的取值。
//!
//! 系统家族直接复用 [`OsFamily`]（`core::platform` 的口径），这里不另立一套
//! Windows/macOS/Linux 枚举——否则同一件事会有两个真相源。
//!
//! `target` 的清单写法是 `系统-架构`，例如 `win-x64` / `mac-arm64` / `linux-arm64`。
//! 解析容忍常见别名（`windows` / `macos` / `osx`、`x86_64` / `amd64`、`aarch64`），
//! 因为清单是人（或 CI 脚本）拼出来的，别让 `win-amd64` 这种写法静默变成「没有可用产物」。

use std::fmt;

use crate::platform::os_info::OsFamily;

/// 产物架构。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arch {
    /// x86_64 / amd64。
    X64,
    /// aarch64 / arm64。
    Arm64,
    /// i686 / x86 32 位。
    X86,
    /// 编译目标认不出的架构（对照组永远匹配不上，于是走「本平台没有产物」那条路）。
    Unknown,
}

impl Arch {
    /// 清单写法。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
            Self::X86 => "x86",
            Self::Unknown => "unknown",
        }
    }

    /// 当前编译目标的架构。
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_arch = "x86_64") {
            Self::X64
        } else if cfg!(target_arch = "aarch64") {
            Self::Arm64
        } else if cfg!(target_arch = "x86") {
            Self::X86
        } else {
            Self::Unknown
        }
    }

    /// 解析清单写法（容忍 `x86_64` / `amd64` / `aarch64` / `i686` 等别名）。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "x64" | "x86_64" | "x86-64" | "amd64" => Some(Self::X64),
            "arm64" | "aarch64" | "arm64e" => Some(Self::Arm64),
            "x86" | "i686" | "i386" | "x86_32" => Some(Self::X86),
            _ => None,
        }
    }
}

/// 产物目标平台（系统 + 架构）。
///
/// 没派生 `Hash`/`Ord`：`OsFamily` 尚未派生这两个 trait（`core::platform` 不在本 mission
/// 的改动范围内），需要当 Map 键时再回头补，别在这里另造一套系统枚举绕过去。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    /// 系统家族。
    pub os: OsFamily,
    /// 架构。
    pub arch: Arch,
}

impl Target {
    /// 构造。
    #[must_use]
    pub const fn new(os: OsFamily, arch: Arch) -> Self {
        Self { os, arch }
    }

    /// 当前编译目标。
    #[must_use]
    pub fn current() -> Self {
        Self::new(OsFamily::current(), Arch::current())
    }

    /// 解析清单写法（`系统-架构`）。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (os, arch) = text.trim().split_once('-')?;
        Some(Self::new(parse_os(os)?, Arch::parse(arch)?))
    }

    /// 清单写法。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match (self.os, self.arch) {
            (OsFamily::Windows, Arch::X64) => "win-x64",
            (OsFamily::Windows, Arch::Arm64) => "win-arm64",
            (OsFamily::Windows, Arch::X86) => "win-x86",
            (OsFamily::Windows, Arch::Unknown) => "win-unknown",
            (OsFamily::MacOs, Arch::X64) => "mac-x64",
            (OsFamily::MacOs, Arch::Arm64) => "mac-arm64",
            (OsFamily::MacOs, Arch::X86) => "mac-x86",
            (OsFamily::MacOs, Arch::Unknown) => "mac-unknown",
            (OsFamily::Linux, Arch::X64) => "linux-x64",
            (OsFamily::Linux, Arch::Arm64) => "linux-arm64",
            (OsFamily::Linux, Arch::X86) => "linux-x86",
            (OsFamily::Linux, Arch::Unknown) => "linux-unknown",
        }
    }
}

fn parse_os(text: &str) -> Option<OsFamily> {
    match text.trim().to_ascii_lowercase().as_str() {
        "win" | "windows" => Some(OsFamily::Windows),
        "mac" | "macos" | "osx" | "darwin" => Some(OsFamily::MacOs),
        "linux" => Some(OsFamily::Linux),
        _ => None,
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 安装包形态。
///
/// 只区分「安装器装出来的」与「解压即用的」两大类，具体怎么装由打包层决定：
/// 安装器装的实例（msi/dmg/deb/rpm）本模块**不主张**原地覆盖（见 `install` 模块文档），
/// 便携形态（portable-zip / appimage / portable-exe）才走自替换。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PackageFormat {
    /// Windows 安装器。
    Msi,
    /// macOS 磁盘映像。
    Dmg,
    /// Linux AppImage。
    AppImage,
    /// Debian 包。
    Deb,
    /// RPM 包。
    Rpm,
    /// 便携 zip（解压即用）。
    PortableZip,
    /// 便携单文件 exe。
    PortableExe,
}

impl PackageFormat {
    /// 清单写法。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Msi => "msi",
            Self::Dmg => "dmg",
            Self::AppImage => "appimage",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::PortableZip => "portable-zip",
            Self::PortableExe => "portable-exe",
        }
    }

    /// 解析清单写法。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "msi" => Some(Self::Msi),
            "dmg" => Some(Self::Dmg),
            "appimage" => Some(Self::AppImage),
            "deb" => Some(Self::Deb),
            "rpm" => Some(Self::Rpm),
            "portable-zip" | "zip" => Some(Self::PortableZip),
            "portable-exe" | "exe" => Some(Self::PortableExe),
            _ => None,
        }
    }

    /// 是否需要安装器代劳（这些形态不该被本模块原地覆盖）。
    #[must_use]
    pub const fn needs_installer(self) -> bool {
        matches!(self, Self::Msi | Self::Dmg | Self::Deb | Self::Rpm)
    }
}

impl fmt::Display for PackageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
