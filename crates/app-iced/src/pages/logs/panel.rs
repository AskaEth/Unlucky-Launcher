//! 两块的共用外壳：**定高的卡片 + 内部独立滚动**。
//!
//! ## 为什么必须定高（M95 r1 的实拍 bug）
//!
//! 第一版把诊断卡按内容自然高度画、日志卡吃剩余高度。三条结论在 424×540 的正文里就要
//! ~500px，结果：诊断卡被裁掉尾部（一行摘要整块消失）、日志卡拿到 **0** 高度（整块不见）。
//! 截图见报告里的「r1 崩了」。
//!
//! 所以两块都**由调用方给定高**（`FillPortion`），各自内部滚动：
//!
//! | 块 | 高度 | 为什么 |
//! |---|---|---|
//! | 诊断 | `FillPortion(4)` | 崩溃时先读结论；份额要够 177px 的滚动视口，才放得下**一条完整结论**（含建议动作与置信度） |
//! | 日志 | `FillPortion(2)` | 日志是长流水，滚就是了 |
//!
//! 两者都是**兄弟**滚动区（不是嵌套）：滚轮归属清楚，不存在「滚动条套滚动条」。

use iced::widget::{container, scrollable};
use iced::{Background, Element, Length};

use unlucky_design::component_map::Component;
use unlucky_design::elevation::MaterialLayer;

use crate::theme::Ui;

use super::super::frame;
use super::super::Message;

/// 定高卡片（`Card` 材质层 + `OutlinedCard` 形状与 **1px 描边**，与 `frame::card` 同一套令牌：
/// 实例页 / 下载页的卡都带这条描边，日志页的两张主卡不能例外）。
pub fn card<'a>(
    ui: &'a Ui,
    height: Length,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let shape = Component::OutlinedCard.spec().shape;

    container(content)
        .width(Length::Fill)
        .height(height)
        .padding(ui.metrics().card_padding)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.layer(MaterialLayer::Card))),
            border: ui.outlined(shape),
            ..container::Style::default()
        })
        .into()
}

/// 卡片内的竖向滚动区（贴顶：诊断卡从头读）。
pub fn scroll<'a>(ui: &'a Ui, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    scroller(ui, content, false)
}

/// 卡片内的竖向滚动区（**贴底**：日志卡看最后几行）。
///
/// `anchor_bottom` = `Anchor::End`：滚动偏移按「距底多远」记，所以**内容增长时视图保持在
/// 距底同一位置 ⇒ 天然跟随尾部**。用户上滚后偏移 = 距底 N 像素，新内容到达时仍按「距底 N」
/// 呈现（视图跟着内容下移），也就是说**这里没有「上滚即冻结」**——那需要页面自己持有滚动
/// 状态 + `on_scroll` 消息 + app 层回灌，属于另一件事（已登记在报告里）。
pub fn scroll_tail<'a>(
    ui: &'a Ui,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    scroller(ui, content, true)
}

fn scroller<'a>(
    ui: &'a Ui,
    content: impl Into<Element<'a, Message>>,
    tail: bool,
) -> Element<'a, Message> {
    let view = scrollable(content)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        .height(Length::Fill)
        .width(Length::Fill)
        .style(frame::scroll_style(ui));

    if tail {
        view.anchor_bottom().into()
    } else {
        view.into()
    }
}
