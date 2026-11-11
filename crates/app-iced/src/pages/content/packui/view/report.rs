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

use super::super::model::{ConflictRow, FatalKind, Report};
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
        // 「取消太晚」的那句补充说明（标题仍是真实的 `已导入 / 部分成功`）。
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            report.cancel_note.as_deref().unwrap_or(""),
            ui.color(Role::Tertiary),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    if let Some(fatal) = &report.fatal {
        // 三种阶段各自的底色/图标：预检没过用 `SecondaryContainer`（**不是错的颜色**——
        // 目标目录一个字节都没写，没什么可修的残留），已回滚用 `ErrorContainer`，
        // 回滚不完整留 `ErrorContainer`（它才是真的要对一眼目录的状态）。
        let (role, on_role) = match fatal.kind {
            FatalKind::NotStarted => (Role::SecondaryContainer, Role::OnSecondaryContainer),
            _ => (Role::ErrorContainer, Role::OnErrorContainer),
        };
        block = block.push(marker_box(
            ui,
            role,
            row![
                frame::glyph(fatal.kind.icon(), ui.color(on_role), frame::ICON_PX),
                column![
                    frame::text_role(
                        ui,
                        TypeRole::LabelLarge,
                        true,
                        fatal.title.as_str(),
                        ui.color(on_role),
                    ),
                    frame::text_role_owned(
                        ui,
                        TypeRole::BodyMedium,
                        false,
                        format!("{} · {}", fatal.label, fatal.code),
                        ui.color(on_role),
                    ),
                    frame::text_role(
                        ui,
                        TypeRole::BodySmall,
                        false,
                        fatal.message.as_str(),
                        ui.color(on_role),
                    ),
                ]
                .spacing(ui.space(Space::X1))
                .width(Length::Fill),
            ]
            .spacing(ui.space(Space::X2))
            .align_y(Alignment::Start),
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
    use unlucky_presenter::dto::{ImportCancelView, ImportStatusView};
    match report.status {
        ImportStatusView::Success => IconKind::Check,
        ImportStatusView::Partial => IconKind::Warn,
        ImportStatusView::Failed => IconKind::Error,
        // 取消**不是错误**（M195）：停手用「关闭（叉）」这个描边字形（页面骨架只允许描边，
        // 令牌层的 Play/Stop/More 是填充字形，见 `tests/no_hardcoded_tokens.rs` 的守门），
        // 回滚没做完才升级成 Warn，都不上红。
        ImportStatusView::Cancelled => match report.cancel {
            ImportCancelView::RollbackIncomplete => IconKind::Warn,
            _ => IconKind::Close,
        },
    }
}

pub(super) fn status_role(report: &Report) -> Role {
    use unlucky_presenter::dto::{ImportCancelView, ImportStatusView};
    match report.status {
        ImportStatusView::Success => Role::Primary,
        ImportStatusView::Partial => Role::Tertiary,
        ImportStatusView::Failed => Role::Error,
        // 取消一档也不做成 `Role::Error`：用户自己点的，不该看起来像装坏了。
        ImportStatusView::Cancelled => match report.cancel {
            ImportCancelView::RollbackIncomplete => Role::Tertiary,
            _ => Role::OnSurfaceVariant,
        },
    }
}
