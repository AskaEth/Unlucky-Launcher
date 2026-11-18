//! macOS 探测：产品版本 + 运行架构（D19 · `PlanDoc/11-os-detection.md` §2）。
//!
//! - 产品版本：`sw_vers -productVersion` → `12.3.1`（最低部署目标判定 + MC `os.version` 规则）；
//! - 运行架构：`sysctl -n hw.optional.arm64` 输出 `1` 表示**当前运行在 Apple Silicon 上**
//!   （关键：Rosetta 2 下 `cfg!(target_arch)` 仍是 `x86_64`，但原生渲染路径与 MC 原生库
//!   要的是真实架构，所以必须运行期探）；
//! - 内核版本：`uname -r`（`22.4.0`）只进日志，不参与判定。
//!
//! 解析函数是纯函数且跨平台编译，单测用注入样本覆盖（见 `tests/platform/`）。

/// macOS 版本 + 运行架构。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacOsVersion {
    /// 主版本号（macOS 12 Monterey = 12）。
    pub major: u32,
    /// 次版本号。
    pub minor: u32,
    /// 补丁号。
    pub patch: u32,
    /// 是否运行在 Apple Silicon（含 Rosetta 2 场景）。
    pub arm64: bool,
}

impl MacOsVersion {
    /// 构造。
    pub fn new(major: u32, minor: u32, patch: u32, arm64: bool) -> Self {
        Self {
            major,
            minor,
            patch,
            arm64,
        }
    }

    /// 产品版本串（`rules.os.version` 用，例如 `12.3.1`）。
    pub fn version_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    /// 展示名（`macOS 12.3.1 (arm64)`）。
    pub fn display_name(&self) -> String {
        let arch = if self.arm64 { "arm64" } else { "x86_64" };
        format!("macOS {} ({arch})", self.version_string())
    }

    /// `rules.os.arch` 取值：Apple Silicon 上必须是 `arm64`。
    pub fn runtime_arch(&self) -> &'static str {
        if self.arm64 {
            "arm64"
        } else {
            "x86_64"
        }
    }
}

/// 真实探测；非 macOS 平台恒为 `None`。
#[cfg(target_os = "macos")]
pub fn probe() -> Option<MacOsVersion> {
    let (major, minor, patch) =
        run("sw_vers", &["-productVersion"]).and_then(|text| parse_product_version(&text))?;
    // sysctl 在 Intel 机器上以非零码退出（键不存在）→ stdout 为空 → false。
    let arm64 = run("sysctl", &["-n", "hw.optional.arm64"])
        .map(|text| parse_sysctl_bool(&text))
        .unwrap_or(false);
    Some(MacOsVersion::new(major, minor, patch, arm64))
}

/// 真实探测；非 macOS 平台恒为 `None`。
#[cfg(not(target_os = "macos"))]
pub fn probe() -> Option<MacOsVersion> {
    None
}

/// 解析 `12.3.1` / `12.3` / `12`（`sw_vers -productVersion` 的输出）。
pub fn parse_product_version(text: &str) -> Option<(u32, u32, u32)> {
    let trimmed = text.trim();
    let token = trimmed
        .split_whitespace()
        .find(|token| token.starts_with(|c: char| c.is_ascii_digit()))?;
    let mut parts = token.split('.').map(str::parse::<u32>);
    let major = parts.next()?.ok()?;
    let minor = parts.next().map(|value| value.unwrap_or(0)).unwrap_or(0);
    let patch = parts.next().map(|value| value.unwrap_or(0)).unwrap_or(0);
    Some((major, minor, patch))
}

/// 解析 `sysctl -n hw.optional.arm64` 的输出（`1` 为真）。
pub fn parse_sysctl_bool(text: &str) -> bool {
    text.trim() == "1"
}

#[cfg(target_os = "macos")]
fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = super::child::quiet(program).args(args).output().ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}
