//! 系统主题（浅色/深色）探测：给主题三态的 `auto` 模式做**初始化对齐**与后端不支持时的兜底跟随。
//!
//! 出处：`DevDocs/PlanDoc/09-ui-layout.md` §2「主题模式三态」（D17 用户定案：Win/mac 用 Slint
//! `Palette.color-scheme` 原生跟随 OS 热切；Linux 依 DE 的 xdg-desktop-portal，缺省回落浅色并
//! 允许手动钉死）。Slint 后端会在运行期自己监听 OS 主题变化，所以这里**只补两件事**：
//!
//! 1. 启动时先读一次真实系统主题，避免「auto 模式下首帧按浅色画、几百毫秒后突然翻黑」的闪变；
//! 2. 后端把 `Palette.color-scheme` 报成 `unknown`（无 portal 的 Linux / 软渲染后端）时，
//!    由 app 轮询本模块把系统主题补上，`auto` 仍然可用。
//!
//! 与 platform 层其他探测一致（见 [`super::windows_version`]）：core 是 `#![forbid(unsafe_code)]`，
//! 一律走外部命令 + 纯函数解析。解析函数跨平台编译，任意宿主都能单测（`tests/platform/theme_cases.rs`）。

/// 系统主题三态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemTheme {
    /// 浅色。
    Light,
    /// 深色。
    Dark,
    /// 读不到（无桌面环境 / 命令缺失 / 输出无法解析）。
    Unknown,
}

impl SystemTheme {
    /// 稳定名（日志用）。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Unknown => "unknown",
        }
    }

    /// 是否深色（`Unknown` 为 `None`，调用方决定回落）。
    #[must_use]
    pub const fn is_dark(&self) -> Option<bool> {
        match self {
            Self::Light => Some(false),
            Self::Dark => Some(true),
            Self::Unknown => None,
        }
    }
}

/// Windows 个性化键（`AppsUseLightTheme` = 应用/任务栏主题，与 `SystemUsesLightTheme` 区分）。
pub const WINDOWS_PERSONALIZE_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// 真实探测系统主题。
///
/// - Windows：注册表 `AppsUseLightTheme`（DWORD，1 = 浅色 / 0 = 深色）；
/// - macOS：`defaults read -g AppleInterfaceStyle`（深色时输出 `Dark`，浅色时该键不存在）；
/// - Linux：`gsettings get org.gnome.desktop.interface color-scheme`，退到 `gtk-theme` 名字。
#[must_use]
pub fn probe() -> SystemTheme {
    #[cfg(target_os = "windows")]
    {
        run(
            "reg",
            &["query", WINDOWS_PERSONALIZE_KEY, "/v", "AppsUseLightTheme"],
        )
        .and_then(|text| parse_apps_use_light_theme(&text))
        .unwrap_or(SystemTheme::Unknown)
    }
    #[cfg(target_os = "macos")]
    {
        // 键不存在 = 浅色（defaults 会非零退出，stdout 为空 → 回落 Light）。
        match run("defaults", &["read", "-g", "AppleInterfaceStyle"]) {
            Some(text) => parse_macos_interface_style(&text).unwrap_or(SystemTheme::Unknown),
            None => SystemTheme::Light,
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        probe_linux()
    }
}

/// Linux：先问 GNOME/portal 的 `color-scheme`，再退到 `gtk-theme` 名字里是否带 `-dark`。
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn probe_linux() -> SystemTheme {
    if let Some(text) = run(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "color-scheme"],
    ) {
        if let Some(theme) = parse_gsettings_color_scheme(&text) {
            return theme;
        }
    }
    if let Some(text) = run(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "gtk-theme"],
    ) {
        if let Some(theme) = parse_gtk_theme(&text) {
            return theme;
        }
    }
    SystemTheme::Unknown
}

/// 解析 `reg query ... /v AppsUseLightTheme` 的输出。
///
/// 形如 `    AppsUseLightTheme    REG_DWORD    0x1`；`0x0` = 深色。
#[must_use]
pub fn parse_apps_use_light_theme(text: &str) -> Option<SystemTheme> {
    text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let name = fields.next()?;
        if !name.eq_ignore_ascii_case("AppsUseLightTheme") {
            return None;
        }
        let _kind = fields.next()?;
        let raw = fields.next().unwrap_or_default();
        parse_dword(raw).map(|value| {
            if value == 0 {
                SystemTheme::Dark
            } else {
                SystemTheme::Light
            }
        })
    })
}

/// 解析 `defaults read -g AppleInterfaceStyle` 的输出（深色 = `Dark`；浅色时该键不存在）。
#[must_use]
pub fn parse_macos_interface_style(text: &str) -> Option<SystemTheme> {
    match text.trim() {
        "" => None,
        value if value.eq_ignore_ascii_case("dark") => Some(SystemTheme::Dark),
        _ => Some(SystemTheme::Light),
    }
}

/// 解析 `gsettings get ... color-scheme`：`'prefer-dark'` / `'prefer-light'` / `'default'`。
///
/// `default` 表示「未指定」，要继续看 `gtk-theme`，故返回 `None`。
#[must_use]
pub fn parse_gsettings_color_scheme(text: &str) -> Option<SystemTheme> {
    let value = text.trim().trim_matches('\'').trim_matches('"');
    if value.eq_ignore_ascii_case("prefer-dark") {
        Some(SystemTheme::Dark)
    } else if value.eq_ignore_ascii_case("prefer-light") {
        Some(SystemTheme::Light)
    } else {
        None
    }
}

/// 解析 `gsettings get ... gtk-theme`：主题名带 `dark` 即深色（`Yaru-dark`、`Adwaita-dark`）。
#[must_use]
pub fn parse_gtk_theme(text: &str) -> Option<SystemTheme> {
    let value = text.trim().trim_matches('\'').trim_matches('"');
    if value.is_empty() {
        return None;
    }
    if value.to_ascii_lowercase().contains("dark") {
        Some(SystemTheme::Dark)
    } else {
        Some(SystemTheme::Light)
    }
}

fn parse_dword(raw: &str) -> Option<u32> {
    let raw = raw.trim();
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        raw.parse().ok()
    }
}

/// 跑一个外部命令，取 stdout（命令缺失 / 非零退出 / 空输出都返回 `None`）。
fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = super::child::quiet(program).args(args).output().ok()?;
    if output.stdout.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}
