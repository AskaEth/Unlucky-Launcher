//! 轨宽的**当帧活值**（M301）：`available_width` / `frame_metrics` 扣掉的那一段宽度。
//!
//! ## 为什么不再是常量
//!
//! M300 给了导航轨两个宽度档：展开 `LayoutMetrics::nav_rail_width` = **88**、折叠
//! `nav_collapsed_width` = **64**，中间按 `medium1` / `short4` 插值。若这里仍是
//! `const fn` 恒返回 88，用户折叠后页面照旧按 88 扣宽 ⇒ 内容区右侧留一条 **24px 空白带**，
//! 「折叠腾出空间」只完成一半（M300 登记的偏离，本条收口）。
//!
//! ## 顺序契约（谁必须在谁之前）
//!
//! | 时刻 | 谁 | 做什么 |
//! |---|---|---|
//! | 每次 `App::view()` 开头 | 外壳 `app/view/rail.rs` 的 `rail_account()` | **写**：`note_rail_width(RailAccount::nav_width)` |
//! | 同一次 `view()` 里、之后 | 页面几何（`available_width` / `frame_metrics` / `pages/agent/metrics.rs`） | **读**：`rail_width()` |
//!
//! 「先写后读」是**代码顺序**保证的：`app/view.rs` 的 `let account = self.rail_account();`
//! 排在页面树（`page_with_settings_transition`）之前，所以页面扣的宽度与外壳画的轨**同一帧同一个数**。
//!
//! ## 为什么这份采样不是「第二份状态」
//!
//! 折叠的**决定**只有一份（presenter 的 `UiPrefs::nav_collapsed`，落盘 `ui.nav.collapsed`）；
//! 过渡进度也只有一份（`components/motion` 的左轨轨道）。本模块存的是「这一帧外壳真画出来的那个
//! 宽度」，是 [`crate::pages::layout`] 的**只读当帧输入**：没有任何代码把它写回状态、也不跨帧
//! 做任何判断，所以两处不会漂。
//!
//! ## 为什么是 thread-local
//!
//! 它是**当帧采样**，不是应用状态：没有业务语义、不需要跨线程或跨进程，用例天然隔离（一个用例
//! 一个线程）。仓内同样的先例：`components/motion`（时间轴）、`pages/appearance/probe.rs`。
//!
//! ## `None` 的语义：本线程还没有外壳采样过 ⇒ 回落展开档
//!
//! 没有外壳的通道（`pages_demo`、组件库 harness、直接调 `frame_metrics` 的单测）里，轨本身是
//! **组件版** `pages/nav.rs` 画的（只有 88 / 240 两态，没有折叠档），所以页面也必须按 88 扣宽。
//! 于是读不到值时回落 `nav_rail_width`（不 panic、不 unwrap），这条由
//! `pages/tests/rail_width.rs::no_shell_channel_falls_back_to_the_expanded_token` 钉住。
//!
//! ## 为什么没有「一帧后自动失效」
//!
//! 自动失效要先知道「哪一拍是新帧」，而帧边界是 `App::view()` 的开头——那里不在本条的 scope 内。
//! 折中成两条可断言的性质：①外壳**每帧覆盖**（写入口只有一个，见 `rail_account()`）；
//! ②没采过样的通道回落 88。副作用说明白：同一线程上先跑过 `App::view()`（折叠档）再跑无外壳
//! 通道，会读到上一次采样——`#[cfg(test)]` 的 `forget_rail_width`（本文件）是给这种混跑场景的显式复位。

use std::cell::Cell;

use unlucky_design::spacing::LayoutMetrics;

/// 展开档（`nav_rail_width` = 88）。
pub fn rail_width_expanded() -> f32 {
    LayoutMetrics::DESKTOP.nav_rail_width
}

/// 折叠档（`nav_collapsed_width` = 64）。算式与「为什么不是 48」见 `design/spacing.rs` 的令牌文档。
pub fn rail_width_collapsed() -> f32 {
    LayoutMetrics::DESKTOP.nav_collapsed_width
}

thread_local! {
    /// 这一帧的轨宽：`None` = 本线程还没有外壳采样过。
    ///
    /// clippy 1.99 的 `missing_const_for_thread_local` 对**已经是** `const { … }` 的初始化也报
    /// （换过写法照报，属于误报；`components/motion` 等三处已有同样的 `allow`）。
    #[allow(clippy::missing_const_for_thread_local)]
    static SAMPLE: Cell<Option<f32>> = const { Cell::new(None) };
}

/// 这一帧的轨宽（逻辑 px）。未采样 ⇒ 回落展开档令牌 88。
pub fn rail_width() -> f32 {
    SAMPLE.with(Cell::get).unwrap_or_else(rail_width_expanded)
}

/// 外壳每帧采一次（**唯一写入口**：`app/view/rail.rs` 的 `rail_account()`，
/// `pages/tests/rail_width.rs` 扫源码守这条）。
///
/// 非有限值（NaN / inf）与负值**不写入**：布局跟着一条坏数走会算出负的可用宽，
/// 而那比「这一帧沿用上一帧」糟得多。静默忽略而不是 `assert`——帧路径上不许 panic。
pub(crate) fn note_rail_width(width: f32) {
    if width.is_finite() && width >= 0.0 {
        SAMPLE.with(|sample| sample.set(Some(width)));
    }
}

/// 丢掉本线程的采样（回到「未采样」= 回落 88）。
///
/// 产品路径不用它（外壳每帧覆盖）；它给的是两条判据：①`None` 的回落行为可断言；
/// ②「先跑过一次外壳再跑无外壳通道」的单测可以显式复位，不必依赖线程调度。
#[cfg(test)]
pub(crate) fn forget_rail_width() {
    SAMPLE.with(|sample| sample.set(None));
}

#[cfg(test)]
mod tests {
    //! 采样槽自身的三条判据（几何面在 `pages/tests/rail_width.rs`）。

    use super::*;

    /// 未采样 ⇒ 回落 88，且**不 panic**（`None` 就是「本帧没有外壳」）。
    #[test]
    fn an_unwritten_sample_falls_back_to_the_expanded_token() {
        forget_rail_width();
        assert_eq!(rail_width(), rail_width_expanded());
        assert_eq!(rail_width(), 88.0, "回落值就是展开档令牌");
    }

    /// 写进去什么就读到什么；两个端值分别等于两个令牌（不是手抄的字面量）。
    #[test]
    fn the_sample_is_read_back_verbatim() {
        forget_rail_width();
        note_rail_width(rail_width_collapsed());
        assert_eq!(rail_width(), rail_width_collapsed());
        note_rail_width(rail_width_expanded());
        assert_eq!(rail_width(), rail_width_expanded());
        forget_rail_width();
    }

    /// 坏数不写：NaN / inf / 负数被忽略（沿用上一帧），而不是把几何拖成负宽。
    #[test]
    fn a_non_finite_or_negative_sample_is_ignored() {
        forget_rail_width();
        note_rail_width(f32::NAN);
        assert_eq!(rail_width(), rail_width_expanded(), "NaN 不许写进去");
        note_rail_width(f32::INFINITY);
        assert_eq!(rail_width(), rail_width_expanded(), "inf 不许写进去");
        note_rail_width(-8.0);
        assert_eq!(rail_width(), rail_width_expanded(), "负宽不许写进去");
        forget_rail_width();
    }
}
