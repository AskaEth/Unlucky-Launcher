//! UI 偏好项与持久化键登记（导航折叠、AI 栏折叠、磁贴尺寸、视图、主题三态与外观四件、下载分类）。
//!
//! 偏好不自己写盘：core::setting 的 schema 按 [`keys::ALL`] 登记这些键，
//! app 启动时读一次、变更时写回（跨模块约定见 `DevDocs/SubAgentsTalk/app-DTO通道约定.md`）。
//! 落盘实现是 [`crate::settings_bridge`]（同一份 `config/launcher.json`，与 `ull config set` 同源），
//! 主题三态 + `appearance.*` 高级键已接（M41）；其余 UI 偏好键仍只在内存里（后续 mission 接）。

/// 背景图填充模式（09 §101）。直接复用 core 的枚举：取值与落盘词表只有一处真相。
pub use unlucky_core::setting::appearance::BgFill;

use crate::dto::VersionCategory;

/// 预设强调色板（09 §104）。**必须**与 `ui/pages/settings-appearance.slint` 的 `swatch-colors`
/// 同序同值：Slint 侧只回传索引，十六进制串由这张表给出（一处真相）。
pub const ACCENT_PRESETS: [&str; 8] = [
    "#0078D4", "#0F6CBD", "#107C10", "#9D5D00", "#C42B1C", "#8764B8", "#C239B3", "#00B7C3",
];

/// 磁贴尺寸（Q24-3：小 6 列 / 中 4 列 / 大 3 列）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TileSize {
    /// 小：6 列，只留图标/名字/启动键
    Small,
    /// 中：4 列（D16 默认）
    #[default]
    Medium,
    /// 大：3 列，显示完整徽标与最后游玩时间
    Large,
}

impl TileSize {
    /// 该尺寸对应的网格列数。
    pub fn columns(self) -> usize {
        match self {
            Self::Small => 6,
            Self::Medium => 4,
            Self::Large => 3,
        }
    }

    /// 设置页/实例页分段控件的文案。
    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "小",
            Self::Medium => "中",
            Self::Large => "大",
        }
    }
}

/// 实例页视图模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstanceView {
    /// 磁贴网格（默认）
    #[default]
    Tiles,
    /// 紧凑列表
    List,
}

/// 主题三态（D17，P0 交付）：`appearance.theme_mode ∈ {light, dark, auto}`，出厂 auto。
///
/// auto 由 `theme/tokens.slint` 读 Slint 原生 `Palette.color-scheme` 跟随系统（后端随 OS 热更）；
/// light/dark 由 `Theme.theme-mode` 钉死**自绘体系**；std-widgets 内部件（ListView 滚动条 /
/// ProgressIndicator / AboutSlint）目前仍跟随系统色，`Palette.color-scheme` 的钉死留 P1。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    /// 跟随系统（默认）
    #[default]
    Auto,
    /// 强制浅色
    Light,
    /// 强制深色
    Dark,
}

impl ThemeMode {
    /// 落盘取值（`appearance.theme_mode`）：`auto` / `light` / `dark`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// 从落盘取值解析；未知值按 `auto` 兜底（配置被手改坏也不崩）。
    pub fn from_key_value(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::Auto,
        }
    }
}

/// 09 §101-104「外观（高级）」四件的取值快照（出厂全部关闭）。
///
/// 与 `unlucky_core::setting::appearance::Appearance` 一一对应，但字段类型是 UI 侧好用的
/// `f32`/`String`；落盘由 [`crate::settings_bridge`] 转成 schema 键。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AppearancePrefs {
    /// 背景图总开关
    pub bg_image_enabled: bool,
    /// 背景图路径（PNG；空 = 未设置）
    pub bg_image: String,
    /// 填充模式
    pub bg_fill: BgFill,
    /// 暗化层强度 0.0~0.9
    pub bg_dim: f32,
    /// 面板半透明度 0.4~1.0
    pub panel_alpha: f32,
    /// 玻璃拟态卡片
    pub glass: bool,
    /// 自定义强调色 `#RRGGBB`；空 = 出厂 [#0078D4]
    pub accent_color: String,
}

impl AppearancePrefs {
    /// 出厂态：Metro 纯色，四个高级特性全关。
    pub fn factory() -> Self {
        Self {
            bg_image_enabled: false,
            bg_image: String::new(),
            bg_fill: BgFill::Cover,
            bg_dim: 0.35,
            panel_alpha: 1.0,
            glass: false,
            accent_color: String::new(),
        }
    }

    /// 背景图是否真的生效（开关打开且路径非空）。
    pub fn bg_active(&self) -> bool {
        self.bg_image_enabled && !self.bg_image.trim().is_empty()
    }

    /// 规范化写入强调色：`#RRGGBB` 才接受（空串 = 恢复出厂）。
    ///
    /// 返回 `false` 表示输入不合法（调用方据此亮设置页的 warn 行，不改现状）。
    pub fn set_accent(&mut self, text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            self.accent_color.clear();
            return true;
        }
        match unlucky_core::setting::appearance::normalize_hex(trimmed) {
            Some(hex) => {
                self.accent_color = hex;
                true
            }
            None => false,
        }
    }

    /// 用预设色板第 `index` 个颜色（越界忽略）。
    pub fn set_accent_preset(&mut self, index: usize) -> bool {
        match ACCENT_PRESETS.get(index) {
            Some(hex) => self.set_accent(hex),
            None => false,
        }
    }
}

/// 需要跨会话记忆的 UI 偏好。
///
/// 注意：`Eq` 不成立（[`AppearancePrefs`] 带 `f32` 滑条值），用到的地方按 `PartialEq` 比。
#[derive(Debug, Clone, PartialEq)]
pub struct UiPrefs {
    /// 左侧导航栏折叠成图标条
    pub nav_collapsed: bool,
    /// 右侧 AI 栏折叠成 icon 轨
    pub ai_collapsed: bool,
    /// 磁贴尺寸
    pub tile_size: TileSize,
    /// 实例页视图
    pub instance_view: InstanceView,
    /// 主题模式
    pub theme: ThemeMode,
    /// 外观（高级）四件（09 §101-104）
    pub appearance: AppearancePrefs,
    /// 下载页选中的分类
    pub download_category: VersionCategory,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            nav_collapsed: false,
            ai_collapsed: false,
            // D16 默认中磁贴（4 列）
            tile_size: TileSize::Medium,
            instance_view: InstanceView::Tiles,
            theme: ThemeMode::Auto,
            appearance: AppearancePrefs::factory(),
            download_category: VersionCategory::Release,
        }
    }
}

/// 持久化键名登记（core::setting schema 按此注册；改名等于迁移，慎动）。
pub mod keys {
    /// 导航栏折叠
    pub const NAV_COLLAPSED: &str = "ui.nav.collapsed";
    /// AI 栏折叠
    pub const AI_COLLAPSED: &str = "ui.ai.collapsed";
    /// 磁贴尺寸
    pub const TILE_SIZE: &str = "ui.instances.tile_size";
    /// 实例页视图（磁贴/列表）
    pub const INSTANCE_VIEW: &str = "ui.instances.view";
    /// 主题三态（D17；与设置页「外观」分组同名，取值见 [`super::ThemeMode::as_str`]）
    pub const THEME_MODE: &str = "appearance.theme_mode";
    /// 背景图开关（09 §101）
    pub const BG_IMAGE_ENABLED: &str = "appearance.bg_image_enabled";
    /// 背景图路径
    pub const BG_IMAGE: &str = "appearance.bg_image";
    /// 背景图填充模式
    pub const BG_FILL: &str = "appearance.bg_fill";
    /// 暗化层强度
    pub const BG_DIM: &str = "appearance.bg_dim";
    /// 面板半透明度（09 §102）
    pub const PANEL_ALPHA: &str = "appearance.panel_alpha";
    /// 玻璃拟态卡片（09 §103）
    pub const GLASS: &str = "appearance.glass";
    /// 自定义强调色（09 §104）
    pub const ACCENT_COLOR: &str = "appearance.accent_color";
    /// 下载页分类
    pub const DOWNLOAD_CATEGORY: &str = "ui.download.category";

    /// 本模块登记的全部键（自检/迁移用）。
    pub const ALL: [&str; 13] = [
        NAV_COLLAPSED,
        AI_COLLAPSED,
        TILE_SIZE,
        INSTANCE_VIEW,
        THEME_MODE,
        BG_IMAGE_ENABLED,
        BG_IMAGE,
        BG_FILL,
        BG_DIM,
        PANEL_ALPHA,
        GLASS,
        ACCENT_COLOR,
        DOWNLOAD_CATEGORY,
    ];
}

#[cfg(test)]
mod tests {
    use super::{keys, InstanceView, ThemeMode, TileSize, UiPrefs};

    #[test]
    fn defaults_match_d16() {
        let prefs = UiPrefs::default();
        assert_eq!(prefs.tile_size, TileSize::Medium);
        assert_eq!(prefs.tile_size.columns(), 4);
        assert_eq!(prefs.instance_view, InstanceView::Tiles);
        assert_eq!(prefs.theme, ThemeMode::Auto);
        assert_eq!(prefs.theme.as_str(), "auto");
        assert!(!prefs.nav_collapsed);
        assert!(!prefs.ai_collapsed);
    }

    #[test]
    fn tile_size_columns_follow_q24() {
        assert_eq!(TileSize::Small.columns(), 6);
        assert_eq!(TileSize::Medium.columns(), 4);
        assert_eq!(TileSize::Large.columns(), 3);
    }

    #[test]
    fn theme_mode_round_trips_through_key_values() {
        for mode in [ThemeMode::Auto, ThemeMode::Light, ThemeMode::Dark] {
            assert_eq!(ThemeMode::from_key_value(mode.as_str()), mode);
        }
        // 坏值兜底
        assert_eq!(ThemeMode::from_key_value("solarized"), ThemeMode::Auto);
    }

    #[test]
    fn pref_keys_are_unique() {
        let mut keys = keys::ALL.to_vec();
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), total);
    }
}
