//! 内容库页的**状态层**：加载 / 空 / 失败 / 未联网 / 状态分派。
//!
//! 从 `parts.rs` 拆出（M161 合入后那份越过了 400 行；状态层本来就是「一屏只有一个」的
//! 独立东西，拆开后 `parts.rs` 只剩筛选条与列表）。四态各一支，判据在
//! `model::Status`，这里只负责画。
//!
//! **未联网（M162）与失败是两屏**：前者说「要么联网、要么真的没有」，后者说「拉取失败了」。

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::app::components::feedback::{empty_state, EmptyState};
use crate::theme::Ui;

use super::super::model::{Content, Status};
use super::intent;
use crate::pages::content::Message as ContentMessage;
use crate::pages::frame;
use crate::pages::Message as PageMessage;

pub(crate) fn loading<'a>(ui: &'a Ui, catalog: Catalog) -> Element<'a, PageMessage> {
    centered(
        column![
            frame::text_role_centered(
                ui,
                TypeRole::TitleMedium,
                true,
                catalog.text(Key::ContentLoading),
                ui.color(Role::OnSurface)
            ),
            frame::text_role_centered(
                ui,
                TypeRole::BodySmall,
                false,
                catalog.text(Key::ContentLoadingHint),
                ui.color(Role::OnSurfaceVariant)
            ),
            buttons::text(
                ui,
                Button::new(
                    catalog.text(Key::ContentRetry),
                    intent(ContentMessage::Retry)
                )
            ),
        ]
        .spacing(ui.space(Space::X3))
        .width(Length::Fill)
        .align_x(Alignment::Center),
    )
}

/// 空态（真 `EmptyState`：CTA 是标准 Filled 按钮）。
pub(crate) fn empty<'a>(ui: &'a Ui, catalog: Catalog) -> Element<'a, PageMessage> {
    centered(empty_state(
        ui,
        EmptyState::new(
            IconKind::Search,
            catalog.text(Key::ContentEmptyTitle),
            catalog.text(Key::ContentEmptyBody),
        )
        .action(
            catalog.text(Key::ContentRetry),
            intent(ContentMessage::Retry),
        ),
    ))
}

/// 失败态（带原因与重试出口；原因来自 host，不打码不打折）。
pub(crate) fn failed<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    reason: &'a str,
) -> Element<'a, PageMessage> {
    centered(
        column![
            row![
                frame::glyph(IconKind::Error, ui.color(Role::Error), frame::ICON_PX),
                frame::text_role(
                    ui,
                    TypeRole::TitleSmall,
                    true,
                    catalog.text(Key::ContentFailedTitle),
                    ui.color(Role::Error)
                ),
            ]
            .spacing(ui.space(Space::X2))
            .align_y(Alignment::Center),
            frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                reason,
                ui.color(Role::OnSurfaceVariant)
            ),
            frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                catalog.text(Key::ContentFailedBody),
                ui.color(Role::OnSurfaceVariant)
            ),
            buttons::filled(
                ui,
                Button::new(
                    catalog.text(Key::ContentRetry),
                    intent(ContentMessage::Retry)
                )
                .leading(IconKind::Refresh)
            ),
        ]
        .spacing(ui.space(Space::X3))
        .width(Length::Fill)
        .align_x(Alignment::Center),
    )
}

fn centered<'a>(content: impl Into<Element<'a, PageMessage>>) -> Element<'a, PageMessage> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

/// 未联网态（M162）：与失败态同一套骨架，只是说清「要么联网、要么真的没有」这件事。
pub(crate) fn offline<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    reason: &'a str,
) -> Element<'a, PageMessage> {
    centered(
        column![
            row![
                frame::glyph(
                    IconKind::Warn,
                    ui.color(Role::OnSurfaceVariant),
                    frame::ICON_PX
                ),
                frame::text_role(
                    ui,
                    TypeRole::TitleSmall,
                    true,
                    catalog.text(Key::ContentOfflineTitle),
                    ui.color(Role::OnSurface)
                ),
            ]
            .spacing(ui.space(Space::X2))
            .align_y(Alignment::Center),
            frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                catalog.text(Key::ContentOfflineBody),
                ui.color(Role::OnSurfaceVariant)
            ),
            frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                reason,
                ui.color(Role::OnSurfaceVariant)
            ),
            buttons::outlined(
                ui,
                Button::new(
                    catalog.text(Key::ContentRetry),
                    intent(ContentMessage::Retry)
                )
                .leading(IconKind::Refresh),
            ),
        ]
        .spacing(ui.space(Space::X2))
        .align_x(Alignment::Center),
    )
}

/// 状态分派（四态各一支；`Ready` 由调用方画列表 + 详情）。
pub(crate) fn status_body<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
) -> Option<Element<'a, PageMessage>> {
    match &model.status {
        Status::Loading => Some(loading(ui, catalog)),
        Status::Empty => Some(empty(ui, catalog)),
        Status::Failed(reason) => Some(failed(ui, catalog, reason.as_str())),
        // M162：未联网不是「没有结果」，也不是普通失败——单独一屏（标题/正文/图标都不同）
        Status::Offline(reason) => Some(offline(ui, catalog, reason.as_str())),
        Status::Ready => None,
    }
}
