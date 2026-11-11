//! 两层透明度的**判据层**：整窗 α / 面板 α 的钳位链、CPU 档降级、滑块映射。纯函数、零 iced。
//!
//! ## 为什么要分成「两层」
//!
//! 用户定案（D34 / D36）：
//!
//! - **整窗 α**：整扇窗口（含自绘标题栏、导航轨、所有卡片）作为一个整体乘上 α。Windows 上唯一
//!   实现是 `SetLayeredWindowAttributes(LWA_ALPHA)`，DWM 常数乘法、成本≈0，所以**CPU 档也允许**；
//! - **面板 α**：只有面板/卡片这一层半透明，让背景图透出来。CPU 档**强制 1.0**（D36 对 D27 的
//!   修订：CPU 档被限制的是「多层半透明的合成成本」，不是窗口级常数乘法）。
//!
//! ## 真值表（三层 × 三个后端；`x` = 用户设置值）
//!
//! | 项 | GPU | CPU | auto | 下限 | 上限 | 钳位还是拒绝 |
//! |---|---|---|---|---|---|---|
//! | 整窗 α | `clamp(x, 0.08, 1.0)` | **同左**（D36 修订：CPU 也允许） | 同左 | 0.08 | 1.0 | 钳位；`x ≤ 0` 或非有限值 → **拒绝**（整窗穿透） |
//! | 面板 α | `clamp(x, 0.4, 1.0)` | **恒 1.0** | `clamp(x, 0.4, 1.0)`（auto 是探测后的择优档） | 0.4 | 1.0 | 钳位 |
//! | 玻璃 | `x` | **恒 false** | `x` | — | — | 无（布尔） |
//!
//! 「auto」按**请求值**判（`Backend::Auto` 是用户没指定、让探测决定）：它不走 CPU 的强制分支，
//! 因为「强制 1.0」是 CPU 渲染档的降级规则，而探测结果可能是 GPU。真落到 tiny-skia 时
//! `probe` 会写 `ICED_BACKEND=tiny-skia`，但那时 `Options::backend` 仍是 `Auto`——这条已知
//! 张力登记在 [`crate::pages::appearance`] 的偏离表里（要收口得让探测结果回灌 `Backend`）。
//!
//! ## 两条链为什么都在本文件
//!
//! 它们共用一个**滑块映射**与一套钳位纪律，分开写迟早会出现「滑块给 0.0、钳位给 0.08、
//! 日志又打 0.0」这种三处各说一套的事故。整窗 α 的**执行**（FFI）在
//! `crate::platform::win32::transparency`，本文件只给「该下发什么」与「下发成什么字节」。

use unlucky_presenter::prefs::WINDOW_ALPHA_MIN;

use crate::platform::effective_window_alpha;
use crate::platform::win32::transparency::alpha_byte;
use crate::Backend;

/// 整窗 α 下限的 `f32` 形态（唯一真源仍是 `unlucky_presenter::prefs::WINDOW_ALPHA_MIN`）。
pub const WINDOW_ALPHA_FLOOR: f32 = WINDOW_ALPHA_MIN as f32;

/// 面板 α 下限（core 的 `PANEL_ALPHA_MIN` = 0.4，经 `unlucky_presenter::prefs` 重导出）。
pub const PANEL_ALPHA_FLOOR: f32 = unlucky_presenter::prefs::PANEL_ALPHA_MIN as f32;

/// 整窗 α 滑块位置（0..=1）→ 请求值（0.08..=1.0）。
#[must_use]
pub fn window_alpha_from_fraction(fraction: f32) -> f32 {
    let fraction = sanitize_fraction(fraction);
    WINDOW_ALPHA_FLOOR + fraction * (1.0 - WINDOW_ALPHA_FLOOR)
}

/// 请求值 → 滑块位置（[`window_alpha_from_fraction`] 的逆；钳位后落在 0..=1）。
#[must_use]
pub fn window_fraction(alpha: f32) -> f32 {
    let alpha = clamp(alpha, WINDOW_ALPHA_FLOOR, 1.0);
    (alpha - WINDOW_ALPHA_FLOOR) / (1.0 - WINDOW_ALPHA_FLOOR)
}

/// 面板 α 滑块位置（0..=1）→ 请求值（0.4..=1.0）。
#[must_use]
pub fn panel_alpha_from_fraction(fraction: f32) -> f32 {
    let fraction = sanitize_fraction(fraction);
    PANEL_ALPHA_FLOOR + fraction * (1.0 - PANEL_ALPHA_FLOOR)
}

/// 请求值 → 滑块位置。
#[must_use]
pub fn panel_fraction(alpha: f32) -> f32 {
    let alpha = clamp(alpha, PANEL_ALPHA_FLOOR, 1.0);
    (alpha - PANEL_ALPHA_FLOOR) / (1.0 - PANEL_ALPHA_FLOOR)
}

/// 该后端是否**强制**面板不透明（D36：CPU 档面板 α / 玻璃 / 模糊一律 1.0）。
#[must_use]
pub fn panel_is_forced_opaque(backend: Backend) -> bool {
    matches!(backend, Backend::Cpu)
}

/// 面板 α 的生效值：CPU 档恒 1.0，其余钳到 `0.4..=1.0`。
#[must_use]
pub fn effective_panel_alpha(backend: Backend, requested: f32) -> f32 {
    if panel_is_forced_opaque(backend) {
        return 1.0;
    }
    clamp(requested, PANEL_ALPHA_FLOOR, 1.0)
}

/// 玻璃拟态卡片的生效值：CPU 档恒关（它要的是多层半透明 + 模糊底，那是 D27 的动机）。
#[must_use]
pub fn effective_glass(backend: Backend, requested: bool) -> bool {
    requested && !panel_is_forced_opaque(backend)
}

/// 整窗 α 最终**下发到 Win32 的字节**（`None` = 不该下发：`α ≥ 1` 或整窗穿透被拒绝）。
///
/// 这条链是「配置值 → 判据 → 量化字节」的唯一定义点：
/// `effective_window_alpha`（钳位/拒绝）→ [`alpha_byte`]（**向上取整**，永不低于下限）。
#[must_use]
pub fn window_alpha_byte(backend: Backend, requested: f32) -> Option<u8> {
    effective_window_alpha(backend, f64::from(requested))
        .filter(|alpha| *alpha < 1.0)
        .map(alpha_byte)
}

fn sanitize_fraction(fraction: f32) -> f32 {
    clamp(fraction, 0.0, 1.0)
}

fn clamp(value: f32, min: f32, max: f32) -> f32 {
    if value.is_nan() {
        min
    } else {
        value.clamp(min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        effective_glass, effective_panel_alpha, panel_alpha_from_fraction, panel_fraction,
        panel_is_forced_opaque, window_alpha_byte, window_alpha_from_fraction, window_fraction,
        PANEL_ALPHA_FLOOR, WINDOW_ALPHA_FLOOR,
    };
    use crate::Backend;

    const BACKENDS: [Backend; 3] = [Backend::Gpu, Backend::Cpu, Backend::Auto];

    #[test]
    fn floor_values_are_the_ones_the_user_decided() {
        // 抓得住：有人把下限改回 0（= 整窗穿透）或把面板下限降到「前景压不住背景图」。
        // 两个下限都来自 `unlucky_presenter::prefs` 的重导出（M93 r2 起不再有本地字面量），
        // 所以这条同时钉住「presenter 的重导出没被改坏」。
        assert_eq!(WINDOW_ALPHA_FLOOR, 0.08);
        assert_eq!(PANEL_ALPHA_FLOOR, 0.4);
    }

    /// 任务要求的**钳位链**：配置写 0.8 → 判据放行 0.8 → 下发字节 205。
    ///
    /// 抓得住：①`effective_window_alpha` 把 0.8 原样放行（CPU 档也放行，D36 修订）；
    /// ②**向上取整**这条纪律（`f32` 的 0.8 扩到 `f64` 是 0.8000000119…，×255 = 204.000003，
    /// ceil = **205**；若谁改成 `round` 就会变 204 并在这里红——那正是「实际下发可能低于下限」
    /// 的同一个坑）；③`α ≥ 1` 时**不下发**（`None`，而不是 255）。
    /// 抓不住：这 205 个字节真的被 Win32 收下（FFI 成败由 `transparency` 的单测与本机日志举证）。
    #[test]
    fn the_clamp_chain_reports_the_byte_that_would_be_sent() {
        for backend in BACKENDS {
            assert_eq!(
                window_alpha_byte(backend, 0.8),
                Some(205),
                "{backend:?}：0.8 → 205（f32→f64 略大于 0.8，ceil 上去）"
            );
            assert_eq!(
                window_alpha_byte(backend, 0.5),
                Some(128),
                "{backend:?}：0.5×255=127.5 → 128"
            );
            assert_eq!(
                window_alpha_byte(backend, 1.0),
                None,
                "{backend:?}：不透明不下发"
            );
            assert_eq!(
                window_alpha_byte(backend, 0.001),
                Some(21),
                "{backend:?}：低于下限先夹到 0.08，再向上取整 21（0.0824 ≥ 0.08）"
            );
            assert_eq!(
                window_alpha_byte(backend, -1.0),
                None,
                "{backend:?}：整窗穿透被拒绝，不给字节"
            );
            assert_eq!(window_alpha_byte(backend, f32::NAN), None);
        }
    }

    /// D36 的核心分界：**CPU 档允许整窗 α，强制面板 1.0**。
    ///
    /// 抓得住：谁把这条写反（CPU 档整窗 α 也强制 1.0，或 CPU 档面板 α 跟着用户值走）。
    /// 抓不住：真实 CPU 档下窗口是否真的半透明（观感未验，见报告「没验的部分」）。
    #[test]
    fn cpu_tier_allows_the_window_alpha_but_forces_the_panel_opaque() {
        assert!(
            window_alpha_byte(Backend::Cpu, 0.5).is_some(),
            "CPU 档允许整窗 α"
        );
        assert!(panel_is_forced_opaque(Backend::Cpu));
        assert_eq!(
            effective_panel_alpha(Backend::Cpu, 0.5),
            1.0,
            "CPU 档面板强制 1.0"
        );
        assert!(!effective_glass(Backend::Cpu, true), "CPU 档玻璃强制关");
        // 另外两档不做这个降级（否则「强制」就失去了意义）。
        for backend in [Backend::Gpu, Backend::Auto] {
            assert!(!panel_is_forced_opaque(backend), "{backend:?} 不强制");
            assert_eq!(effective_panel_alpha(backend, 0.5), 0.5);
            assert!(effective_glass(backend, true));
        }
    }

    #[test]
    fn panel_alpha_is_clamped_into_range() {
        for backend in [Backend::Gpu, Backend::Auto] {
            assert_eq!(effective_panel_alpha(backend, 0.0), PANEL_ALPHA_FLOOR);
            assert_eq!(effective_panel_alpha(backend, 0.2), PANEL_ALPHA_FLOOR);
            assert_eq!(effective_panel_alpha(backend, 3.0), 1.0);
            assert_eq!(effective_panel_alpha(backend, f32::NAN), PANEL_ALPHA_FLOOR);
        }
    }

    #[test]
    fn slider_fractions_round_trip_without_leaving_the_range() {
        for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let alpha = window_alpha_from_fraction(fraction);
            assert!(
                (WINDOW_ALPHA_FLOOR..=1.0).contains(&alpha),
                "{fraction} → {alpha}"
            );
            assert!((window_fraction(alpha) - fraction).abs() < 1e-6);
        }
        // 越界输入（滑块自己不会给，但别让下游吃 NaN）
        assert_eq!(window_alpha_from_fraction(-1.0), WINDOW_ALPHA_FLOOR);
        assert_eq!(window_alpha_from_fraction(9.0), 1.0);
        assert_eq!(window_alpha_from_fraction(f32::NAN), WINDOW_ALPHA_FLOOR);

        for fraction in [0.0, 0.5, 1.0] {
            let alpha = panel_alpha_from_fraction(fraction);
            assert!((PANEL_ALPHA_FLOOR..=1.0).contains(&alpha));
            assert!((panel_fraction(alpha) - fraction).abs() < 1e-6);
        }
        // 0.4 与 1.0 是滑块两端：端点必须精确（否则「拖到头还不是 1.0」）
        assert_eq!(panel_alpha_from_fraction(1.0), 1.0);
        assert_eq!(panel_fraction(1.0), 1.0);
        assert_eq!(window_alpha_from_fraction(1.0), 1.0);
        assert_eq!(window_fraction(WINDOW_ALPHA_FLOOR), 0.0);
    }
}
