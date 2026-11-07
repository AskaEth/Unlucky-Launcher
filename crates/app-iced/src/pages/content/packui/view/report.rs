//! 结果块：三态标题 + 容忍域逐条 + 冲突逐条 + 致命块。

use iced::widget::{column, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::pages::frame;
use crate::pages::Message as PageMessage;
use crate::theme::Ui;

use super::super::model::{ConflictRow, Report};
use super::super::PackMessage;
use super::intent;
use super::parts::{bounded, marker_box, rows_block, tolerable_title};

/// 结果块。
pub(super) fn report_block<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    report: &'a Report,
) -> Element<'a, PageMessage> {
    let mut block = column![
        row![
            frame::glyph(
                status_icon(report),
                ui.color(status_role(report)),
                frame::ICON_PX
            ),
            frame::text_role(
                ui,
                TypeRole::TitleSmall,
                true,
                report.title.as_str(),
                ui.color(status_role(report)),
            ),
        ]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            report.subtitle.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            report.written_line.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    if let Some(fatal) = &report.fatal {
        block = block.push(marker_box(
            ui,
            Role::ErrorContainer,
            column![
                frame::text_role(
                    ui,
                    TypeRole::LabelLarge,
                    true,
                    fatal.title.as_str(),
                    ui.color(Role::OnErrorContainer),
                ),
                frame::text_role_owned(
                    ui,
                    TypeRole::BodyMedium,
                    false,
                    format!("{} · {}", fatal.label, fatal.code),
                    ui.color(Role::OnErrorContainer),
                ),
                frame::text_role(
                    ui,
                    TypeRole::BodySmall,
                    false,
                    fatal.message.as_str(),
                    ui.color(Role::OnErrorContainer),
                ),
            ]
            .spacing(ui.space(Space::X1))
            .width(Length::Fill),
        ));
        if let Some(hint) = &fatal.hint {
            block = block.push(
                row![
                    frame::text_role(
                        ui,
                        TypeRole::LabelMedium,
                        false,
                        catalog.text(Key::PackFatalHint),
                        ui.color(Role::OnSurfaceVariant),
                    ),
                    frame::text_role(
                        ui,
                        TypeRole::BodySmall,
                        false,
                        hint.as_str(),
                        ui.color(Role::OnSurfaceVariant),
                    ),
                ]
                .spacing(ui.space(Space::X2)),
            );
        }
    }

    // 容忍域（缺文件 / 被拒 component）：中性容器 + `Warn` 图标 + 逐条
    if let Some(title) = &report.missing_title {
        block = block.push(
            column![
                tolerable_title(ui, title.clone()),
                bounded(
                    ui,
                    rows_block(ui, catalog, &report.missing),
                    report.missing.len(),
                    3,
                    120.0,
                ),
            ]
            .spacing(ui.space(Space::X1))
            .width(Length::Fill),
        );
    }
    if let Some(title) = &report.rejected_title {
        // 没有这句的话，「Could not fetch (tolerated): 3」下面会多出一行没有归属的
        // 「not supported by the client」（复审 r1 P2-4）
        block = block.push(
            column![
                tolerable_title(ui, title.clone()),
                // 与 missing / conflicts 同一条口径：条目多时框起来滚，别把动作顶出屏幕
                bounded(
                    ui,
                    rows_block(ui, catalog, &report.rejected),
                    report.rejected.len(),
                    3,
                    110.0,
                ),
            ]
            .spacing(ui.space(Space::X1))
            .width(Length::Fill),
        );
    }

    if let Some(title) = &report.conflicts_title {
        block = block.push(
            column![
                tolerable_title(ui, title.clone()),
                bounded(
                    ui,
                    column(
                        report
                            .conflicts
                            .iter()
                            .map(|conflict| conflict_row(ui, catalog, conflict))
                            .collect::<Vec<_>>()
                    )
                    .spacing(ui.space(Space::X1))
                    .width(Length::Fill),
                    report.conflicts.len(),
                    2,
                    160.0,
                ),
            ]
            .spacing(ui.space(Space::X1))
            .width(Length::Fill),
        );
    }

    for warning in &report.warnings {
        block = block.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            warning.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ));
    }

    if let Some(note) = &report.overwrite_note {
        block = block.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            note.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ));
    }

    let mut actions = row![]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center);
    if report.can_overwrite {
        actions = actions.push(buttons::outlined(
            ui,
            Button::new(
                catalog.text(Key::PackOverwriteAction),
                intent(PackMessage::Overwrite),
            )
            .leading(IconKind::Refresh),
        ));
    }
    actions = actions.push(buttons::text(
        ui,
        Button::new(catalog.text(Key::PackDismiss), intent(PackMessage::Dismiss)),
    ));
    block = block.push(actions);

    block.into()
}

/// 一条冲突：路径 + 成因 + 本次处置（点一行展开三方哈希）。
pub(super) fn conflict_row<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    conflict: &'a ConflictRow,
) -> Element<'a, PageMessage> {
    let head = row![
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            conflict.path.as_str(),
            ui.color(Role::OnSurface),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            conflict.reason.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            conflict.choice.as_str(),
            ui.color(Role::Tertiary),
        ),
        buttons::text(
            ui,
            Button::new(
                catalog.text(Key::PackConflictDetail),
                intent(PackMessage::ToggleConflict(conflict.path.clone())),
            )
            .leading(IconKind::ChevronDown),
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let mut block = column![head].width(Length::Fill);
    if conflict.expanded {
        block = block.push(frame::text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.render(
                Key::PackConflictHashes,
                &[
                    ("pack", conflict.pack_sha1.as_str()),
                    ("base", conflict.base_sha1.as_deref().unwrap_or("—")),
                    ("disk", conflict.disk_sha1.as_deref().unwrap_or("—")),
                ],
            ),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    block.into()
}

pub(super) fn status_icon(report: &Report) -> IconKind {
    match report.status {
        unlucky_presenter::dto::ImportStatusView::Success => IconKind::Check,
        unlucky_presenter::dto::ImportStatusView::Partial => IconKind::Warn,
        unlucky_presenter::dto::ImportStatusView::Failed => IconKind::Error,
    }
}

pub(super) fn status_role(report: &Report) -> Role {
    match report.status {
        unlucky_presenter::dto::ImportStatusView::Success => Role::Primary,
        unlucky_presenter::dto::ImportStatusView::Partial => Role::Tertiary,
        unlucky_presenter::dto::ImportStatusView::Failed => Role::Error,
    }
}
