//! 进度插值的**公开采样口**（M305）：页面只拿得到「这一帧这根条到几成」。
//!
//! ## 谁起跑 / 谁采样
//!
//! | 侧 | 函数 | 干什么 |
//! |---|---|---|
//! | update | [`observe`] | 报「哪一根条 + 真实进度」；差分与起跑在泵里 |
//! | view | [`frame`] | 这一帧画出来的值（**追值**，不是整数值硬跳）+ 收束强度 |
//!
//! **为什么 `key` 与 `target` 两个参数都要传**：泵记的是「上次报的 key + 目标」，视图报的是
//! 这一帧模型里的「key + 目标」。两者不一致 ⇒ 有人改了模型却没走 [`observe`]（截图 harness、
//! 单测直接构造模型、或将来某个新入口忘了接线）⇒ 那时 [`value`] **直接返回目标**：
//! 宁可无动效，也不许把界面卡在中间值（同 `motion::rail` 的口径）。
//!
//! ## `MotionScale::NONE`
//!
//! [`observe`] 不插轨道 ⇒ 值严格等于目标、收束恒为 0 ⇒ `needs_frames()` 不会
//! 因为进度变化而为真。

use super::pump::ProgressFrame;
use super::PUMP;

/// **update 侧**：报「哪一根条（`key`）+ 真实进度」。返回 `true` = 真的起跑了追值。
pub fn observe(key: &str, target: f32) -> bool {
    let started = PUMP.with(|pump| pump.borrow_mut().observe_progress(key, target));
    if started {
        super::reset_frame_clock();
    }
    started
}

/// 一帧的完整进度采样（`value` + `settle`）：页面一次取完，避免两次进 thread-local。
pub fn frame(key: &str, target: f32) -> ProgressFrame {
    PUMP.with(|pump| {
        let pump = pump.borrow();
        ProgressFrame {
            value: pump.progress_value(key, target),
            settle: pump.progress_settle(key, target),
        }
    })
}
