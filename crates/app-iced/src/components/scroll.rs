//! 滚动条**并排**在内容右侧，不压在内容上（M173）。
//!
//! ## 根因（读 iced 0.14 的源码，不是猜测）
//!
//! `scrollable` 的竖直滚动条是**浮层**，而且内容区不扣任何横向内边距：
//!
//! - 内容拿到的约束仍是整个视口宽——`iced_widget-0.14.2/src/scrollable.rs:437` 那条
//!   `layout::padded(limits, self.width, self.height, Padding::ZERO, …)`：`Scrollbar::spacing`
//!   默认 `None`，右内边距**恒 0**；
//! - 轨道按视口右边缘定位——同文件 `scrollable.rs:1970-1980`
//!   `x = bounds.x + bounds.width - total_scrollbar_width`，`total_scrollbar_width =
//!   width.max(scroller_width) + 2 * margin`。
//!
//! 于是轨道（本仓 6 + 2×2 = **10dp**）整条压在内容最右边那 10dp 上。屏幕上的观感就是
//! 「滚动条骑在卡片右边缘」。
//!
//! ## 为什么不用 iced 自己的 `Scrollbar::spacing(Some(..))`
//!
//! 它**只在滑块可见时**才预留（`scrollable.rs:478-500`：`if state.is_scrollbar_visible
//! { padding } else { 0.0 }`）。长页面 ⇄ 短页面切换时内容宽度会跳 10dp，卡片会横移——
//! 那是另一个缺陷（`M173` 的判据要求「有无滚动条时内容宽度一致」）。
//!
//! ## 本模块的做法：恒定槽位
//!
//! 内容外面包一层「右侧恒留 `slot`」的容器，再交给 `scrollable`：
//!
//! ```text
//! 视口  0 ───────────────────────── 视口宽 − slot │ slot │
//!       │            内容（可滚）                 │ 轨道 │
//! ```
//!
//! `内容可用宽度 = 视口宽 − slot` 与滑块在不在**无关**（因为内边距来自本模块，不来自 iced 的
//! 可见性判断）。`slot = rail_width + 2 * margin` 与 iced 的 `total_scrollbar_width` 同口径，
//! 轨道整个落在槽里，滑块再内缩 `margin`，所以内容与滑块之间永远隔着 `margin`。
//!
//! 逻辑像素（dp）：高 DPI 下 iced 按缩放因子整体放大，槽与轨道同比例，二者不会重新重叠。
//!
//! ## 谁该用
//!
//! 所有「内容宽度吃满视口」的竖直滚动区。设置页（`pages/settings/frame.rs`）是第一个用户；
//! 实例页列表列 / 详情列、日志面板、内容库正文、版本管理侧栏目前仍是 `scrollable(...)` 裸调，
//! 同一处重叠在那些页面上照样存在（M173 的 finding 里有清单）。

use iced::widget::{container, scrollable};
use iced::{Element, Length, Padding, Theme};

/// 滚动条轨占的横向槽宽（dp）= 轨宽 + 两侧留白。
///
/// 与 iced 的 `total_scrollbar_width`（`width.max(scroller_width) + 2 * margin`）同一个式子：
/// 参数里的 `rail_width` 传的是 `Scrollbar::width` 与 `scroller_width` 的同一个值（本仓两者相等）。
pub fn rail_slot(rail_width: f32, margin: f32) -> f32 {
    rail_width + 2.0 * margin
}

/// 内容可用宽度（dp）= 视口宽 − 槽宽。
///
/// 这条式子是本模块的**判据**：`components/scroll.rs` 的测试用 iced 的布局引擎真量一遍，
/// `pages/settings/tests/scroll_fit.rs` 在产品那条 `frame::scroll` 上再量一遍。
/// 负数按 0 处理——视口比槽还窄时（产品窗口不会发生）宁可内容零宽，也不给 iced 送负宽。
#[cfg(test)]
pub fn content_width(viewport: f32, slot: f32) -> f32 {
    (viewport - slot).max(0.0)
}

/// 竖直滚动区：内容右侧恒留 [`rail_slot`] 的槽，滚动条画在槽里。
///
/// `style` 就是 `scrollable::style` 回调（配色由调用方按令牌给），本模块只管几何。
pub fn vertical<'a, Message, Renderer>(
    rail_width: f32,
    margin: f32,
    style: impl Fn(&Theme, scrollable::Status) -> scrollable::Style + 'a,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: iced::advanced::text::Renderer + 'a,
{
    scrollable(beside(rail_slot(rail_width, margin), content))
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(rail_width)
                .scroller_width(rail_width)
                .margin(margin),
        ))
        .height(Length::Fill)
        .style(style)
        .into()
}

/// 内容 + 右侧槽位。容器宽度仍是 `Fill`（= 视口宽），扣掉的是**内容的**可用宽度。
fn beside<'a, Message, Renderer>(
    slot: f32,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    container(content)
        .width(Length::Fill)
        .padding(Padding {
            right: slot,
            ..Padding::ZERO
        })
        .into()
}

#[cfg(test)]
mod tests {
    //! ## 为什么这里的守卫是**真的布局**而不是源码扫描
    //!
    //! iced 的控件树支持在测试里跑一遍布局：`iced_core` 在 debug 构建下给 `()` 实现了全套
    //! `Renderer` trait（`iced_core-0.14.0/src/renderer/null.rs`），所以 `Element<_, _, Theme, ()>`
    //! 能被 `Widget::layout` 真正量出尺寸——**不需要开窗、不需要 GPU**。
    //!
    //! 于是「内容可用宽度 = 视口宽 − 槽宽」是一条数值断言，而不是「看图觉得对」；把
    //! [`beside`] 的 `padding` 去掉，下面的 `content_width` 立刻变成 434 而断言红。
    //!
    //! ⚠️ 诚实边界：`()` 的 Renderer 实现带 `#[cfg(debug_assertions)]`，所以这些测试在
    //! `cargo test --release` 下编不过（本仓 CI/本地一律 dev profile 跑测试）。

    use iced::advanced::layout::Limits;
    use iced::advanced::widget::Tree;
    use iced::widget::{container, scrollable, space};
    use iced::{Rectangle, Size};

    use super::*;

    /// 本仓的轨道参数（真源是 `pages/frame/pieces.rs` 的两个常量，这里按「用户取值」复核）。
    const RAIL: f32 = 6.0;
    const MARGIN: f32 = 2.0;
    /// 设置页表单列在 1180 宽窗口下的列宽（`measure_cap_px` = 424）。
    const VIEWPORT: f32 = 424.0;
    /// 视口高（1180×760 减去页头/页边距后的量级；只要求「长页 > 视口」「短页 < 视口」）。
    const VIEWPORT_HEIGHT: f32 = 600.0;
    /// 长页面（一定出滑块）与短页面（一定不出滑块）的内容高。
    const LONG: f32 = 4000.0;
    const SHORT: f32 = 120.0;

    /// 真跑一遍布局，返回 (槽容器, 内容) 两个矩形。
    ///
    /// 节点树：`scrollable → [槽容器（Fill 宽，右内边距 = slot） → [内容]]`，见
    /// `iced_core/src/layout.rs:201`（`padded` 的节点恒为「自己 + 一个子节点」）。
    fn laid_out(content_height: f32) -> (Rectangle, Rectangle) {
        let filler: Element<'_, (), Theme, ()> = container(space())
            .width(Length::Fill)
            .height(Length::Fixed(content_height))
            .into();

        let mut element: Element<'_, (), Theme, ()> =
            vertical::<(), ()>(RAIL, MARGIN, scrollable::default, filler);
        let mut tree = Tree::new(element.as_widget());

        let node = element.as_widget_mut().layout(
            &mut tree,
            &(),
            &Limits::new(Size::ZERO, Size::new(VIEWPORT, VIEWPORT_HEIGHT)),
        );

        let gutter = node.children()[0].bounds();
        let content = node.children()[0].children()[0].bounds();

        (gutter, content)
    }

    /// **本 mission 的核心判据**：内容可用宽度 = 视口宽 − 滚动条槽，且与有没有滑块无关。
    #[test]
    fn the_content_width_leaves_the_rail_its_own_slot() {
        let slot = rail_slot(RAIL, MARGIN);
        assert_eq!(slot, RAIL + 2.0 * MARGIN, "槽宽口径 = 轨宽 + 两侧留白");

        let (_, long) = laid_out(LONG);
        let (_, short) = laid_out(SHORT);

        assert_eq!(
            long.width,
            content_width(VIEWPORT, slot),
            "长页面（出滑块）的内容宽度必须是「视口宽 − 槽宽」"
        );
        assert_eq!(
            short.width,
            content_width(VIEWPORT, slot),
            "短页面（不出滑块）的内容宽度必须**一样**：否则滑块一出现内容就横跳"
        );
        assert_eq!(VIEWPORT - long.width, slot, "右侧扣掉的正好是槽宽");

        // 滑块落在槽的中间：内容右缘 → margin → 滑块 → margin → 视口右缘。
        // 轨道按 iced 的算法定位（`scrollable.rs:1970-2005`：槽宽 = 轨宽 + 2×margin，滑块再内缩 margin），
        // 所以「内容右缘 + margin」就是滑块的左缘。
        let content_right = long.x + long.width;
        let pill_left = content_right + MARGIN;
        let pill_right = pill_left + RAIL;
        assert_eq!(
            pill_right + MARGIN,
            VIEWPORT,
            "滑块必须整个落在槽内（两侧各留 margin），否则「并排」变成「压着」"
        );
    }

    /// 槽容器自己仍是满宽（`Fill`），否则页面为它预留的 424 会被再吃掉一截。
    #[test]
    fn the_slot_wrapper_keeps_the_full_viewport_width() {
        let (gutter, _) = laid_out(LONG);
        assert_eq!(gutter.width, VIEWPORT);
    }

    /// 左边界与宽度在长/短两态下**逐项相同**（M173 明写的「两张对照图 + 数值」里的数值部分）。
    #[test]
    fn long_and_short_pages_share_the_same_left_edge_and_width() {
        let (_, long) = laid_out(LONG);
        let (_, short) = laid_out(SHORT);

        assert_eq!(long.x, short.x, "左边界必须一致");
        assert_eq!(long.width, short.width, "宽度必须一致");
        assert_eq!(short.x, 0.0, "槽只占右侧，左侧内边距为 0");
    }

    #[test]
    fn content_width_never_goes_negative() {
        assert_eq!(content_width(RAIL, rail_slot(RAIL, MARGIN)), 0.0);
        assert_eq!(content_width(0.0, rail_slot(RAIL, MARGIN)), 0.0);
        assert_eq!(content_width(VIEWPORT, 0.0), VIEWPORT);
    }
}
