//! 详情**整页**（M455 从 `detail.rs` 拆出：那边加完磁贴视图那一支后到了 426 行）。
//!
//! 两条入口共用同一棵正文：
//!
//! | 入口 | 什么时候 | 多出来的东西 |
//! |---|---|---|
//! | [`super::narrow_detail`] | 窄窗（`SingleColumn`）下点了实例 | 一句「窗口窄于 840 时列表与详情共用一列」的提示 |
//! | [`detail_page`] | **磁贴视图**下点了一张格子 | 没有（见下） |
//!
//! 磁贴视图为什么也走「整页详情」：网格本来就吃满整块宽度（一行 `tile_size.columns()` 个格子），
//! 没有第二条栏能放详情。于是「点格子 → 压一层 `Route::Detail` → 这一屏换成详情」。而那句窄窗
//! 提示在 1180 宽的窗口上会变成假话（列表与详情并没有挤在一列里），所以这里按参数决定要不要它。

use iced::widget::{column, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_presenter::router::Route;

use crate::app::components::buttons::{self, Button};
use crate::app::components::divider;
use crate::app::components::interaction::Interaction;
use crate::theme::Ui;

use crate::pages::layout::FrameMetrics;
use crate::pages::model::Detail;
use crate::pages::{frame, Message};

use super::super::state::LaunchForm;
use super::{
    action_row, detail_head, discovered_note, forced_state, meta_grid, push_text, scroller, Texts,
};

/// **磁贴视图里的详情整页**（M455）：点一张格子 = 压一层 `Route::Detail`，网格让位给详情。
pub fn detail_page<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    detail: &'a Detail,
    form: &'a LaunchForm,
    mods: &'a super::super::Mods,
    saves: &'a super::super::Saves,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    frame::body(
        metrics,
        page_body(ui, metrics, detail, form, mods, saves, forced, texts, false),
    )
}

/// 两条入口共用的正文（`narrow` = 要不要那句窄窗提示）。
#[allow(clippy::too_many_arguments)]
pub fn page_body<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    detail: &'a Detail,
    form: &'a LaunchForm,
    mods: &'a super::super::Mods,
    saves: &'a super::super::Saves,
    forced: Option<Interaction>,
    texts: Texts,
    narrow: bool,
) -> Element<'a, Message> {
    // 列表退成上一屏，本屏只放详情（官方 compact 的 list-detail 口径）。
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

    if let Some(note) = discovered_note(ui, detail, texts) {
        body = body.push(note);
    }
    body = push_text(ui, metrics, body, detail);
    // 顺序与宽窗那一支一致：内容（MODS）在选项（启动参数）之前，理由见 `detail_column`。
    body = body.push(super::super::mods::section(ui, mods, forced, texts));
    body = body.push(super::super::saves::section(ui, saves, forced, texts));
    body = body.push(super::super::launch_args::section(
        ui,
        &detail.launch,
        form,
        texts,
    ));

    if narrow {
        body = body.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            texts.narrow_note(),
            ui.color(Role::OnSurfaceVariant),
        ));
    }

    scroller(ui, body.push(action_row(ui, forced, texts)))
}
