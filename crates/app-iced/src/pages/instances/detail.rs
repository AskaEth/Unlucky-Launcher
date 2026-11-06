//! 实例页的**详情列**（M90 r2 从 `instances.rs` 拆出来：那边超过 400 行会被 guards 的 file-size 判红）。
//!
//! 结构：标题行（名字 + 版本 + Play/Stop）→ 元信息网格（列数由 `layout::meta_columns` 决定）
//! → 一条分隔线 → 正文段 / 运行状态行 → 动作行（Duplicate / Open folder / Delete）。
//! 窄窗（<840）时列表退成上一屏，本文件只画详情 + 一个「返回」按钮。

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_presenter::router::Route;

use crate::app::components::buttons::{self, Button};
use crate::app::components::divider;
use crate::app::components::interaction::Interaction;
use crate::theme::Ui;
use unlucky_design::icons::IconKind;

use super::super::frame;
use super::super::layout::{self, FrameMetrics};
use super::super::model::Detail;
use super::super::{Action, Message};
use super::forced_state;
use super::Texts;

// --------------------------------------------------------------- 详情列

pub(super) fn detail_column<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    detail: &'a Detail,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    let mut body = column![
        detail_head(ui, detail, forced, texts),
        meta_grid(ui, metrics, detail),
        divider::full(ui),
    ]
    .spacing(ui.space(Space::X4))
    .width(Length::Fill);

    body = push_text(ui, metrics, body, detail);

    // 页头 + 元信息网格 + **一条**分隔线以下才是正文与操作区（M84 那两条线是占位时期的排版残留）
    frame::left_aligned(metrics.column, body.push(action_row(ui, forced, texts)))
}

pub(super) fn narrow_detail<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    detail: &'a Detail,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    // 窄窗：列表退成上一屏，本屏只放详情（官方 compact 的 list-detail 口径）。
    let mut body = column![
        row![buttons::text(
            ui,
            forced_state(
                Button::new(texts.back(), Message::Navigate(Route::Instances)),
                forced,
            ),
        )]
        .width(Length::Fill)
        .align_y(Alignment::Center),
        detail_head(ui, detail, forced, texts),
        meta_grid(ui, metrics, detail),
        divider::full(ui),
    ]
    .spacing(ui.space(Space::X4))
    .width(Length::Fill);

    body = push_text(ui, metrics, body, detail);

    body = body.push(frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        texts.narrow_note(),
        ui.color(Role::OnSurfaceVariant),
    ));

    frame::body(metrics, body.push(action_row(ui, forced, texts)))
}

/// 正文段（演示数据才有）与运行状态行（真数据才有）——两者都是可选的，顺序固定。
///
/// 散文段套 [`frame::prose`]：M102 起页面块吃满页宽，但「一行 ≤72 字符」的承诺仍然只由
/// 散文承担（`frame::body_width` 管块、`metrics.column` 管一句话）。
fn push_text<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    mut body: iced::widget::Column<'a, Message>,
    detail: &'a Detail,
) -> iced::widget::Column<'a, Message> {
    if let Some(note) = &detail.note {
        body = body.push(frame::prose(
            metrics,
            frame::text_role(
                ui,
                TypeRole::BodyMedium,
                false,
                note.as_str(),
                ui.color(Role::OnSurfaceVariant),
            ),
        ));
    }
    if let Some(status) = &detail.status {
        body = body.push(frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            status.as_str(),
            ui.color(Role::OnSurface),
        ));
    }
    body
}

fn detail_head<'a>(
    ui: &'a Ui,
    detail: &'a Detail,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    let titles = column![
        frame::text_role(
            ui,
            TypeRole::TitleLarge,
            true,
            detail.name.as_str(),
            ui.color(Role::OnSurface)
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            detail.subtitle.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    // 主操作：**全屏唯一**实心 accent（rubric K2）。不可用时退化成禁用态（只清消息，不改身份）。
    let launch = if detail.can_launch {
        buttons::filled(
            ui,
            forced_state(
                Button::new(texts.play(), Message::Launch(detail.id.clone())),
                forced,
            ),
        )
    } else {
        buttons::filled(ui, Button::disabled(texts.play()))
    };
    let stop = if detail.can_stop {
        buttons::text(
            ui,
            forced_state(
                Button::new(texts.stop(), Message::Stop(detail.id.clone())),
                forced,
            ),
        )
    } else {
        buttons::text(ui, Button::disabled(texts.stop()))
    };

    row![titles, launch, stop]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .into()
}

/// 元信息网格：列数由 [`layout::meta_columns`]（按正文列宽）决定 —— **1180 与 900 都是 2 列**。
///
/// 3 列需要 `3 × META_CARD_MIN_WIDTH + 2 × META_GAP` = 504px，而行宽上限是 424，**数学上不可行**；
/// 硬塞回 3 列会让 `NeoForge 20.4.237`（118px）这类值按词折行、同排卡底不齐。
fn meta_grid<'a>(ui: &'a Ui, metrics: FrameMetrics, detail: &'a Detail) -> Element<'a, Message> {
    let mut grid = column![].spacing(layout::META_GAP).width(Length::Fill);
    let columns = metrics.meta_columns.clamp(1, detail.meta.len().max(1));

    for chunk in detail.meta.chunks(columns) {
        let mut cells_row = row![].spacing(layout::META_GAP).width(Length::Fill);
        for cell in chunk {
            let card = frame::card(
                ui,
                column![
                    frame::text_role(
                        ui,
                        TypeRole::LabelMedium,
                        false,
                        cell.label.as_str(),
                        ui.color(Role::OnSurfaceVariant),
                    ),
                    frame::text_role(
                        ui,
                        TypeRole::BodyMedium,
                        false,
                        cell.value.as_str(),
                        ui.color(Role::OnSurface)
                    ),
                ]
                .spacing(ui.space(Space::X1))
                .width(Length::Fill),
            );
            cells_row = cells_row.push(container(card).width(Length::Fill));
        }
        grid = grid.push(cells_row);
    }

    grid.into()
}

fn action_row<'a>(ui: &'a Ui, forced: Option<Interaction>, texts: Texts) -> Element<'a, Message> {
    row![
        buttons::text(
            ui,
            forced_state(
                Button::new(texts.duplicate(), Message::Action(Action::Duplicate)),
                forced,
            )
        ),
        buttons::text(
            ui,
            forced_state(
                Button::new(texts.open_folder(), Message::Action(Action::OpenFolder)),
                forced,
            )
        ),
        buttons::text(
            ui,
            forced_state(
                Button::new(texts.delete(), Message::Action(Action::Delete)),
                forced
            ),
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center)
    .into()
}

pub(super) fn detail_empty<'a>(ui: &'a Ui, texts: Texts) -> Element<'a, Message> {
    frame::empty_state(
        ui,
        IconKind::Instances,
        texts.empty_title(),
        texts.empty_body(),
        Vec::new(),
    )
}
