//! 对话本体：空态 + 消息气泡（M119 从 `conversation.rs` 拆出来守 400 行硬线）。
//!
//! ## 数据从哪来
//!
//! 直接画 presenter 的 `AgentRail::messages`（`ChatMessage`）：`role` 决定角色小标与气泡容器，
//! `text` 是正文（**已经过 presenter 的第二道脱敏闸门** `scrub_text`），`tool` 是工具调用条目
//! （D4：工具调用会话可见）。壳里不再有 `Speaker`/`Turn` 那种自己的会话类型——那是一份要跟
//! presenter 对齐的镜像，M119 已经删掉。

use iced::widget::{column, container, scrollable};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};

use unlucky_presenter::dto::{ChatMessage, ChatRole};

use crate::app::components::feedback::empty_state::{empty_state, EmptyState};
use crate::pages::frame;
use crate::theme::Ui;

use super::super::Message;
use super::{text_role, text_role_owned, Conversation};

/// 空会话：引导空态（零条消息时替代气泡列表）。
///
/// `body` 由调用方从 [`super::super::Disclaimer`] 取（模拟态/真实态两句话），这里不再自己取词。
pub(super) fn empty<'a>(ui: &'a Ui, catalog: Catalog, body: &'a str) -> Element<'a, Message> {
    container(empty_state(
        ui,
        EmptyState::new(IconKind::Ai, catalog.text(Key::AgentEmptyTitle), body),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// 气泡列表（可滚动）。
pub(super) fn list<'a>(
    ui: &'a Ui,
    spec: Conversation<'a>,
    catalog: Catalog,
) -> Element<'a, Message> {
    let mut items = column![].spacing(ui.space(Space::X3)).width(Length::Fill);

    for message in spec.messages {
        items = items.push(bubble(ui, message, catalog));
    }

    scrollable(container(items).padding(Padding::from([ui.space(Space::X1), 0.0])))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(frame::scroll_style(ui))
        .into()
}

/// 一条消息：角色小标 + 正文。三种容器（用户 = `SecondaryContainer`、助手/工具 = 卡片层、
/// 系统提示 = 无容器只留文字），气泡整体右/左对齐。
fn bubble<'a>(ui: &'a Ui, message: &'a ChatMessage, catalog: Catalog) -> Element<'a, Message> {
    let (role, fill, ink, align) = match message.role {
        ChatRole::User => (
            Key::AgentRoleYou,
            Some(ui.color(Role::SecondaryContainer)),
            ui.color(Role::OnSecondaryContainer),
            Alignment::End,
        ),
        // 工具条目也挂在助手名下：它是助手这一轮做的事，不是第三方说话人
        ChatRole::Assistant | ChatRole::Tool => (
            Key::AgentRoleAssistant,
            Some(ui.layer(MaterialLayer::Card)),
            ui.color(Role::OnSurface),
            Alignment::Start,
        ),
        ChatRole::System => (
            Key::AgentRoleNotice,
            None,
            ui.color(Role::OnSurfaceVariant),
            Alignment::Start,
        ),
    };

    // 工具条目：正文是结果摘要，名字前缀一行（`tool.name`，与 ToolSpec 注册表同名）。
    // 两者都来自 presenter 的事件，已过脱敏；这里只拼展示串。
    let body = match &message.tool {
        Some(tool) => format!("{} · {}", tool.name, tool.summary),
        None => message.text.clone(),
    };

    let inner = column![
        text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            catalog.text(role),
            ui.color(Role::OnSurfaceVariant),
            Length::Fill,
        ),
        text_role_owned(ui, TypeRole::BodyMedium, false, body, ink, Length::Fill),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    let card = container(inner)
        .width(Length::Fill)
        .padding(Padding::from([ui.space(Space::X2), ui.space(Space::X3)]))
        .style(move |_theme| container::Style {
            background: fill.map(Background::Color),
            border: if fill.is_some() {
                ui.outlined(ShapeScale::Medium)
            } else {
                Border::default()
            },
            ..container::Style::default()
        });

    container(card).width(Length::Fill).align_x(align).into()
}
