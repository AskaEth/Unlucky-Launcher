//! 折叠档：48px 图标条（M119 从 `mod.rs` 拆出来守 400 行硬线）。
//!
//! 条上三行：AI 图标（带未读点）+ 小字标签 + 未读计数。为什么未读既有点又有数字：48px 宽
//! 装不下 `2 unread` 那 8 个字符（实测贴到窗角被裁），所以条上只放「有几条」，完整那句
//! （`{count} unread`）在面板头部的胶囊里。零未读时给一个等高留白，免得条上元素上下跳。
//!
//! 未读数来自 presenter 的 `AgentRail::unread`（M119 起壳里不再自己数）。

use iced::widget::{column, container, space, stack, text, tooltip};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};

use unlucky_presenter::agent::AgentRail;

use crate::app::components::buttons::{self, IconVariant};
use crate::theme::Ui;

use super::metrics::COLLAPSED_WIDTH;
use super::{conversation, Message};

/// 折叠条。
pub(super) fn strip<'a>(ui: &'a Ui, rail: &'a AgentRail, catalog: Catalog) -> Element<'a, Message> {
    let side = ui.metrics().control_height;

    // 图标按钮 40×40（M3 icon button），未读点叠在右上角；下面两行小字保证「不得只有图标」：
    // 条上每个可点元素都有可读文字（完整名字在 tooltip 里）。
    let icon: Element<'a, Message> = buttons::icon(
        ui,
        IconVariant::Standard,
        IconKind::Ai,
        Some(Message::Expand),
        None,
    );

    let glyph: Element<'a, Message> = if rail.unread > 0 {
        stack![
            icon,
            container(dot(ui))
                .width(Length::Fixed(side))
                .height(Length::Fixed(side))
                .align_x(Alignment::End)
                .align_y(Alignment::Start)
                .padding(Padding::from([0.0, ui.space(Space::X1)])),
        ]
        .into()
    } else {
        icon
    };

    let labelled = container(column![
        glyph,
        conversation::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            catalog.text(Key::AgentStripLabel),
            ui.color(Role::OnSurfaceVariant),
            // `Shrink` 而不是 `Fill`：48 宽的条上 Fill 会让文字贴左缘，"2 unread" 会被窗角裁掉
            Length::Shrink,
        ),
        unread_text(ui, rail.unread),
    ])
    .width(Length::Fixed(COLLAPSED_WIDTH))
    .align_x(Alignment::Center)
    .padding(Padding::from([ui.space(Space::X6), 0.0]));

    let tip_style = ui.style(TypeRole::BodySmall);
    let tip = container(
        text(catalog.text(Key::AgentOpen))
            .size(tip_style.size_px)
            .font(ui.font_of(tip_style))
            .color(ui.color(Role::OnSurface)),
    )
    .padding(Padding::from([ui.space(Space::X1), ui.space(Space::X2)]))
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.layer(MaterialLayer::Overlay))),
        border: Border {
            radius: ui.radius_dp(ShapeScale::Small),
            ..Border::default()
        },
        ..container::Style::default()
    });

    container(tooltip(labelled, tip, tooltip::Position::Left))
        .width(Length::Fixed(COLLAPSED_WIDTH))
        .height(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.layer(MaterialLayer::Canvas))),
            ..container::Style::default()
        })
        .into()
}

/// 未读点（8×8，`Primary`）。
fn dot<'a>(ui: &'a Ui) -> Element<'a, Message> {
    let side = ui.space(Space::X2);

    container(space::horizontal())
        .width(Length::Fixed(side))
        .height(Length::Fixed(side))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.color(Role::Primary))),
            border: Border {
                radius: ui.radius(ShapeScale::Full, side),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// 未读计数（零 = 等高留白，免得条上元素上下跳）。
fn unread_text<'a>(ui: &'a Ui, unread: usize) -> Element<'a, Message> {
    let style = ui.style(TypeRole::LabelMedium);

    if unread == 0 {
        return space::vertical().height(style.line_height_px).into();
    }

    text(unread.to_string())
        .size(style.size_px)
        .font(ui.font_of(style))
        .color(ui.color(Role::Primary))
        .width(Length::Shrink)
        .into()
}
