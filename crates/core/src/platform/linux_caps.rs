//! Linux 探测：**能力**而非版本号（D19 · `PlanDoc/11-os-detection.md` §2）。
//!
//! 发行版版本对能不能跑没有意义，所以这里分三类事实：
//! - `/etc/os-release` 的 `PRETTY_NAME` —— **仅供显示/日志**；
//! - 内核 release（`/proc/sys/kernel/osrelease`，退化 `uname -r`）—— 喂 MC 的
//!   `rules.os.version`，同时给出最低内核判定用的 `major.minor`；
//! - 能力探测：图形会话类型（Wayland / X11 / 无显示）与 GL 能力钩子（见 [`probe_gl`]）。
//!
//! glibc 版本用 `ldd --version` 探测（musl / Termux 上取不到，此时不阻塞）。
//! fontconfig 可用性与 `xdg-desktop-portal` 同理留到 app 层（字体回退、主题跟随、文件选择器），
//! core 只报环境事实，不做渲染后端决策。
//! 解析函数是纯函数且跨平台编译，单测用注入样本覆盖（见 `tests/platform/`）。

/// 图形会话类型（`Unknown` 是派生默认值：未探测）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SessionKind {
    /// Wayland 会话（`WAYLAND_DISPLAY` 或 `XDG_SESSION_TYPE=wayland`）。
    Wayland,
    /// X11 会话（`DISPLAY` 或 `XDG_SESSION_TYPE=x11`）。
    X11,
    /// 没有图形会话（服务器 / 容器 / SSH）。GUI 用不了，CLI 照常。
    Headless,
    /// 未探测（非 Linux 平台，或探测未执行）。
    #[default]
    Unknown,
}

impl SessionKind {
    /// 简短名（日志用）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wayland => "wayland",
            Self::X11 => "x11",
            Self::Headless => "headless",
            Self::Unknown => "unknown",
        }
    }

    /// 是否存在可用的显示服务器。
    pub fn has_display(self) -> bool {
        matches!(self, Self::Wayland | Self::X11)
    }
}

/// GL/EGL 能力探测结果。
///
/// P1 只留钩子：真探测要 `dlopen("libEGL.so")` + `eglQueryString`，那是 FFI，
/// 而 core 是 `#![forbid(unsafe_code)]`；因此真实探测放在 app 层「渲染后端选择」里做，
/// core 只负责把 `Unknown` 如实报出来，不阻塞任何流程。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GlProbe {
    /// 未探测（P1 常态）。
    #[default]
    Unknown,
    /// 有可用的 GL/EGL。
    Available,
    /// 没有可用 GL（调用方应降级到软渲染）。
    Missing,
}

impl GlProbe {
    /// 简短名（日志用）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Available => "available",
            Self::Missing => "missing",
        }
    }
}

/// Linux 探测结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxCaps {
    /// 发行版展示名（`PRETTY_NAME`，仅显示）。
    pub distro: Option<String>,
    /// 内核 release 原始串（`6.8.0-45-generic`），喂 `rules.os.version`。
    pub kernel: Option<String>,
    /// 内核 `major.minor`（最低内核判定用）。
    pub kernel_version: Option<(u32, u32)>,
    /// glibc `major.minor`；musl / 取不到时为 `None`。
    pub glibc: Option<(u32, u32)>,
    /// 图形会话类型。
    pub session: SessionKind,
    /// GL 能力（P1 恒为 [`GlProbe::Unknown`]）。
    pub gl: GlProbe,
}

/// 探测当前 Linux 环境；非 Linux 平台返回全空的默认值。
#[cfg(target_os = "linux")]
pub fn probe() -> LinuxCaps {
    let distro = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| parse_os_release(&text));
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| run("uname", &["-r"]));
    let glibc = run("ldd", &["--version"]).and_then(|text| parse_glibc_version(&text));
    LinuxCaps {
        distro,
        kernel_version: kernel.as_deref().and_then(parse_kernel_version),
        kernel,
        glibc,
        session: current_session(),
        gl: probe_gl(),
    }
}

/// 探测当前 Linux 环境；非 Linux 平台返回全空的默认值。
#[cfg(not(target_os = "linux"))]
pub fn probe() -> LinuxCaps {
    LinuxCaps {
        distro: None,
        kernel: None,
        kernel_version: None,
        glibc: None,
        session: SessionKind::Unknown,
        gl: GlProbe::Unknown,
    }
}

/// GL/EGL 探测钩子（P1 返回 [`GlProbe::Unknown`]，见类型文档）。
pub fn probe_gl() -> GlProbe {
    GlProbe::Unknown
}

/// 按当前环境变量判定会话类型。
pub fn current_session() -> SessionKind {
    session_from_env(|key| std::env::var(key).ok())
}

/// 按注入的环境变量读取函数判定会话类型（便于单测）。
///
/// 判定顺序：`WAYLAND_DISPLAY` → `XDG_SESSION_TYPE` → `DISPLAY` → 无显示。
pub fn session_from_env(get: impl Fn(&str) -> Option<String>) -> SessionKind {
    let has = |key: &str| get(key).map(|v| !v.trim().is_empty()).unwrap_or(false);
    if has("WAYLAND_DISPLAY") {
        return SessionKind::Wayland;
    }
    match get("XDG_SESSION_TYPE").as_deref().map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("wayland") => return SessionKind::Wayland,
        Some(value) if value.eq_ignore_ascii_case("x11") => return SessionKind::X11,
        _ => {}
    }
    if has("DISPLAY") {
        return SessionKind::X11;
    }
    SessionKind::Headless
}

/// 从 `/etc/os-release` 取 `PRETTY_NAME`（去引号）。
pub fn parse_os_release(text: &str) -> Option<String> {
    let mut fallback = None;
    for line in text.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'').trim();
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "PRETTY_NAME" => return Some(value.to_string()),
            // 没有 PRETTY_NAME 时用 NAME + VERSION_ID 拼一个（某些裁剪发行版只给这两个）。
            "NAME" => {
                let name = value.to_string();
                fallback = Some(match fallback.take() {
                    Some(prev) => format!("{prev} {name}"),
                    None => name,
                });
            }
            "VERSION_ID" => {
                let version = value.to_string();
                fallback = Some(match fallback.take() {
                    Some(prev) => format!("{prev} {version}"),
                    None => version,
                });
            }
            _ => {}
        }
    }
    fallback
}

/// 从内核 release 串取 `major.minor`（`6.8.0-45-generic` → `(6, 8)`）。
pub fn parse_kernel_version(release: &str) -> Option<(u32, u32)> {
    let trimmed = release.trim();
    let mut parts = trimmed.split(|c: char| !c.is_ascii_digit());
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    Some((major, minor))
}

/// 从 `ldd --version` 输出取 glibc `major.minor`
/// （`ldd (Ubuntu GLIBC 2.39-0ubuntu8.3) 2.39` → `(2, 39)`）。
pub fn parse_glibc_version(text: &str) -> Option<(u32, u32)> {
    // 只看第一行：后面是版权声明，可能含有别的数字。
    let first = text.lines().next().unwrap_or_default();
    first.split_whitespace().rev().find_map(|token| {
        let (major, minor) = token.split_once('.')?;
        let minor = minor
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap_or_default();
        Some((major.parse().ok()?, minor.parse().ok()?))
    })
}

/// `(2, 31)` → `"2.31"`（展示用）。
pub fn format_version(version: (u32, u32)) -> String {
    format!("{}.{}", version.0, version.1)
}

#[cfg(target_os = "linux")]
fn run(program: &str, args: &[&str]) -> Option<String> {
    use std::process::Command;
    let output = Command::new(program).args(args).output().ok()?;
    if output.stdout.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}
