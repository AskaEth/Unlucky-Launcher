//! M173：设置页表单列的**滚动条槽**（滚动条并排，不压内容）。
//!
//! 这里守的是整页级别的数值契约，不是「哪一组有哪些行」——所以它不属于任何 section 文件。
//!
//! ## 怎么量
//!
//! iced 的控件树支持在测试里真跑布局：`iced_core` 在 debug 构建下给 `()` 实现了全套 `Renderer`
//! trait（`iced_core-0.14.0/src/renderer/null.rs`），于是 `frame::scroll::<()>(…)` 返回的
//! `Element<_, Message, Theme, ()>` 能被 `Widget::layout` 量出**真的**子节点尺寸。不需要开窗，
//! 也不需要 GPU——这是「数值断言，不是看图」的落点。
//!
//! 节点树（`iced_core/src/layout.rs:201`：`padded` 恒为「自己 + 一个子节点」）：
//!
//! ```text
//! scrollable → [槽容器（Fill，右内边距 = 槽宽） → [内容]]
//! ```
//!
//! ## 抓什么 / 抓不住什么
//!
//! 抓得住：内容右侧真的留出了「轨宽 + 两侧 margin」的槽；长/短页面（出/不出滑块）内容宽度与左
//! 边界**逐项相同**；把 `app::components::scroll` 的 padding 改回 0，本文件立刻红。
//! 抓不住：滑块画在槽里的**像素**位置（那由 `pages/settings` 的出图与 iced 自己的 `Rail::new`
//! 公式担保，见 `src/components/scroll.rs` 文档里的源码行号）。

use iced::advanced::layout::Limits;
use iced::advanced::widget::Tree;
use iced::widget::{container, space};
use iced::{Element, Length, Rectangle, Size, Theme};
use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;

use super::super::frame;
use super::super::frame::{SCROLLBAR_MARGIN, SCROLLBAR_WIDTH};
use crate::pages::Message;
use crate::theme::{Families, Ui};

/// 设置页表单列宽：`frame_metrics` 的 `column`（= `measure_cap_px()`，1180 与 880 窗口下都是它）。
const COLUMN: f32 = 424.0;
/// 视口高（1180×760 减去标题栏 / 页头 / 上下页边距之后的量级；只要求「长页 > 它」「短页 < 它」）。
const VIEWPORT_HEIGHT: f32 = 600.0;
/// 长页面（正文远超视口 ⇒ 一定出滑块）。
const LONG: f32 = 4000.0;
/// 短页面（正文装得下 ⇒ 一定不出滑块）。
const SHORT: f32 = 120.0;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

/// 滚动条槽宽（dp）：轨道 + 两侧留白，与 iced 的 `total_scrollbar_width` 同口径。
fn slot() -> f32 {
    SCROLLBAR_WIDTH + 2.0 * SCROLLBAR_MARGIN
}

/// 真跑一遍布局，返回 (槽容器, 内容) 两个矩形。
fn laid_out(content_height: f32) -> (Rectangle, Rectangle) {
    let ui = ui();

    let filler: Element<'_, Message, Theme, ()> = container(space())
        .width(Length::Fill)
        .height(Length::Fixed(content_height))
        .into();

    // 走的是**产品那条** `frame::scroll`（泛型只为换成 `()` 渲染器，几何一字不改）。
    let mut element = frame::scroll::<()>(&ui, filler);
    let mut tree = Tree::new(element.as_widget());

    let node = element.as_widget_mut().layout(
        &mut tree,
        &(),
        &Limits::new(Size::ZERO, Size::new(COLUMN, VIEWPORT_HEIGHT)),
    );

    let gutter = node.children()[0].bounds();
    let content = node.children()[0].children()[0].bounds();

    (gutter, content)
}

/// **本条 mission 的核心数值断言**：内容可用宽度 = 容器宽 − 滚动条占位。
#[test]
fn the_form_column_reserves_a_rail_slot_beside_the_content() {
    let (gutter, content) = laid_out(LONG);

    assert_eq!(slot(), 10.0, "6 轨宽 + 2×2 内缩 = 10dp 的槽");
    assert_eq!(gutter.width, COLUMN, "槽容器自身仍是满宽");
    assert_eq!(
        content.width,
        COLUMN - slot(),
        "内容可用宽度必须是「容器宽 − 滚动条槽」，否则轨道就压在内容右缘上"
    );
    assert_eq!(content.width, 414.0, "424 − 10 = 414");
}

/// 长页面（出滑块）与短页面（不出滑块）——内容宽度与左边界必须**逐项相同**。
///
/// 这是 M173 要求的「两张对照图 + 数值」里的数值那一半：只给一张出滑块的图说明不了「滑块出现时
/// 内容会不会跳」，必须两态同框比。
#[test]
fn the_content_does_not_jump_between_a_long_and_a_short_page() {
    let (long_gutter, long) = laid_out(LONG);
    let (short_gutter, short) = laid_out(SHORT);

    assert_eq!(long_gutter.x, short_gutter.x, "槽容器的左边界不许变");
    assert_eq!(long_gutter.width, short_gutter.width, "槽容器的宽度不许变");
    assert_eq!(long.x, short.x, "左边界不许横跳");
    assert_eq!(long.width, short.width, "宽度不许横跳");
    assert_eq!(short.x, 0.0, "槽只占右侧：左侧内边距为 0");
    assert_eq!(
        short.x + short.width,
        COLUMN - slot(),
        "内容右缘 = 轨道的左缘（再往右 10dp 才是轨道自己）"
    );
}

/// 反证：`content_width` 这条纯量与实际布局量出来的是同一个数（两处不许各算一套）。
#[test]
fn the_layout_agrees_with_the_pure_quantity() {
    let (_, content) = laid_out(LONG);
    assert_eq!(
        content.width,
        crate::app::components::scroll::content_width(COLUMN, slot())
    );
}
