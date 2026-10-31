//! 「外观（高级）」配置簇的读写（D17 · `09-ui-layout.md` §2 G6 + §111 约束）。
//!
//! 为什么放在 core：主题令牌的**取值与校验**是配置事实，视觉实现（Slint 侧）只消费结果。
//! 分层纪律要求 core 零 UI 依赖——本文件只认 `appearance.*` 这些键与它们的取值形状，
//! 不认任何像素/控件（[`super::keys`] 登记同一份键表，两边由 `tests/setting/appearance_keys.rs` 固定）。
//!
//! 设计约束（09 §111）：一切高级特性**默认关闭**（出厂 Metro 纯色）；全部走 D6 schema
//! （AI 可写白名单内，可被 agent 调节）。因此默认值必须等于「什么都没开」的出厂态：
//! `panel_alpha = 1.0`、`glass = false`、`bg_image_enabled = false`、`accent` 空串（= 出厂 accent）。

use serde_json::Value;

use super::document::SettingDocument;
use super::store::SettingStore;

/// `appearance.bg_fill` 的允许值（09 §101：cover / contain / tile）。**顺序即 UI 分段控件的顺序**。
pub const BG_FILLS: &[&str] = &["cover", "contain", "tile"];

/// 出厂强调色（Metro 蓝，D14）；`appearance.accent_color` 为空串时用它。
pub const DEFAULT_ACCENT: &str = "#0078D4";

/// `appearance.panel_alpha` 下限：再低前景就压不住背景图（09 §111「对比度检查提示」的硬底）。
pub const PANEL_ALPHA_MIN: f64 = 0.4;
/// `appearance.bg_dim` 下限（0 = 不暗化，允许但设置页会提示对比度风险）。
pub const BG_DIM_MIN: f64 = 0.0;
/// `appearance.bg_dim` 上限：再高背景图基本看不见，等于关掉。
pub const BG_DIM_MAX: f64 = 0.95;

/// 背景图填充模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BgFill {
    /// 等比铺满、裁切溢出（默认：不露底色）
    #[default]
    Cover,
    /// 等比完整显示、留底色
    Contain,
    /// 原始尺寸平铺
    Tile,
}

impl BgFill {
    /// 落盘取值。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Contain => "contain",
            Self::Tile => "tile",
        }
    }

    /// 从落盘取值解析；未知值按 `cover` 兜底（手改坏配置也不崩）。
    #[must_use]
    pub fn from_key_value(value: &str) -> Self {
        match value {
            "contain" => Self::Contain,
            "tile" => Self::Tile,
            _ => Self::Cover,
        }
    }
}

/// 外观（高级）配置簇：**只是取值快照**，读写都走 [`super::store::SettingStore`]。
#[derive(Debug, Clone, PartialEq)]
pub struct Appearance {
    /// 主题三态：`auto` / `light` / `dark`（与 `prefs::ThemeMode` 同名同值）。
    pub theme_mode: String,
    /// 自定义窗口背景图开关（默认关）。
    pub bg_image_enabled: bool,
    /// 背景图路径（PNG；空 = 未设置）。
    pub bg_image: String,
    /// 背景图填充模式。
    pub bg_fill: BgFill,
    /// 叠加暗化层强度 0.0~0.95（越大前景越清晰）。
    pub bg_dim: f64,
    /// 全局面板半透明度 0.4~1.0（1.0 = 出厂不透明）。
    pub panel_alpha: f64,
    /// 玻璃拟态卡片（预生成模糊背景层）。
    pub glass: bool,
    /// 自定义强调色 `#RRGGBB`（空串 = 出厂 [`DEFAULT_ACCENT`]）。
    pub accent_color: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme_mode: "auto".to_owned(),
            bg_image_enabled: false,
            bg_image: String::new(),
            bg_fill: BgFill::Cover,
            bg_dim: 0.35,
            panel_alpha: 1.0,
            glass: false,
            accent_color: String::new(),
        }
    }
}

impl Appearance {
    /// 从配置文档读一份快照（文件值 → 默认值；坏值按默认值兜底，绝不 panic）。
    #[must_use]
    pub fn read(doc: &SettingDocument) -> Self {
        let default = Self::default();
        let text = |key: &str| doc.effective_str(key).map(str::to_owned);
        let boolean = |key: &str| doc.effective(key).and_then(Value::as_bool);
        let number = |key: &str| doc.effective(key).and_then(Value::as_f64);

        Self {
            theme_mode: text("appearance.theme_mode").unwrap_or(default.theme_mode),
            bg_image_enabled: boolean("appearance.bg_image_enabled").unwrap_or(false),
            bg_image: text("appearance.bg_image").unwrap_or_default(),
            bg_fill: text("appearance.bg_fill")
                .map_or(default.bg_fill, |value| BgFill::from_key_value(&value)),
            bg_dim: number("appearance.bg_dim")
                .map_or(default.bg_dim, |value| clamp(value, BG_DIM_MIN, BG_DIM_MAX)),
            panel_alpha: number("appearance.panel_alpha").map_or(default.panel_alpha, |value| {
                clamp(value, PANEL_ALPHA_MIN, 1.0)
            }),
            glass: boolean("appearance.glass").unwrap_or(false),
            accent_color: text("appearance.accent_color")
                .map(|value| normalize_hex(&value).unwrap_or_default())
                .unwrap_or_default(),
        }
    }

    /// 从 store 读（配置读写的统一入口）。
    #[must_use]
    pub fn from_store(store: &SettingStore) -> Self {
        Self::read(store.doc())
    }

    /// 实际生效的强调色：未自定义时回落到出厂 accent。
    #[must_use]
    pub fn accent_or_default(&self) -> &str {
        if self.accent_color.is_empty() {
            DEFAULT_ACCENT
        } else {
            &self.accent_color
        }
    }

    /// 背景图是否真的会用上（开关打开 **且** 路径非空）。
    #[must_use]
    pub fn bg_image_active(&self) -> bool {
        self.bg_image_enabled && !self.bg_image.trim().is_empty()
    }

    /// 写回一组键（调用方负责 `save()`；默认值不落盘由 `SettingDocument` 决定）。
    ///
    /// 值先过一遍规范化：`bg_fill` 认枚举、`bg_dim`/`panel_alpha` 夹到范围内、
    /// `accent_color` 只接受 `#RRGGBB`（空串表示清掉自定义）。
    ///
    /// # Errors
    /// 键值被 schema 拒绝（只读文件 / 未知键 / 越界）时返回 [`super::error::SettingError`]。
    pub fn write(&self, store: &mut SettingStore) -> super::error::SettingResult<()> {
        store.set(
            "appearance.theme_mode",
            Value::from(self.theme_mode.as_str()),
        )?;
        store.set(
            "appearance.bg_image_enabled",
            Value::from(self.bg_image_enabled),
        )?;
        store.set("appearance.bg_image", Value::from(self.bg_image.as_str()))?;
        store.set("appearance.bg_fill", Value::from(self.bg_fill.as_str()))?;
        store.set(
            "appearance.bg_dim",
            Value::from(clamp(self.bg_dim, BG_DIM_MIN, BG_DIM_MAX)),
        )?;
        store.set(
            "appearance.panel_alpha",
            Value::from(clamp(self.panel_alpha, PANEL_ALPHA_MIN, 1.0)),
        )?;
        store.set("appearance.glass", Value::from(self.glass))?;
        store.set(
            "appearance.accent_color",
            Value::from(self.accent_color.as_str()),
        )?;
        Ok(())
    }
}

/// `#RRGGBB` 规范化（大写、去空格）；不合法返回 `None`。
#[must_use]
pub fn normalize_hex(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let body = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if body.len() != 6 || !body.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("#{}", body.to_ascii_uppercase()))
}

/// `#RRGGBB` → `(r, g, b)`（UI 侧构造颜色用）。
#[must_use]
pub fn hex_to_rgb(text: &str) -> Option<(u8, u8, u8)> {
    let hex = normalize_hex(text)?;
    let byte = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
    Some((byte(1..3)?, byte(3..5)?, byte(5..7)?))
}

/// 相对亮度（WCAG 2.x，`0.0`~`1.0`）：设置页「对比度检查提示」用它判断前景是否够读。
#[must_use]
pub fn relative_luminance(text: &str) -> Option<f64> {
    let (r, g, b) = hex_to_rgb(text)?;
    let channel = |value: u8| {
        let value = f64::from(value) / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    Some(0.2126f64.mul_add(
        channel(r),
        0.7152f64.mul_add(channel(g), 0.0722 * channel(b)),
    ))
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value.is_nan() {
        min
    } else {
        value.clamp(min, max)
    }
}
