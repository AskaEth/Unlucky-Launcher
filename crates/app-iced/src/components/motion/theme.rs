//! 主题交叉淡入的**公开采样口**（M313）：`Ui` 只拿得到「这一帧用哪套方案」。
//!
//! ## 谁起跑 / 谁采样
//!
//! | 侧 | 函数 | 干什么 |
//! |---|---|---|
//! | update | [`observe`] | 报「从哪块板 → 到哪块板」；轨道与时长在泵里（[`super::pump`]） |
//! | update | [`frame`] | 这一帧的 49 角色方案 + 进度（`App::sync_theme_fade` 每帧重建 `Ui`） |
//!
//! **起跑必须在 update 侧**：iced 先算订阅表、后建视图（`app/subscription.rs` 的模块文档有源码
//! 行号），在 `view` 里插轨道当拍订阅不上——那是 M192 r1 复审 P1-1 的原始缺陷。
//!
//! ## 为什么采样口给的是**整套方案**而不是一个进度数
//!
//! 页面层不该知道「哪几个角色要插值」：给进度就必然有人只插其中几个（底色走了、文字没走，
//! 那正是「闪切一半」）。这里一次给全 49 个角色，产物恒不透明（见 `pump/theme.rs` 的 `crossfade`）。
//!
//! ## `MotionScale::NONE`（`ULL_MOTION=off` / 截图通道）
//!
//! [`observe`] 不插轨道，但**记下终点板** ⇒ [`frame`] 从第一帧起就给终点方案
//! （`progress = 1.0`、`running = false`）。关掉动效 = 瞬时切到**最终值**，不是跑一遍 0 时长。

use unlucky_design::color::Scheme;

pub use super::pump::ThemeFrame;
use super::PUMP;

/// **update 侧**：换主题（`from → to`）。返回 `true` = 真的起跑了淡入（帧基准由本模块重置）。
///
/// 关着动效时返回 `false` 且只记账（终点板），所以调用方不需要自己判档——照常调即可。
pub fn observe(from: Scheme, to: Scheme) -> bool {
    let started = PUMP.with(|pump| pump.borrow_mut().observe_theme(from, to));
    if started {
        super::reset_frame_clock();
    }
    started
}

/// 这一帧该用哪套方案（**只采样、无副作用**）。
///
/// `None` = 这个进程还没换过主题（调用方保持 `rebuild_ui` 刚算好的 `Ui`，一帧都不动）。
pub fn frame() -> Option<ThemeFrame> {
    PUMP.with(|pump| pump.borrow().theme_frame())
}

/// 淡入是否在跑（日志与断言用；`needs_frames` 走的是泵的 `is_animating`）。
#[allow(dead_code)]
pub fn running() -> bool {
    PUMP.with(|pump| pump.borrow().theme_animating())
}

/// 相位是否被钉住（`ULL_MOTION_AT` 取证通道；产品路径恒 `false`）。
///
/// 起跑判据要用它：运行期换主题有「上一帧已经画过」这件事可用，**启动期没有**（首帧还没画过
/// 任何东西），所以首帧只允许取证通道起跑（见 `app/state/theme.rs::rebuild_ui`）。
pub fn pinned() -> bool {
    PUMP.with(|pump| pump.borrow().pinned())
}
