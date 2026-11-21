//! 向导里的小工具：等宽行、可换行正文、标签、问题行、档位人话。
//!
//! 单独一个文件只为守 400 行硬线（AGENTS §3.1）：三段视图（`pick` / `panes`）与卡壳
//! （`adopt.rs`）都要用它们，放哪儿都会让那一处越线。

use iced::widget::text::LineHeight;
use iced::widget::{column, text};
use iced::{Color, Element, Font, Length, Pixels};

use unlucky_design::color::Role;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{AdoptIsolationView, AdoptProblemView, AdoptTargetView};

use crate::pages::frame;
use crate::theme::Ui;

use super::super::message::Message;

/// 段内的次级标签（`LabelMedium` + `OnSurfaceVariant`）。
pub(super) fn label<'a>(ui: &'a Ui, value: &'a str) -> Element<'a, Message> {
    frame::text_role(
        ui,
        TypeRole::LabelMedium,
        false,
        value,
        ui.color(Role::OnSurfaceVariant),
    )
    .map(Message::Page)
}

/// 等宽一行（路径 / 机读码要能逐字读）。
pub(super) fn mono<'a>(ui: &'a Ui, value: String) -> Element<'a, Message> {
    let style = ui.style(TypeRole::BodySmall);
    text(value)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
        .font(Font {
            family: iced::font::Family::Name(ui.families().mono),
            ..Font::DEFAULT
        })
        .color(ui.color(Role::OnSurface))
        .width(Length::Fill)
        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
        .into()
}

/// 一段可换行的正文（长路径只按词折行会把卡顶破，所以按词或字形折）。
pub(super) fn wrapped<'a>(ui: &'a Ui, value: String, color: Color) -> Element<'a, Message> {
    frame::text_role_owned(ui, TypeRole::BodySmall, false, value, color).map(Message::Page)
}

/// 「标签 + 值」一行（值用等宽）。
pub(super) fn field_line<'a>(ui: &'a Ui, label_text: &'a str, value: &str) -> Element<'a, Message> {
    column![label(ui, label_text), mono(ui, value.to_string())]
        .spacing(0.0)
        .width(Length::Fill)
        .into()
}

/// 一条问题：稳定码（等宽） + 人读原因（`Error` 色）。**一条都不许折叠**。
pub(super) fn problem_line<'a>(ui: &'a Ui, problem: &'a AdoptProblemView) -> Element<'a, Message> {
    column![
        mono(ui, problem.code.clone()),
        wrapped(ui, problem.message.clone(), ui.color(Role::Error)),
    ]
    .spacing(0.0)
    .width(Length::Fill)
    .into()
}

/// 隔离档的人话（沿用实例页已有的两条；`custom` 就是缺省档，直接说「在 ULL 数据区新建」）。
pub(super) fn isolation_label(isolation: AdoptIsolationView, catalog: Catalog) -> String {
    match isolation {
        AdoptIsolationView::Isolated => catalog.text(Key::InstancesIsolationIsolated).to_string(),
        AdoptIsolationView::Shared => catalog.text(Key::InstancesIsolationShared).to_string(),
        AdoptIsolationView::Custom => catalog.text(Key::AdoptTargetInstanceDir).to_string(),
    }
}

/// 目标档的短标签。
pub(super) fn target_label(target: AdoptTargetView, catalog: Catalog) -> &'static str {
    match target {
        AdoptTargetView::InstanceDir => catalog.text(Key::AdoptTargetInstanceDir),
        AdoptTargetView::Shared => catalog.text(Key::AdoptTargetShared),
        AdoptTargetView::VersionDir => catalog.text(Key::AdoptTargetVersionDir),
    }
}
