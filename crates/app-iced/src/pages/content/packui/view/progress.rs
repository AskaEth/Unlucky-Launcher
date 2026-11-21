//! 进度块（**整块复用下载页的安装面板**）与探测失败块。

use iced::widget::{column, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::pages::download;
use crate::pages::frame;
use crate::pages::Message as PageMessage;
use crate::theme::Ui;

use super::super::model::{Probe, Running};
use super::super::PackMessage;
use super::intent;
use super::parts::marker_box;

/// 进行中：一行步骤说明 + **下载页那块面板**。
pub(super) fn running_block<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    running: &'a Running,
) -> Element<'a, PageMessage> {
    let mut block = column![frame::text_role(
        ui,
        TypeRole::LabelMedium,
        false,
        running.stage_line.as_str(),
        ui.color(Role::OnSurfaceVariant),
    )]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    // 同一块面板：进度条、计数行、取消按钮都归它，这里不另画一条。
    // `None` 只会在「阶段已收尾」时出现（导入的五个阶段都不是终态），那时只剩步骤行。
    if let Some(panel) = &running.panel {
        block = block.push(download::install_panel(ui, catalog, panel));
    }
    block.into()
}

/// 探测失败：把「缺哪个标记」摊开。
pub(super) fn probe_block<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    probe: &'a Probe,
) -> Element<'a, PageMessage> {
    let mut block = column![
        row![
            frame::glyph(IconKind::Warn, ui.color(Role::Tertiary), frame::ICON_PX),
            frame::text_role(
                ui,
                TypeRole::TitleSmall,
                true,
                catalog.text(Key::PackProbeTitle),
                ui.color(Role::Tertiary),
            ),
        ]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center),
        frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            probe.lead.as_str(),
            ui.color(Role::OnSurface),
        ),
        frame::text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.render(Key::PackProbeSaw, &[("saw", &probe.saw.join(" · "))]),
            ui.color(Role::OnSurfaceVariant),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            probe.detail.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    block = block.push(buttons::text(
        ui,
        Button::new(catalog.text(Key::PackDismiss), intent(PackMessage::Dismiss)),
    ));
    marker_box(ui, Role::TertiaryContainer, block)
}
