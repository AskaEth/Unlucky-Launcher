//! Windows 版本探测：真实的 `major.minor.build`（D19 · `PlanDoc/11-os-detection.md` §2）。
//!
//! 数据源（从快到慢，探测发生在启动路径上）：
//!
//! 1. `reg query ... /v CurrentBuildNumber` 取 build 号，由 [`from_build`] 反推 `major.minor`
//!    （build 号在版本族内单调；实测单值查询约 50ms，整键查询约 150ms）；
//! 2. 整键 `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`：Win10+ 有
//!    `CurrentMajorVersionNumber` / `CurrentMinorVersionNumber`（REG_DWORD，`reg query` 以
//!    `0x..` 十六进制输出），Win7 / 8 / 8.1 只有 `CurrentVersion`（`6.1` / `6.2` / `6.3`，
//!    Win10+ 上它是过期值）；
//! 3. 兜底 `cmd /c ver`：输出 `Microsoft Windows [Version 10.0.19045.4046]`（中文系统为
//!    `[版本 …]`）。只取方括号内的数字段，因此与系统语言无关。
//!
//! # 为什么不用 `RtlGetVersion`
//!
//! D19 原文方案是 `RtlGetVersion`（ntdll），但 core 是 `#![forbid(unsafe_code)]`
//! （`lib.rs`，D3 架构纪律 + workspace lint `unsafe_code = deny`），FFI 一律不可用。
//! 上面两条路径读的是同一份真实事实（`cmd.exe` 自带 manifest，报真实版本，不受
//! `GetVersionEx` 兼容层欺骗），差别只是启动时一次性几毫秒 —— 结果由
//! [`crate::platform::os_info::current`] 缓存，不会重复付这个代价。
//!
//! 解析函数是纯函数且**跨平台编译**，单测用注入样本覆盖各版本输出（见 `tests/platform/`）。

/// Windows 11 的起始 build 号（22H2 之前就有 22000）。
pub const WIN11_MIN_BUILD: u32 = 22_000;

/// 存放版本值的注册表键。
pub const VERSION_KEY: &str = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion";

/// Windows 真实版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsVersion {
    /// 主版本号（Win7 = 6，Win10/11 = 10）。
    pub major: u32,
    /// 次版本号（Win7 = 1，Win10/11 = 0）。
    pub minor: u32,
    /// 构建号（Win7 SP1 = 7601，Win10 22H2 = 19045，Win11 = 22000+）。
    pub build: u32,
}

impl WindowsVersion {
    /// 构造。
    pub fn new(major: u32, minor: u32, build: u32) -> Self {
        Self {
            major,
            minor,
            build,
        }
    }

    /// `major.minor` 元组（最低版本判定用）。
    pub fn major_minor(&self) -> (u32, u32) {
        (self.major, self.minor)
    }

    /// 版本串（`rules.os.version` 用，例如 `10.0.19045`）。
    pub fn version_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.build)
    }

    /// 产品名（由版本号推导，不用本地化的注册表 `ProductName`）。
    pub fn display_name(&self) -> &'static str {
        match self.major_minor() {
            (10, _) if self.build >= WIN11_MIN_BUILD => "Windows 11",
            (10, _) => "Windows 10",
            (6, 3) => "Windows 8.1",
            (6, 2) => "Windows 8",
            (6, 1) => "Windows 7",
            (6, 0) => "Windows Vista",
            (5, _) => "Windows XP",
            _ => "Windows",
        }
    }
}

/// 真实探测；非 Windows 平台恒为 `None`。
///
/// 三档，从快到慢（探测在启动路径上，实测整键 `reg query` 会枚举上百个子键，约 150ms：
/// 单值查询约 50ms）：
/// 1. `reg query ... /v CurrentBuildNumber` 取 build，再由 [`from_build`] 反推 `major.minor`；
/// 2. 整键查询（Win7 的 `CurrentVersion`、Win10+ 的数字键都在这里解析，见 [`parse_reg_query`]）；
/// 3. `cmd /c ver`（注册表被策略禁用时的兜底）。
#[cfg(target_os = "windows")]
pub fn probe() -> Option<WindowsVersion> {
    let fast = run("reg", &["query", VERSION_KEY, "/v", "CurrentBuildNumber"])
        .and_then(|text| parse_build_number(&text))
        .and_then(from_build);
    if fast.is_some() {
        return fast;
    }
    if let Some(version) =
        run("reg", &["query", VERSION_KEY]).and_then(|text| parse_reg_query(&text))
    {
        return Some(version);
    }
    run("cmd", &["/c", "ver"]).and_then(|text| parse_ver_output(&text))
}

/// 真实探测；非 Windows 平台恒为 `None`。
#[cfg(not(target_os = "windows"))]
pub fn probe() -> Option<WindowsVersion> {
    None
}

/// 由 build 号反推 `major.minor`（build 号在版本族内单调，是唯一一条免整键查询的快路径）。
///
/// 7600/7601 = 6.1（Win7 / 2008 R2）、9200 = 6.2（Win8 / 2012）、9600 = 6.3（Win8.1 / 2012 R2）、
/// 10240+ = 10.0（Win10 / Win11 / Server 2016+）。认不出的 build 返回 `None`，
/// 由 [`parse_reg_query`] / [`parse_ver_output`] 兜底。
pub fn from_build(build: u32) -> Option<WindowsVersion> {
    let (major, minor) = match build {
        6000..=7599 => (6, 0),
        7600..=9199 => (6, 1),
        9200..=9599 => (6, 2),
        9600..=10239 => (6, 3),
        10240.. => (10, 0),
        // Vista 之前的 build 号太小，不猜（交给整键/`ver` 解析）。
        _ => return None,
    };
    Some(WindowsVersion::new(major, minor, build))
}

/// 从 `reg query ... /v CurrentBuildNumber` 的输出取 build 号。
pub fn parse_build_number(text: &str) -> Option<u32> {
    text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let name = fields.next()?;
        if !name.eq_ignore_ascii_case("currentbuildnumber") {
            return None;
        }
        let _kind = fields.next();
        parse_number(fields.next().unwrap_or_default())
    })
}

/// 解析 `reg query <VERSION_KEY>` 的输出。
///
/// 形如 `    CurrentBuildNumber    REG_SZ    19045`；DWORD 值以 `0x..` 输出。
pub fn parse_reg_query(text: &str) -> Option<WindowsVersion> {
    let mut major = None;
    let mut minor = None;
    let mut build = None;
    let mut legacy = None;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else {
            continue;
        };
        let _kind = fields.next();
        let value = fields.next().unwrap_or_default();
        match name.to_ascii_lowercase().as_str() {
            "currentmajorversionnumber" => major = parse_number(value),
            "currentminorversionnumber" => minor = parse_number(value),
            "currentbuildnumber" => build = parse_number(value),
            "currentversion" => legacy = parse_version_pair(value),
            _ => {}
        }
    }
    let build = build?;
    let (major, minor) = match major {
        Some(major) => (major, minor.unwrap_or(0)),
        // Win7 / 8 / 8.1：没有 CurrentMajorVersionNumber，只能读 CurrentVersion。
        None => legacy?,
    };
    Some(WindowsVersion::new(major, minor, build))
}

/// 解析 `cmd /c ver` 的输出（`Microsoft Windows [Version 10.0.19045.4046]`，
/// 中文系统 `Microsoft Windows [版本 10.0.19045.4046]`）。
pub fn parse_ver_output(text: &str) -> Option<WindowsVersion> {
    let start = text.find('[')? + 1;
    let rest = &text[start..];
    let inner = &rest[..rest.find(']')?];
    // 只保留数字与点，语言相关的字样（`Version` / `版本`）会被切成空段。
    let digits = inner
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .find(|segment| segment.contains('.'))?;
    let parts: Vec<u32> = digits
        .split('.')
        .take(3)
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    match parts.as_slice() {
        [major, minor, build] => Some(WindowsVersion::new(*major, *minor, *build)),
        _ => None,
    }
}

fn parse_number(value: &str) -> Option<u32> {
    let trimmed = value.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()
    } else {
        trimmed.parse().ok()
    }
}

fn parse_version_pair(value: &str) -> Option<(u32, u32)> {
    let (major, minor) = value.trim().split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

#[cfg(target_os = "windows")]
fn run(program: &str, args: &[&str]) -> Option<String> {
    use std::process::Command;
    let output = Command::new(program).args(args).output().ok()?;
    if output.stdout.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}
