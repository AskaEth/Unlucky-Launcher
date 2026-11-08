//! 外观三键的取值表与「非法值回落」诊断（D36 / D37 / D38）。
//!
//! 与 [`super`] 拆开只为单文件 ≤400 行的纪律：这里是纯字面量表 + 纯判据，
//! 不碰文档读写。对外路径仍是 `setting::appearance::*`（[`super`] 逐个重导出）。

use serde_json::Value;

use crate::setting::document::SettingDocument;

/// `appearance.titlebar_mode` 的允许值（D37 三态）。**顺序即设置页分段控件的顺序**。
pub const TITLEBAR_MODES: &[&str] = &["auto", "always", "never"];

/// `appearance.window_corner` 的允许值（M116）。**顺序即设置页分段控件的顺序**。
///
/// 只有两档是**刻意**的：系统不暴露圆角半径（Win32 没有「给我 DWM 的圆角像素值」这种 API），
/// 所以「固定 8/12/16」这类档位只能是自绘圆角，而自绘圆角在 `decorations=false` + 整窗 α 的
/// 窗口上必须裁内容层，会把标题栏三键与缩放热区一起裁掉。用户定案（2026-11-03）：只留
/// 「跟随系统 / 直角」。
pub const WINDOW_CORNERS: &[&str] = &["system", "square"];

/// 窗口外框圆角策略（M116）。
///
/// 只表达**用户要什么**；「这台机器能做到什么」的判定在 UI 侧（`app-iced` 的 `platform`）：
/// core 不探测系统能力（与 [`TitlebarMode`] 同一条纪律）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowCorner {
    /// 跟随系统：Win11 / Server 2025 内核（build ≥ 22000）用 DWM 圆角，Win10 保持直角
    #[default]
    System,
    /// 直角：任何平台都不加圆角
    Square,
}

impl WindowCorner {
    /// 落盘取值。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Square => "square",
        }
    }

    /// 从落盘取值解析；未知值按 `system` 兜底（手改坏配置也不崩，回落原因由
    /// [`diagnostics`] 给出可读诊断）。
    #[must_use]
    pub fn from_key_value(value: &str) -> Self {
        match value {
            "square" => Self::Square,
            _ => Self::System,
        }
    }
}

/// 标题栏三态（D37）。
///
/// 只持有「用户想要哪种」，**不含** `auto` 的判定 —— 「非 Wayland → 显示 / 合成器名精确命中
/// → 隐藏 / 旁证 / 兜底显示」那套顺序属于 UI 侧（WCHROME-IMPL），core 不探测环境。
/// 特别地：判定不许用 `is_decorated()`，那条纪律与本文档无关，别在这里实现。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TitlebarMode {
    /// 按平台/合成器自动决定（默认；由 UI 侧判定）
    #[default]
    Auto,
    /// 永远显示自绘标题栏
    Always,
    /// 永远不显示（合成器已给服务端装饰时）
    Never,
}

impl TitlebarMode {
    /// 落盘取值。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }

    /// 从落盘取值解析；未知值按 `auto` 兜底（手改坏配置也不崩，
    /// 回落原因由 [`diagnostics`] 给出可读诊断）。
    #[must_use]
    pub fn from_key_value(value: &str) -> Self {
        match value {
            "always" => Self::Always,
            "never" => Self::Never,
            _ => Self::Auto,
        }
    }
}

/// `appearance.palette_source` 的允许值：D38 的三级配色来源。
///
/// **取值与 `unlucky_design::palette::Level::code()` 同名同值**（`background-image` /
/// `wallpaper` / `preset`）。这里抄字面量而不是引用 design：`core` 不许反过来依赖 UI 侧
/// （分层纪律，D3），一致性由 `tests` 里的字面量断言固定 —— 与 `JDK_VENDORS` 的处理同一条路子。
pub const PALETTE_SOURCES: &[&str] = &["background-image", "wallpaper", "preset"];

/// 配色来源档（D38：背景图取色 > 桌面壁纸取色 > 内置色卡）。
///
/// 这个键**不是**优先级链本身（链在 `design::palette::resolve` 里），而是「当前生效的档」
/// 与「用户/调用方希望的最强来源」的登记位：默认 [`Self::Preset`] 对应莫奈取色关闭的出厂态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaletteSource {
    /// ① 启动器背景图取色
    BackgroundImage,
    /// ② 桌面壁纸取色
    Wallpaper,
    /// ③ 内置色卡（默认：出厂不取色）
    #[default]
    Preset,
}

impl PaletteSource {
    /// 落盘取值（与 design 侧 `Level::code()` 逐字一致）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BackgroundImage => "background-image",
            Self::Wallpaper => "wallpaper",
            Self::Preset => "preset",
        }
    }

    /// 从落盘取值解析；未知值按 `preset`（内置色卡）兜底。
    #[must_use]
    pub fn from_key_value(value: &str) -> Self {
        match value {
            "background-image" => Self::BackgroundImage,
            "wallpaper" => Self::Wallpaper,
            _ => Self::Preset,
        }
    }
}

/// 一次「按默认档兜底」的诊断：哪个键、读到了什么、兜底成什么、一句给人看的话。
///
/// 存在的理由：非法枚举**不报错**（配置坏掉不该把启动器卡住），但也**不许静默**——
/// 用户/日志要能看出「我写的 `always` 拼错了，现在按 auto 跑」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppearanceDiagnostic {
    /// 出问题的键。
    pub key: &'static str,
    /// 文件里读到的原始文本。
    pub raw: String,
    /// 实际生效的兜底档。
    pub fallback: &'static str,
    /// 一句可读的说明（可直接进日志；UI 侧可再包一层文案）。
    pub message: String,
}

/// 读出需要提示的回落（空 = 一切正常）。
///
/// 只覆盖**枚举型**键（`titlebar_mode` / `palette_source` / `window_corner`）。数值键
/// （`window_opacity`）越界是「钳位且不报错」的既定语义（D36 下限 0.08），不算诊断项。
#[must_use]
pub fn diagnostics(doc: &SettingDocument) -> Vec<AppearanceDiagnostic> {
    let mut found = Vec::new();
    let text = |key: &str| {
        doc.effective(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };

    if let Some(raw) = text("appearance.titlebar_mode") {
        if !TITLEBAR_MODES.contains(&raw.as_str()) {
            let fallback = TitlebarMode::Auto.as_str();
            found.push(AppearanceDiagnostic {
                key: "appearance.titlebar_mode",
                message: format!(
                    "appearance.titlebar_mode 的值 {raw:?} 不在 {TITLEBAR_MODES:?} 之中，已按 {fallback} 处理"
                ),
                raw,
                fallback,
            });
        }
    }
    if let Some(raw) = text("appearance.palette_source") {
        if !PALETTE_SOURCES.contains(&raw.as_str()) {
            let fallback = PaletteSource::Preset.as_str();
            found.push(AppearanceDiagnostic {
                key: "appearance.palette_source",
                message: format!(
                    "appearance.palette_source 的值 {raw:?} 不在 {PALETTE_SOURCES:?} 之中，已按 {fallback} 处理"
                ),
                raw,
                fallback,
            });
        }
    }
    if let Some(raw) = text("appearance.window_corner") {
        if !WINDOW_CORNERS.contains(&raw.as_str()) {
            let fallback = WindowCorner::System.as_str();
            found.push(AppearanceDiagnostic {
                key: "appearance.window_corner",
                message: format!(
                    "appearance.window_corner 的值 {raw:?} 不在 {WINDOW_CORNERS:?} 之中，已按 {fallback} 处理"
                ),
                raw,
                fallback,
            });
        }
    }
    found
}
