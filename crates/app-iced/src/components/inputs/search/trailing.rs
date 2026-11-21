//! 药丸的**尾随位**（M460）：`Filter` 图标 + 可选的**尾随动作**，两者同一侧、不重叠。
//!
//! ## 为什么单开一卷
//!
//! `search.rs` 贴着 400 行硬线（AGENTS.md §3-1），而这一段的规矩比「画一个图标」多两条：
//!
//! | 规矩 | 依据 |
//! |---|---|
//! | 动作与输入框**同一个容器**（`bar_content` 的 `contents` 行） | M460 判据 ①：叉不许再画在药丸外面 |
//! | 动作排在 `Filter` 图标**之后**（同一侧、不重叠） | `20.4`：尾随图标位只有一处，两个都要就得都进去 |
//! | 字形右缘仍落在 `pad_end + icon_offset` = 16 上 | M69 不变量（两侧对称） |
//!
//! ## 第三条为什么需要一个减法
//!
//! `buttons::icon` 的框是 **40**（`LayoutMetrics::control_height`），字形 24 居中，
//! 于是框里自带 8 的透明边（`IconGeometry::padding`）。整圆贴到 `pad_end`(12) 会让字形右缘落在
//! 20 上——比前置图标（16）多缩 4，两块并排能看出来。所以「有动作」时右内边距取
//! `pad_end − icon_offset`：8 + 8 = 16，回到同一条线。两个数都是令牌字段，这里仍然只做减法。
//!
//! ## 为什么不在这里画图标
//!
//! 图标盒（`icons::view` / `layout::fixed`）留在 `search.rs`：`icons.rs` 的
//! `sub_24_boxes_go_through_view_at` 按**文件**计数 `icons::view` 的调用点（白名单未列出 = 0），
//! 搬进来就得改那张表——那不是观感问题的活。所以本模块只**摆位置**，图标由调用方建好递进来。
//!
//! ## 取样口
//!
//! iced 0.14 的 `Element` 没有内省（`instances/search.rs` 的同一句说明），所以「叉这一次是不是
//! **画在药丸里**」只能由画的那一段自己说：[`action_in_field`] 记的是**上一次 [`trailing`] 调用**
//! 有没有把动作拼进输入行。页面把叉挪回药丸外面时它变 `Some(false)`。

use iced::widget::Row;
use iced::Alignment;

use unlucky_design::component_map::ComponentMetrics;
use unlucky_design::icons::IconKind;

use crate::app::components::buttons::{self, IconVariant};
use crate::theme::Ui;

/// 尾随位拼进 `contents` 行的那一段，以及外层药丸该用的右内边距。
pub(super) struct Trailing<'a, M> {
    /// 图标槽 / 动作按钮（顺序即视觉顺序：先 `Filter`，后动作）。
    pub row: Row<'a, M>,
    /// 外层药丸容器的右内边距（理由见模块文档第三条）。
    pub pad_end: f32,
}

/// 造尾随位：`icon` = 组件自带的尾随图标（收起态是 `Filter`，由 `search.rs` 建好递进来），
/// `action` = 页面给的动作消息。
///
/// `action` 为 `None` 时行为与 M460 之前**逐像素相同**（只有一个图标槽），所以下载页那类
/// 不传动作的调用方一个像素都不动。
pub(super) fn trailing<'a, M: Clone + 'a>(
    ui: &'a Ui,
    bar: ComponentMetrics,
    icon: Option<iced::Element<'a, M>>,
    action: Option<M>,
) -> Trailing<'a, M> {
    let mut row = Row::new().align_y(Alignment::Center).spacing(bar.gap);
    if let Some(icon) = icon {
        row = row.push(icon);
    }

    // 取样口要在 `action` 被 move 之前取值（非测试构建里这一行整条消失）。
    #[cfg(test)]
    let present = action.is_some();

    let mut pad_end = bar.pad_end;
    if let Some(message) = action {
        row = row.push(buttons::icon(
            ui,
            IconVariant::Standard,
            IconKind::Close,
            Some(message),
            None,
        ));
        // 40 的框里字形居中：右内边距减掉图标偏移，字形回到 `pad_end + icon_offset`。
        pad_end = bar.pad_end - bar.icon_offset;
    }
    #[cfg(test)]
    note_action(present);

    Trailing { row, pad_end }
}

// ---------------------------------------------------------------- 判据取样口

// 上一次 [`trailing`] 调用有没有把动作拼进输入行。**线程局部**而不是 `static`：用例并行跑，
// 进程级的槽会让两条用例互相覆盖（`instances/grid.rs` 的 `LAST` 是同一个教训）。
#[cfg(test)]
thread_local! {
    static LAST_ACTION: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn note_action(mounted: bool) {
    LAST_ACTION.with(|cell| cell.set(Some(mounted)));
}

/// 上一次渲染药丸时，尾随动作**有没有画在输入行里**（`None` = 还没渲染过药丸）。
///
/// `Some(true)` 的完整含义是：这一帧在 `bar_content` 的 `contents` 行里拼进了一个
/// **挂了 `on_press`** 的按钮（`trailing` 里 `action` 是 `Some` 才会走到那一支）。
#[cfg(test)]
pub fn action_in_field() -> Option<bool> {
    LAST_ACTION.with(std::cell::Cell::get)
}
