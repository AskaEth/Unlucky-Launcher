//! **动效取样口**（M461，只在测试构建里存在）：这一帧磁贴里每格拿到多少可见度。
//!
//! ## 为什么需要它
//!
//! `Element` 没有几何内省（`grid.rs` 里 `Footprint` 的注释是同一件事），「网格真的按泵采样画了」
//! 只能由**画的那一段**自己说出来。判据读 [`Last`]：静息必须 `fading = 0`（一个遮罩都不画）、
//! `sum` 严格等于格子数（每格可见度都是 1）；过渡中间必须有格子落在 `(0, 1)` 里。
//! 用例在 `grid/tests.rs`。
//!
//! `sum` 是**可见度之和**：它把「网格的每一格用的就是泵对那个 id 的采样」压成一条可精确比较的
//! 等式（用例自己按 `motion::rows::frame` 求和，两边逐位相同），比「最小值落在某个区间」结实。
//!
//! ## 为什么单独一卷
//!
//! `grid.rs` 贴着 400 行硬线（M461 接手时 313 行，接上逐格淡入之后 405），而取样口是纯测试
//! 脚手架——搬到 `grid/probe.rs` 之后产品构建整块消失（`#[cfg(test)] mod probe;`），
//! 那一段一行不涨。与 `grid/tests.rs` 分开是因为后者只在测试构建里编、且读的是这里的口径。

use std::cell::RefCell;

/// 一次渲染的汇总。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Last {
    /// 这一帧画了几个格子
    pub(super) cells: usize,
    /// 其中可见度 `< 1.0` 的格子数（= 画了遮罩的格子数；静息帧必须是 **0**）
    pub(super) fading: usize,
    /// 最低可见度（静息 = 严格 `1.0`）
    pub(super) min_visibility: f32,
    /// 可见度之和（静息 = 格子数，逐位相等）
    pub(super) sum: f32,
}

thread_local! {
    /// **线程局部**而不是 `static Mutex`：用例并行跑，进程级取样会被别的用例覆盖
    /// （M455 的 `Footprint` 第一版踩过这一条）。
    static LAST: RefCell<Option<Last>> = const { RefCell::new(None) };
}

/// 清掉上一次渲染的累加（`tiles()` 每画一遍先清零）。
pub(super) fn reset() {
    LAST.with_borrow_mut(|slot| *slot = None);
}

/// 记下一个格子的可见度（同一次渲染里累加出 [`Last`]）。
pub(super) fn note(visibility: f32) {
    LAST.with_borrow_mut(|slot| {
        let mut sample = slot.take().unwrap_or(Last {
            cells: 0,
            fading: 0,
            min_visibility: 1.0,
            sum: 0.0,
        });
        sample.cells += 1;
        sample.sum += visibility;
        if visibility < 1.0 {
            sample.fading += 1;
        }
        sample.min_visibility = sample.min_visibility.min(visibility);
        *slot = Some(sample);
    });
}

/// 上一次渲染的取样（`None` = 还没渲染过）。**读之前要有一次渲染**。
pub(super) fn last() -> Option<Last> {
    LAST.with_borrow(|last| *last)
}
