//! 结果块：三态标题 + 加载器步骤段 + 容忍域逐条 + 冲突逐条 + 致命块。

use iced::widget::{column, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::ImportIssueLevelView;

use crate::app::components::buttons::{self, Button};
use crate::pages::frame;
use crate::pages::Message as PageMessage;
use crate::theme::Ui;

use super::super::model::{ConflictRow, FatalKind, Report};
use super::super::PackMessage;
use super::intent;
use super::parts::{bounded, issue_section, marker_box, rows_block, tolerable_title};
use super::report_processor::processor_block;

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
        // **解到哪个目录 / 登记的实例名**（M311）：用户要知道内容落哪了、档案叫什么。
        // 两条都可缺：`None` 时画空串（整句不出，标签也不出）——宿主侧失败那几档没有目标目录。
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            report.target_line.as_deref().unwrap_or(""),
            ui.color(Role::OnSurfaceVariant),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            report.registered_line.as_deref().unwrap_or(""),
            ui.color(Role::OnSurfaceVariant),
        ),
        // 「这个版本的加载器还没装」——与 CLI 同一句，用 Tertiary 而不是红色：
        // 这不是失败，是"还要再做一步"。
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            report.loader_line.as_deref().unwrap_or(""),
            ui.color(Role::Tertiary),
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
                    frame::text_role(
                        ui,
                        TypeRole::BodyMedium,
                        false,
                        fatal.label.as_str(),
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

    // 加载器安装步骤（M328）：**概览之后、缺文件段之前**。用户第一问是「加载器装上了没」，
    // 缺件明细是第二问——别把「装了哪几步」压在二十条缺文件下面。
    if let Some(section) = &report.processor {
        block = block.push(processor_block(ui, section));
    }

    // 容忍域（缺文件 / 被拒 component）：中性容器 + `Warn` 图标 + 逐条
    if let Some(title) = &report.missing_title {
        block = block.push(
            column![
                tolerable_title(ui, title.clone(), None),
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
                tolerable_title(ui, title.clone(), None),
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
                tolerable_title(ui, title.clone(), None),
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

    // **M339**：两档分开画，标题里的计数按级别算（`Report` 侧切好，这里只负责排版）。
    // **M341**：画法与预检那一屏共用（[`issue_section`]）——同一批 core 注记在两屏不许两种排法。
    // 这两段在这里一直铺开：报告自己就是一屏结果，收口与否是版面决定，改它要先出图。
    if let Some(title) = &report.warnings_title {
        block = block.push(issue_section(
            ui,
            ImportIssueLevelView::Warning,
            title.clone(),
            &report.warnings,
            None,
        ));
    }

    if let Some(title) = &report.notes_title {
        block = block.push(issue_section(
            ui,
            ImportIssueLevelView::Note,
            title.clone(),
            &report.notes,
            None,
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
