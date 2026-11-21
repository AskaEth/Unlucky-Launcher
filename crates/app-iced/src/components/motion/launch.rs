//! 启动成败交叉淡入的**公开采样口**（M314 / A6）：页面只拿得到「这一帧状态行是哪两档之间的多少」。
//!
//! ## 谁起跑 / 谁采样
//!
//! | 侧 | 函数 | 干什么 |
//! |---|---|---|
//! | update | [`observe`] | 报「这个实例的状态行这一拍是哪一档（哪个角色）」；轨道与时长在泵里（[`super::pump`]） |
//! | view | [`frame`] | 这一帧的 `from` / `to` / 进度（`pages/instances/detail.rs` 每帧采样一次） |
//!
//! **起跑必须在 update 侧**：iced 先算订阅表、后建视图（`app/subscription.rs` 的模块文档有源码
//! 行号），在 `view` 里插轨道当拍订阅不上——那是 M192 r1 复审 P1-1 的原始缺陷。
//!
//! ## 为什么给的是「两端角色 + 进度」而不是一个颜色
//!
//! 主题交叉淡入（M313）期间 `Ui` 每帧都在变，泵里记下的实色会与这一帧的板对不上（红句不跟着
//! 换肤）。记**角色**、由页面用这一帧的 `Ui` 求值再合成（`interaction::layer::blend`，端点逐位
//! 精确、中间帧不透明），两件事就天然同源。
//!
//! ## `MotionScale::NONE`（`ULL_MOTION=off` / 截图通道）
//!
//! [`observe`] 不插轨道，但**记目标角色** ⇒ [`frame`] 从第一帧起就给 `progress = 1.0`、
//! `running = false`。关掉动效 = 瞬时切到**最终值**，不是跑一遍 0 时长。
//!
//! ## 没走过 [`observe`] 的模型
//!
//! [`frame`] 返回 `None`（演示数据 / `pages_demo` / 单测直接构造模型）。页面那时用自己的兜底
//! 角色画——**不许**猜一个中间值，也不许为它起轨道。

use unlucky_design::color::Role;

pub use super::pump::LaunchFrame;
use super::PUMP;

/// **update 侧**：这一拍状态行的档（`key` = 实例 id，`tone` = `None` 表示这一档不画状态行）。
///
/// 返回 `true` = 真的起跑了淡入（帧基准由本模块重置）。关着动效时返回 `false` 且只记账，
/// 所以调用方不需要自己判档——照常调即可。
pub fn observe(key: &str, tone: Option<Role>) -> bool {
    let started = PUMP.with(|pump| pump.borrow_mut().observe_launch(key, tone));
    if started {
        super::reset_frame_clock();
    }
    started
}

/// 这一帧的状态行采样（**只采样、无副作用**）。
///
/// `None` = 这份账不认 `key`（没走过 [`observe`]）或这一档没有状态行。
pub fn frame(key: &str) -> Option<LaunchFrame> {
    PUMP.with(|pump| pump.borrow().launch_frame(key))
}

/// 淡入是否在跑（日志与断言用；`needs_frames` 走的是泵的 `is_animating`）。
#[allow(dead_code)]
pub fn running() -> bool {
    PUMP.with(|pump| pump.borrow().launch_animating())
}
