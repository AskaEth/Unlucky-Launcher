//! 一屏的版面几何（`FrameMetrics`）+ 一次算齐它的 [`frame_metrics`]。
//!
//! M357（行数门禁）从 `layout.rs` 原样搬出：那边加上提示条那一项（`banner`）之后到 416 行，
//! 越过了 AGENTS.md §3-1 的 400 行硬线。**内容一个字没改**，只是搬了家——`layout.rs` 用
//! `pub use` 把它们再导出一遍，`pages::layout::FrameMetrics` / `frame_metrics` 两条路径没变。

use super::{
    available_width, measure_cap_px, meta_columns, resolve, PageLayout, PaneKit, WidthClass,
    LIST_COLUMN_WIDTH, PANES_GAP, SIDE_NAV_WIDTH,
};

/// 一屏的版面几何（页面直接消费，不再自己算列宽）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameMetrics {
    /// 窗口宽度（输入）。
    pub window_width: f32,
    /// 宽度档位。
    pub class: WidthClass,
    /// 决议出来的版面。
    pub layout: PageLayout,
    /// 页边距。
    pub gutter: f32,
    /// 页面可用宽度（已扣导航轨与页边距）。
    pub available: f32,
    /// 正文列宽（≤ 行宽上限）。两列版面时是**主列**的正文列宽。
    pub column: f32,
    /// 第二列（list-detail 的列表列 / supporting-pane 的侧列）。
    pub secondary: Option<f32>,
    /// 主列所在 pane 的宽度（单列 = 正文列宽）。
    pub main_pane: f32,
    /// 详情元信息网格列数。
    pub meta_columns: usize,
    /// **提示条吃掉的高度**（M357）：自更新提示条（含它下面的 `Space::X3` 间距）从页面高度
    /// 预算里占掉的量，`0.0` = 这一帧没有提示条。
    ///
    /// 为什么挂在 [`FrameMetrics`] 上而不是给每个页面的 `view` 加一个形参：提示条是**页面外壳**
    /// 的一部分（与标题栏、页头带同类），装配点只有 `pages::mod.rs::body` 一处；挂在几何里之后
    /// `pages::frame.rs::body_height` 与每一页读的是同一个数，没有第二处需要同步。
    pub banner: f32,
}

impl FrameMetrics {
    /// 灌入提示条高度（由 `pages::mod.rs::body` 在装配时调用；其它入口——harness / 单测——没有提示条）。
    #[must_use]
    pub fn with_banner(mut self, banner: f32) -> Self {
        self.banner = banner.max(0.0);
        self
    }
}

/// 一次算齐版面几何。
pub fn frame_metrics(kit: PaneKit, window_width: f32) -> FrameMetrics {
    let class = WidthClass::of(window_width);
    let gutter = class.gutter();
    let available = available_width(window_width);
    let layout = resolve(kit, window_width);

    let (column, secondary, main_pane) = match layout {
        PageLayout::SingleColumn => {
            let column = available.min(measure_cap_px());
            (column, None, column)
        }
        PageLayout::ListDetail => {
            let list = LIST_COLUMN_WIDTH.min(available);
            let pane = (available - list - PANES_GAP).max(0.0);
            (pane.min(measure_cap_px()), Some(list), pane)
        }
        PageLayout::SupportingPane => {
            let side = SIDE_NAV_WIDTH.min(available);
            let pane = (available - side - PANES_GAP).max(0.0);
            (pane.min(measure_cap_px()), Some(side), pane)
        }
    };

    FrameMetrics {
        window_width,
        class,
        layout,
        gutter,
        available,
        column,
        secondary,
        main_pane,
        meta_columns: meta_columns(column),
        banner: 0.0,
    }
}
