//! Termux / termux-x11 下的窗口尺寸自适应（横屏优先）。
//!
//! 验收尺寸（1180×760）是 D30 的硬要求，不能改；但手机显示器可能横屏远大于它
//! （例如 2066×1272）或竖屏略小。这里只在**运行时**把「默认尺寸」换成按当前 X
//! 屏幕算出的最大等比矩形；`ULL_WINDOW_SIZE` 覆盖仍然优先（见 `acceptance::window_size`）。
//!
//! 只在 Termux + 有 `DISPLAY` 时探测；`xdpyinfo` 缺失/解析失败一律回默认，不影响其它平台。

use std::process::Command;

use unlucky_core::platform::paths::ProcessEnv;
use unlucky_core::platform::termux;

/// 屏幕四周留白（像素）。
const MARGIN: f32 = 32.0;
/// 最大放大倍数：大屏上也不把窗口撑成巨屏。
const MAX_SCALE: f32 = 2.0;
/// `xdpyinfo` 里尺寸那一行的前缀。
const DIMENSIONS_PREFIX: &str = "dimensions:";

/// Termux 上的默认窗口尺寸：按 X 屏幕等比放大/缩小，失败返回 `None`（用调用方默认）。
///
/// `default` / `min` 由调用方传入（[`super::WINDOW_SIZE`] / [`super::MIN_SIZE`]），
/// 这里不复制常量。
#[must_use]
pub(super) fn default_size(
    default: (f32, f32),
    min: (f32, f32),
) -> Option<((f32, f32), &'static str)> {
    if !termux::is_termux(&ProcessEnv) || std::env::var_os("DISPLAY").is_none() {
        return None;
    }
    let screen = x_screen_size()?;
    Some((fit_screen(default, min, screen), "termux-screen"))
}

/// 在 `screen` 里放一个保持 `default` 宽高比的最大矩形（保留留白，不突破 `min`）。
#[must_use]
pub(super) fn fit_screen(default: (f32, f32), min: (f32, f32), screen: (f32, f32)) -> (f32, f32) {
    if screen.0 <= 0.0 || screen.1 <= 0.0 || default.0 <= 0.0 || default.1 <= 0.0 {
        return default;
    }
    let available = (
        (screen.0 - MARGIN * 2.0).max(1.0),
        (screen.1 - MARGIN * 2.0).max(1.0),
    );
    let scale = (available.0 / default.0)
        .min(available.1 / default.1)
        .min(MAX_SCALE);
    let width = (default.0 * scale).max(min.0).min(screen.0);
    let height = (default.1 * scale).max(min.1).min(screen.1);
    (width, height)
}

/// 当前 X 屏幕尺寸（`xdpyinfo` 的 `dimensions: WxH pixels`）。
fn x_screen_size() -> Option<(f32, f32)> {
    let output = Command::new("xdpyinfo").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with(DIMENSIONS_PREFIX))?;
    let value = line.split_whitespace().nth(1)?;
    let (width, height) = value.split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::fit_screen;

    const DEFAULT: (f32, f32) = (1180.0, 760.0);
    const MIN: (f32, f32) = (880.0, 560.0);

    fn close(actual: (f32, f32), expected: (f32, f32)) {
        assert!(
            (actual.0 - expected.0).abs() < 0.5 && (actual.1 - expected.1).abs() < 0.5,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn landscape_screen_scales_up_keeping_aspect() {
        // 2066×1272：留白后 2002×1208，高度是瓶颈 → 1875.6×1208。
        close(
            fit_screen(DEFAULT, MIN, (2066.0, 1272.0)),
            (1875.58, 1208.0),
        );
    }

    #[test]
    fn portrait_screen_still_fits_width() {
        // 1272×2066：宽度是瓶颈 → 1208×778。
        close(fit_screen(DEFAULT, MIN, (1272.0, 2066.0)), (1208.0, 778.03));
    }

    #[test]
    fn huge_screen_is_capped_at_two_times() {
        close(fit_screen(DEFAULT, MIN, (4000.0, 3000.0)), (2360.0, 1520.0));
    }

    #[test]
    fn tiny_screen_never_goes_below_min_or_overflow() {
        let (width, height) = fit_screen(DEFAULT, MIN, (800.0, 600.0));
        assert_eq!(width, 800.0, "宽度受屏幕限制");
        assert_eq!(height, 560.0, "高度不小于最小尺寸");
    }

    #[test]
    fn invalid_screen_falls_back_to_default() {
        assert_eq!(fit_screen(DEFAULT, MIN, (0.0, 0.0)), DEFAULT);
    }
}
