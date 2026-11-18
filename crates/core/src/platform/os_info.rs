//! 运行期系统探测与最低支持判定（D19 · `DevDocs/PlanDoc/11-os-detection.md`）。
//!
//! 事实通道按平台分流（各文件里的 `probe()`）：[`windows_version`] 注册表 `CurrentBuildNumber`
//! 快路径 → 整键 `CurrentVersion` 家族 → `cmd /c ver` 兜底；[`macos_version`] `sw_vers` +
//! `sysctl`；[`linux_caps`] `/etc/os-release`（仅显示）+ 内核 release + 会话/GL 能力探测。
//! 不用 `RtlGetVersion` FFI 是因为 core 是 `#![forbid(unsafe_code)]`，而这些路径读的是同一份
//! 真实事实，代价只是一次性几十毫秒。
//!
//! 支持基线（用户拍板）：**Windows 10 起、macOS 12 起、Linux（内核/glibc 达标）**。
//! 因此等级只有两档：[`SupportLevel::One`] 与 [`SupportLevel::Unsupported`]——Win8.1/8/7
//! 不承诺 GUI，也不承诺 CLI（识别出旧版本正是为了报「不支持」）。
//!
//! 入口：[`detect`]（真探）· [`current`]（进程内缓存）· [`check`]（CLI 启动自检）；
//! 等级映射是纯函数（[`support_level_for`] 及三平台分解），单测用注入式假数据覆盖。

use std::sync::OnceLock;

use crate::platform::linux_caps::{self, GlProbe, SessionKind};
use crate::platform::macos_version;
use crate::platform::vk_probe::{self, VkProbe};
use crate::platform::windows_version;

/// 最低支持的 Windows 版本（10.0 = Windows 10，build 10240 起）。
pub const MIN_WINDOWS: (u32, u32) = (10, 0);
/// 最低支持的 macOS 主版本（Slint / winit 的现实部署下限，见 D19 §4）。
pub const MIN_MACOS_MAJOR: u32 = 12;
/// 最低 Linux 内核（Rust std 的 Linux 下限）。
pub const MIN_LINUX_KERNEL: (u32, u32) = (3, 2);
/// 最低 glibc（`x86_64-unknown-linux-gnu` 构建产物能跑的下限）。
pub const MIN_GLIBC: (u32, u32) = (2, 17);

/// 操作系统家族。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsFamily {
    /// Windows。
    Windows,
    /// macOS。
    MacOs,
    /// Linux 及其它 POSIX 系统。
    Linux,
}

impl OsFamily {
    /// 当前编译目标的系统家族。
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }

    /// Mojang 清单里 `rules.os.name` 的取值。
    pub fn manifest_name(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::MacOs => "osx",
            Self::Linux => "linux",
        }
    }

    /// `rules.os.arch` 的取值（按编译目标；Apple Silicon 的真实架构另见
    /// [`MacOsVersion::runtime_arch`](crate::platform::macos_version::MacOsVersion::runtime_arch)）。
    pub fn manifest_arch(self) -> &'static str {
        if cfg!(target_arch = "x86_64") {
            "x86_64"
        } else if cfg!(target_arch = "x86") {
            "x86"
        } else if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "unknown"
        }
    }

    /// 是否为 POSIX 系统（决定 `nice`、进程组 kill 等分支）。
    pub fn is_posix(self) -> bool {
        !matches!(self, Self::Windows)
    }
}

/// 支持等级（两档：满足基线 / 不满足）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportLevel {
    /// 一等：基线以上（Windows 10+ / macOS 12+ / Linux 达标），GUI + CLI。
    One,
    /// 不支持：低于基线，启动时拒绝并给升级指引。
    Unsupported,
}

impl SupportLevel {
    /// 简短名（日志/事件用）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::One => "one",
            Self::Unsupported => "unsupported",
        }
    }

    /// 是否允许运行。
    pub fn is_usable(self) -> bool {
        matches!(self, Self::One)
    }
}

/// 运行期系统信息（不可变快照，可跨线程共享；字段全是探测出来的事实）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsInfo {
    /// 系统家族。
    pub family: OsFamily,
    /// `rules.os.arch` 用的架构名。
    pub arch: String,
    /// 版本 `major.minor`（探测失败为 `None`）。
    pub major_minor: Option<(u32, u32)>,
    /// 构建号（Windows build，其它平台 `None`）。
    pub build: Option<u32>,
    /// 展示名（`Windows 11` / `macOS 12.3.1 (arm64)` / `Ubuntu 24.04.1 LTS`）。
    pub product: String,
    /// 版本串（`rules.os.version` 用；Linux 上是内核 release）。
    pub version_string: String,
    /// 图形会话类型（Linux 有意义，其它平台 `Unknown`）。
    pub session: SessionKind,
    /// GL 能力（P1 恒为 `Unknown`，见 [`GlProbe`]）。
    pub gl: GlProbe,
    /// Vulkan 能力（文件/注册表层，无 `unsafe`；真探测归 app 层，见 [`VkProbe`]）。
    pub vk: VkProbe,
    /// Linux 内核 `major.minor`。
    pub kernel: Option<(u32, u32)>,
    /// glibc `major.minor`（musl / 取不到为 `None`）。
    pub glibc: Option<(u32, u32)>,
    /// 支持等级（由 [`support_level_for`] 从上面的事实算出）。
    pub support_level: SupportLevel,
}

impl OsInfo {
    /// `rules.os.version` 的取值（探测失败时为空串 —— 规则求值会走保守分支）。
    pub fn os_version(&self) -> &str {
        &self.version_string
    }

    /// 一行日志摘要（`§5.3` 每次启动写日志 / 诊断包环境快照）。
    pub fn summary_line(&self) -> String {
        let version = if self.version_string.is_empty() {
            "unknown"
        } else {
            self.version_string.as_str()
        };
        format!(
            "os={} version={version} arch={} level={} product=\"{}\" session={} gl={} vk={}",
            self.family.manifest_name(),
            self.arch,
            self.support_level.as_str(),
            self.product,
            self.session.as_str(),
            self.gl.as_str(),
            self.vk.as_str(),
        )
    }

    /// 用户可读文案（中英双语）。
    pub fn describe(&self) -> SupportText {
        match self.support_level {
            SupportLevel::One => self.text_supported(),
            SupportLevel::Unsupported => self.text_unsupported(),
        }
    }

    fn text_supported(&self) -> SupportText {
        let mut zh = format!("系统满足最低要求：{}（{}）。", self.product, self.arch);
        let mut en = format!("Meets minimum requirements: {}.", self.product);
        if self.family == OsFamily::Linux && !self.session.has_display() {
            zh.push_str("\n注意：未检测到 Wayland/X11 图形会话，GUI 需要图形会话；CLI 不受影响。");
            en.push_str(
                "\nNote: no Wayland/X11 graphical session; the GUI needs one. CLI is fine.",
            );
        }
        SupportText { zh, en }
    }

    fn text_unsupported(&self) -> SupportText {
        let product = if self.product.is_empty() {
            "未知系统"
        } else {
            self.product.as_str()
        };
        let (need_zh, need_en) = match self.family {
            OsFamily::Windows => (
                " Windows 10 或更高版本".to_string(),
                "Windows 10 or newer".to_string(),
            ),
            OsFamily::MacOs => (
                format!(" macOS {MIN_MACOS_MAJOR} 或更高版本"),
                format!("macOS {MIN_MACOS_MAJOR} or newer"),
            ),
            OsFamily::Linux => {
                let kernel = format!("{}.{}", MIN_LINUX_KERNEL.0, MIN_LINUX_KERNEL.1);
                let glibc = MIN_GLIBC.0;
                (
                    format!(" Linux 内核 ≥ {kernel}（glibc ≥ {glibc}）"),
                    format!("Linux kernel >= {kernel} (glibc >= {glibc})"),
                )
            }
        };
        // Windows 上版本探测失败时要说清楚是「探不到」而不是「探到旧版本」。
        let hint = if self.family == OsFamily::Windows && self.major_minor.is_none() {
            "（未能探测到系统版本）"
        } else {
            ""
        };
        SupportText {
            zh: format!("不支持的系统：{product}{hint}。本启动器要求{need_zh}。"),
            en: format!("Unsupported system: {product}. This launcher requires {need_en}."),
        }
    }
}

/// 双语用户文案。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportText {
    /// 中文文案。
    pub zh: String,
    /// 英文文案。
    pub en: String,
}

impl SupportText {
    /// 中文 + 换行 + 英文（CLI 直接打印的形态）。
    pub fn combined(&self) -> String {
        format!("{}\n{}", self.zh, self.en)
    }
}

/// 启动自检结果（CLI 打印 + 写日志用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportCheck {
    /// 支持等级。
    pub level: SupportLevel,
    /// 用户可读文案（中英双语，多行）。
    pub message: String,
    /// 探测到的系统信息（写日志 / 诊断包用）。
    pub os: OsInfo,
}

impl SupportCheck {
    /// 是否允许继续启动。
    pub fn is_usable(&self) -> bool {
        self.level.is_usable()
    }
}

/// 按家族选对应的等级判定（探测与单测走同一条路径）。
pub fn support_level_for(
    family: OsFamily,
    major_minor: Option<(u32, u32)>,
    kernel: Option<(u32, u32)>,
    glibc: Option<(u32, u32)>,
) -> SupportLevel {
    match family {
        OsFamily::Windows => windows_level(major_minor),
        OsFamily::MacOs => macos_level(major_minor.map(|(major, _)| major)),
        OsFamily::Linux => linux_level(kernel, glibc),
    }
}

/// Windows 等级：≥ Windows 10 → 一等；Win8.1/8/7 及更旧 → 不支持；探测不到 → 不支持（保守）。
pub fn windows_level(major_minor: Option<(u32, u32)>) -> SupportLevel {
    match major_minor {
        Some(version) if version >= MIN_WINDOWS => SupportLevel::One,
        _ => SupportLevel::Unsupported,
    }
}

/// macOS 等级：≥ 12 → 一等；更低或不认识 → 不支持。
pub fn macos_level(major: Option<u32>) -> SupportLevel {
    match major {
        Some(major) if major >= MIN_MACOS_MAJOR => SupportLevel::One,
        _ => SupportLevel::Unsupported,
    }
}

/// Linux 等级：内核/glibc 探得到但过低 → 不支持；取不到（容器、musl、Termux）不阻塞 → 一等。
pub fn linux_level(kernel: Option<(u32, u32)>, glibc: Option<(u32, u32)>) -> SupportLevel {
    if kernel.is_some_and(|version| version < MIN_LINUX_KERNEL) {
        return SupportLevel::Unsupported;
    }
    if glibc.is_some_and(|version| version < MIN_GLIBC) {
        return SupportLevel::Unsupported;
    }
    SupportLevel::One
}

/// 真探一次当前系统（会起子进程，别在热路径反复调用；常规路径用 [`current`]）。
pub fn detect() -> OsInfo {
    let family = OsFamily::current();
    let facts = probe(family);
    let arch = if facts.arm64 {
        "arm64".to_string()
    } else {
        family.manifest_arch().to_string()
    };
    let product = facts
        .product
        .unwrap_or_else(|| family.manifest_name().to_string());
    OsInfo {
        family,
        arch,
        major_minor: facts.major_minor,
        build: facts.build,
        product,
        version_string: facts.version_string.unwrap_or_default(),
        session: facts.session,
        gl: facts.gl,
        vk: vk_probe::probe_vk(),
        kernel: facts.kernel,
        glibc: facts.glibc,
        support_level: support_level_for(family, facts.major_minor, facts.kernel, facts.glibc),
    }
}

/// 三平台探测结果统一成同一形状（内部用；`None` 表示该平台没有这项事实）。
#[derive(Default)]
struct Facts {
    major_minor: Option<(u32, u32)>,
    build: Option<u32>,
    product: Option<String>,
    version_string: Option<String>,
    kernel: Option<(u32, u32)>,
    glibc: Option<(u32, u32)>,
    session: SessionKind,
    gl: GlProbe,
    arm64: bool,
}

fn probe(family: OsFamily) -> Facts {
    match family {
        OsFamily::Windows => {
            let version = windows_version::probe();
            Facts {
                major_minor: version.map(|v| v.major_minor()),
                build: version.map(|v| v.build),
                product: version.map(|v| v.display_name().to_string()),
                version_string: version.map(|v| v.version_string()),
                ..Facts::default()
            }
        }
        OsFamily::MacOs => {
            let version = macos_version::probe();
            Facts {
                major_minor: version.map(|v| (v.major, v.minor)),
                product: version.map(|v| v.display_name()),
                version_string: version.map(|v| v.version_string()),
                arm64: version.is_some_and(|v| v.arm64),
                ..Facts::default()
            }
        }
        OsFamily::Linux => {
            let caps = linux_caps::probe();
            Facts {
                major_minor: caps.kernel_version,
                product: caps.distro,
                version_string: caps.kernel,
                kernel: caps.kernel_version,
                glibc: caps.glibc,
                session: caps.session,
                gl: caps.gl,
                ..Facts::default()
            }
        }
    }
}

/// 进程内缓存的一次探测结果（启动时多处以它为准，避免重复起子进程）。
pub fn current() -> &'static OsInfo {
    static CACHED: OnceLock<OsInfo> = OnceLock::new();
    CACHED.get_or_init(detect)
}

/// CLI 启动自检：等级 + 双语文案 + 系统信息。
pub fn check() -> SupportCheck {
    let os = current().clone();
    SupportCheck {
        level: os.support_level,
        message: os.describe().combined(),
        os,
    }
}
