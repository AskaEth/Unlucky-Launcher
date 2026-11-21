//! 弹窗的**页脚操作区**（M449 从 `view.rs` 搬出来，守 400 行硬线；搬移零行为改动）。
//!
//! ## 两行
//!
//! 辅助操作（开文件夹 / 复制路径 / 展开）在上，主次按钮在下靠右。为什么分两行：五个按钮塞一行
//! 在 560 的卡宽里会把主按钮压成竖排（实测第一版就是这样，「导出诊断包」被挤成两行单字）。
//!
//! ## 每个按钮的底都是 Overlay 面板（M449 修的 M234 那条洞）
//!
//! 卡片画的是 `Ui::layer(MaterialLayer::Overlay)`（= `SurfaceContainerHigh`，壁纸模式下带面板 α），
//! 而 `buttons::Button::new` 的默认底是 `Base::CANVAS`（`Role::Surface`）。M449 之前这一整块
//! ——含崩溃卡片里的三枚辅助按钮——全都落在那个默认值上 ⇒ 悬停 8% / 按下 10% / 禁用 10% 与
//! 禁用内容 38% 集体合成到**错的**底层。
//!
//! 今天所有按钮都经 [`panel_button`] 造：**唯一的构造入口**，底恒为 [`Base::OVERLAY`]。于是
//! 「漏传 `.base(..)`」在这条产品路径上不再是一个可能（`actions/tests.rs` 把两件事都钉着：
//! 造出来的 `Button` 真的带 Overlay 底，且本文件里没有第二个绕过它构造按钮的地方）。
//!
//! ⚠️ `Base` 只装一个 `Role`：面板 α < 1 时容器画的是**合成色**，状态层仍按**不透明**角色色合成
//! （误差 = 面板 α 那一段）。这是 `interaction::base` 模块文档「抓不住 ①」已登记的边界。
//!
//! 卡片里还有一条**不是** `buttons::Button` 的控件：路径行（`view.rs` 里直接拿 `iced::widget::button`
//! 包的两行文字，`background: None` / `text_color: TRANSPARENT`）。它没有任何状态层与禁用态合成，
//! 所以不吃 `Base`——不是漏了，是那条路根本没有要合成的颜色。

use iced::widget::{column, row, Space};
use iced::{Alignment, Element, Length};

use unlucky_design::spacing::Space as Sp;

use crate::app::components::buttons;
use crate::app::components::interaction::Base;
use crate::app::dialog::{Message as DialogMessage, Modal};
use crate::app::message::Message;
use crate::theme::Ui;

/// 弹窗里的按钮（**本文件唯一的按钮构造入口**）：底永远是 Overlay 面板。
fn panel_button<'a>(label: &'a str, message: Message) -> buttons::Button<'a, Message> {
    buttons::Button::new(label, message).base(Base::OVERLAY)
}

/// 底部按钮区：**两行**——辅助操作（开文件夹 / 复制路径 / 展开）在上，主次按钮在下靠右。
///
/// 为什么分两行：五个按钮塞一行在 560 的卡宽里会把主按钮压成竖排（实测第一版就是这样，
/// 「导出诊断包」被挤成两行单字）。
pub(super) fn footer<'a>(ui: &'a Ui, modal: &'a Modal) -> Element<'a, Message> {
    let mut column_actions = column![].spacing(ui.space(Sp::X2)).width(Length::Fill);

    if let Some(card) = &modal.card {
        column_actions = column_actions.push(
            row![
                buttons::outlined(
                    ui,
                    panel_button(
                        card.actions[1].as_str(),
                        Message::Dialog(DialogMessage::OpenFolder),
                    ),
                ),
                buttons::outlined(
                    ui,
                    panel_button(
                        card.actions[2].as_str(),
                        Message::Dialog(DialogMessage::CopyPaths),
                    ),
                ),
                buttons::text(
                    ui,
                    panel_button(card.toggle.as_str(), Message::Dialog(DialogMessage::Toggle)),
                ),
            ]
            .spacing(ui.space(Sp::X2))
            .align_y(Alignment::Center),
        );
    }

    let mut line = row![Space::new().width(Length::Fill)].spacing(ui.space(Sp::X2));
    // 三档强调位：`outlined`（最左，重但不抢主位）→ `text` → `filled`（最右，主按钮）。
    //
    // **动作跟着按钮走，不跟着位置猜**（M349 r2 / 复审 P2-3）：建议框把「下载 Java 8」提到
    // `filled` 那一格，而那条出口的动作是 `Tertiary`；`Modal::primary_action` /
    // `tertiary_action` 就是为此存在的（默认仍是 Primary / Tertiary，硬拦那一支一字不变）。
    let action = |slot: Option<&DialogMessage>, default: DialogMessage| {
        Message::Dialog(slot.cloned().unwrap_or(default))
    };
    if let Some(tertiary) = &modal.tertiary {
        line = line.push(buttons::outlined(
            ui,
            panel_button(
                tertiary.as_str(),
                action(modal.tertiary_action.as_ref(), DialogMessage::Tertiary),
            ),
        ));
    }
    if let Some(secondary) = &modal.secondary {
        line = line.push(buttons::text(
            ui,
            panel_button(
                secondary.as_str(),
                Message::Dialog(DialogMessage::Secondary),
            ),
        ));
    }
    if !modal.blocking_progress {
        line = line.push(buttons::filled(
            ui,
            panel_button(
                modal.primary.as_str(),
                action(modal.primary_action.as_ref(), DialogMessage::Primary),
            ),
        ));
    }
    column_actions.push(line.align_y(Alignment::Center)).into()
}

#[cfg(test)]
mod tests;
