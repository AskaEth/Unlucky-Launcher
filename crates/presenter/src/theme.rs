//! 主题与外观的**纯**判据：轮询节拍、背景图缓存键与量化、系统主题取值。
//!
//! 这一层原本混在 `crates/app` 的 `ui_bind.rs` / `theme_runtime.rs` 里，与「把值灌进窗口属性」
//! 的绑定代码缠在一起，于是两个 UI 版都得各写一遍。抽到这里的只有**判据**：什么时候该重读配置、
//! 什么时候该重建背景图、系统主题该报哪个值。灌属性仍然由 UI 侧做，本模块不碰任何框架类型。

use crate::prefs::BgFill;

/// 配置热切轮询间隔（拍数，33ms/拍 → 约 1s 一次 `mtime` 比对，AI/CLI 改完下一拍即生效）。
pub const CONFIG_POLL_TICKS: u64 = 30;

/// 系统主题兜底轮询间隔（拍数）：只在 `auto` 且后端不认识系统主题时启动（约 3s）。
pub const SYSTEM_THEME_POLL_TICKS: u64 = 90;

/// tile 模式重建画布的窗口尺寸粒度（像素）：避免拖动窗口时反复重算平铺画布。
pub const TILE_REGENERATE_STEP: u32 = 128;

/// 系统主题取值（UI 侧根组件属性就用这三个值：0 未知 / 1 浅色 / 2 深色）。
pub const SYSTEM_THEME_UNKNOWN: i32 = 0;
/// 浅色。
pub const SYSTEM_THEME_LIGHT: i32 = 1;
/// 深色。
pub const SYSTEM_THEME_DARK: i32 = 2;

/// 背景图加工输入（值相同就不重新解码）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundKey {
    /// 背景图路径
    pub path: String,
    /// 填充模式
    pub fill: BgFill,
    /// tile 用的窗口尺寸（按 [`TILE_REGENERATE_STEP`] 量化；cover/contain 恒为 (0, 0)）
    pub viewport: (u32, u32),
}

/// 本拍是否该比对一次配置文件 `mtime`（热切的采样点）。
pub fn config_poll_due(ticks: u64) -> bool {
    ticks.is_multiple_of(CONFIG_POLL_TICKS)
}

/// 本拍是否该兜底探测系统主题。
///
/// 启动时先探一次（`probed == false`），之后只在「主题是 auto + 后端不认识系统主题」时
/// 按 [`SYSTEM_THEME_POLL_TICKS`] 兜底轮询。
pub fn system_theme_poll_due(probed: bool, auto: bool, backend_known: bool, ticks: u64) -> bool {
    !probed || (auto && !backend_known && ticks.is_multiple_of(SYSTEM_THEME_POLL_TICKS))
}

/// 系统主题取值（纯映射，便于单测）。
pub fn system_theme_code(theme: unlucky_core::platform::theme_probe::SystemTheme) -> i32 {
    use unlucky_core::platform::theme_probe::SystemTheme;
    match theme {
        SystemTheme::Light => SYSTEM_THEME_LIGHT,
        SystemTheme::Dark => SYSTEM_THEME_DARK,
        SystemTheme::Unknown => SYSTEM_THEME_UNKNOWN,
    }
}

/// 探测本机系统主题并给出 UI 侧属性值。
pub fn system_theme_value() -> i32 {
    system_theme_code(unlucky_core::platform::theme_probe::probe())
}

/// tile 画布的窗口尺寸（量化到 [`TILE_REGENERATE_STEP`]，避免拖动窗口时反复重算）。
///
/// 非 tile 模式恒为 (0, 0)：cover/contain 不依赖窗口尺寸，缩放只让渲染器拉伸。
pub fn tile_viewport(fill: BgFill, size: (u32, u32)) -> (u32, u32) {
    if fill != BgFill::Tile {
        return (0, 0);
    }
    (
        size.0 / TILE_REGENERATE_STEP * TILE_REGENERATE_STEP,
        size.1 / TILE_REGENERATE_STEP * TILE_REGENERATE_STEP,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        config_poll_due, system_theme_code, system_theme_poll_due, tile_viewport, BackgroundKey,
        CONFIG_POLL_TICKS, SYSTEM_THEME_DARK, SYSTEM_THEME_LIGHT, SYSTEM_THEME_POLL_TICKS,
        SYSTEM_THEME_UNKNOWN, TILE_REGENERATE_STEP,
    };
    use crate::prefs::BgFill;
    use unlucky_core::platform::theme_probe::SystemTheme;

    #[test]
    fn config_poll_fires_every_interval() {
        assert!(config_poll_due(0));
        assert!(!config_poll_due(1));
        assert!(config_poll_due(CONFIG_POLL_TICKS));
        assert!(!config_poll_due(CONFIG_POLL_TICKS + 1));
    }

    #[test]
    fn system_theme_probe_backs_off_after_first_read() {
        // 启动时无条件探一次
        assert!(system_theme_poll_due(false, false, true, 1));
        // 探过、主题非 auto：不再轮询
        assert!(!system_theme_poll_due(
            true,
            false,
            false,
            SYSTEM_THEME_POLL_TICKS
        ));
        // 探过、auto、后端认识系统主题：不再轮询
        assert!(!system_theme_poll_due(
            true,
            true,
            true,
            SYSTEM_THEME_POLL_TICKS
        ));
        // 探过、auto、后端不认识：按间隔兜底
        assert!(!system_theme_poll_due(
            true,
            true,
            false,
            SYSTEM_THEME_POLL_TICKS - 1
        ));
        assert!(system_theme_poll_due(
            true,
            true,
            false,
            SYSTEM_THEME_POLL_TICKS
        ));
    }

    #[test]
    fn system_theme_codes_are_stable() {
        assert_eq!(
            system_theme_code(SystemTheme::Unknown),
            SYSTEM_THEME_UNKNOWN
        );
        assert_eq!(system_theme_code(SystemTheme::Light), SYSTEM_THEME_LIGHT);
        assert_eq!(system_theme_code(SystemTheme::Dark), SYSTEM_THEME_DARK);
    }

    #[test]
    fn tile_viewport_quantizes_and_ignores_other_fills() {
        assert_eq!(tile_viewport(BgFill::Cover, (1180, 760)), (0, 0));
        assert_eq!(tile_viewport(BgFill::Contain, (1180, 760)), (0, 0));
        let (width, height) = tile_viewport(BgFill::Tile, (1180, 760));
        assert_eq!(width % TILE_REGENERATE_STEP, 0);
        assert_eq!(height % TILE_REGENERATE_STEP, 0);
        assert_eq!((width, height), (1152, 640));
    }

    #[test]
    fn background_key_compares_by_value() {
        let key = BackgroundKey {
            path: "a.png".to_string(),
            fill: BgFill::Tile,
            viewport: (256, 256),
        };
        assert_eq!(key.clone(), key);
        assert_ne!(
            key,
            BackgroundKey {
                path: "a.png".to_string(),
                fill: BgFill::Tile,
                viewport: (128, 256),
            }
        );
    }
}
