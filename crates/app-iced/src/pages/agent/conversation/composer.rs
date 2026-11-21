//! 输入区（多行输入框 + 发送/中断）：会话视图的**下半截**。
//!
//! 从 `conversation.rs` 拆出来的理由只有一条：那边贴着 400 行硬上限（AGENTS.md §3-1）。
//! 拆的位置是「输入区」这一整块 + 一个排版小工具，语义边界与文件边界一致。
//!
//! ## Enter / Shift+Enter 的分流（任务第 4 条）
//!
//! `text_editor` 的默认绑定把两种 Enter 都折成 `Binding::Enter`，所以必须自己接管
//! （见 [`composer`] 里的 `key_binding`）：`modifiers.shift()` 决定「换行」还是「发送」。
//! **只在输入框聚焦时生效**——`KeyPress.status` 不是 `Focused` 时原样交回默认绑定，
//! 于是别处的 Enter 不会被助手栏抢走。

use iced::widget::text_editor::{self, Binding, KeyPress};
use iced::widget::{column, row, text, text_editor as editor};
use iced::{keyboard, Background, Border, Element, Length, Pixels, Theme};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::theme::{Ui, STROKE_PX};

use super::super::state::Focus;
use super::super::Message;
use super::{focused_state, Conversation};

pub(super) fn composer<'a>(
    ui: &'a Ui,
    spec: Conversation<'a>,
    catalog: Catalog,
) -> Element<'a, Message> {
    let style = ui.style(TypeRole::BodyMedium);

    let field = editor(spec.draft)
        .placeholder(spec.note.placeholder)
        .on_action(Message::Edit)
        .key_binding(binding_for)
        .size(style.size_px)
        .font(ui.font_of(style))
        .height(Length::Fixed(
            ui.space(Space::X8) + ui.space(Space::X8) + ui.space(Space::X1),
        ))
        .padding(ui.space(Space::X2))
        .style(move |_theme: &Theme, status| {
            let focused = matches!(status, text_editor::Status::Focused { .. });
            text_editor::Style {
                background: Background::Color(ui.layer(MaterialLayer::Card)),
                border: if focused {
                    Border {
                        color: ui.color(Role::Primary),
                        width: STROKE_PX * 2.0,
                        radius: ui.radius_dp(ShapeScale::Small),
                    }
                } else {
                    ui.outlined(ShapeScale::Small)
                },
                placeholder: ui.color(Role::OnSurfaceVariant),
                value: ui.color(Role::OnSurface),
                selection: ui.color(Role::PrimaryContainer),
            }
        });

    let send_label = catalog.text(Key::AgentSend);
    let send = if spec.blank || spec.busy {
        Button::disabled(send_label)
    } else {
        Button::new(send_label, Message::Submit)
    };
    // 键盘焦点在「发送」上时用 `Interaction::Focused`（10% 状态层），与头部动作同一套焦点语言。
    let send = focused_state(
        send.leading(IconKind::Send),
        spec.focus == Some(Focus::Send),
    );

    let mut actions = row![]
        .spacing(ui.space(Space::X2))
        .align_y(iced::Alignment::Center);
    if spec.busy {
        actions = actions.push(buttons::outlined(
            ui,
            focused_state(
                Button::new(catalog.text(Key::AgentStop), Message::Stop).leading(IconKind::Close),
                spec.focus == Some(Focus::Stop),
            ),
        ));
    }
    actions = actions.push(buttons::filled(ui, send));

    column![field, actions]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill)
        .into()
}

/// Enter = 发送 / Shift+Enter = 换行（任务第 4 条）。
///
/// 为什么走 `key_binding` 而不是全局键盘订阅：`KeyPress` 直接带 `modifiers`，而 iced 的默认绑定
/// 把两种 Enter 都折成 `Binding::Enter`（`text_editor.rs:1210`），所以**必须**在这里分流。
/// 未聚焦时一律交回默认绑定（`from_key_press` 对非聚焦返回 `None`），于是别处的 Enter 不会被
/// 助手栏抢走。
///
/// 抽成**纯函数**是为了能被单测钉住（复审 P3-2：之前一个测试都没覆盖这条分流）。
pub(super) fn binding_for(press: KeyPress) -> Option<Binding<Message>> {
    let enter = matches!(
        press.modified_key.as_ref(),
        keyboard::Key::Named(keyboard::key::Named::Enter)
    );
    if !enter || !matches!(press.status, text_editor::Status::Focused { .. }) {
        return Binding::from_key_press(press);
    }
    if press.modifiers.shift() {
        Some(Binding::Enter)
    } else {
        Some(Binding::Custom(Message::Submit))
    }
}

/// 一档文字（**借来的**正文）。
///
/// 与 `pages::frame::text_role` 同源，但那边返回的是 `Element<pages::Message>`，助手栏要自己的
/// 消息类型——这是 M104 起就有的分工，不是重复实现。
/// 一档文字（**拼出来的**正文：工具条目 `name · summary`、横幅文案都走它）。
///
/// 为什么不能直接用借用版：正文是 `String`，而 `Element` 不允许借用局部变量。
pub(crate) fn text_role_owned<'a>(
    ui: &'a Ui,
    role: TypeRole,
    emphasized: bool,
    content: String,
    color: iced::Color,
    width: Length,
) -> Element<'a, Message> {
    let style = if emphasized {
        ui.emphasized(role)
    } else {
        ui.style(role)
    };

    text(content)
        .size(style.size_px)
        .line_height(iced::widget::text::LineHeight::Absolute(Pixels(
            style.line_height_px,
        )))
        .font(ui.font_of(style))
        .color(color)
        .width(width)
        .into()
}

pub(crate) fn text_role<'a>(
    ui: &'a Ui,
    role: TypeRole,
    emphasized: bool,
    content: &'a str,
    color: iced::Color,
    width: Length,
) -> Element<'a, Message> {
    let style = if emphasized {
        ui.emphasized(role)
    } else {
        ui.style(role)
    };

    text(content)
        .size(style.size_px)
        .line_height(iced::widget::text::LineHeight::Absolute(Pixels(
            style.line_height_px,
        )))
        .font(ui.font_of(style))
        .color(color)
        .width(width)
        .into()
}

#[cfg(test)]
mod tests {
    use iced::keyboard::key::{Code, Named, Physical};
    use iced::keyboard::{Key, Modifiers};

    use super::super::super::Message;
    use super::*;

    fn press(key: Key, modifiers: Modifiers, focused: bool) -> KeyPress {
        let status = if focused {
            text_editor::Status::Focused { is_hovered: false }
        } else {
            text_editor::Status::Active
        };
        KeyPress {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Code(Code::Enter),
            modifiers,
            text: None,
            status,
        }
    }

    fn enter() -> Key {
        Key::Named(Named::Enter)
    }

    /// **Enter 发送 / Shift+Enter 换行**（复审 P3-2 点名缺的那条）：
    /// 抓得住两条分支各自选对了 `Binding`（非 shift 只发消息、不插换行；shift 才插换行）。
    /// 抓不住真实按键（`KeyPress` 是这里手搓的，不是 winit 送上来的）。
    #[test]
    fn enter_sends_and_shift_enter_breaks_the_line() {
        // 非 shift + 聚焦 ⇒ 发消息（**不**插换行）
        let plain = binding_for(press(enter(), Modifiers::default(), true));
        assert!(
            matches!(plain, Some(Binding::Custom(Message::Submit))),
            "Enter 应当发 Submit，实际 {plain:?}"
        );

        // shift + 聚焦 ⇒ 插换行
        let shifted = binding_for(press(enter(), Modifiers::SHIFT, true));
        assert!(
            matches!(shifted, Some(Binding::Enter)),
            "Shift+Enter 应当插换行，实际 {shifted:?}"
        );

        // 未聚焦 ⇒ 原样交回默认绑定（它自己对非聚焦返回 None），别处的 Enter 不受影响
        assert!(binding_for(press(enter(), Modifiers::default(), false)).is_none());

        // 其它键 ⇒ 交回默认绑定（这里用 Tab：默认绑定里没有它）
        let tab = press(Key::Named(Named::Tab), Modifiers::default(), true);
        assert!(binding_for(tab).is_none());
    }
}
