//! 导出块与预检块：格式选择 → **预检**（不写盘）→ 确认写盘。

use iced::widget::{column, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::app::components::inputs::{text_field, Field};
use crate::app::components::selection::{chip, Chip};
use crate::pages::frame;
use crate::pages::Message as PageMessage;
use crate::theme::Ui;

use super::super::model::{Export, Preview};
use super::super::PackMessage;
use super::intent;
use super::parts::{bounded, rows_block, tolerable_title};

/// 导出块。
pub(super) fn export_block<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    export: &'a Export,
) -> Element<'a, PageMessage> {
    let mut block = column![].spacing(ui.space(Space::X2)).width(Length::Fill);

    if !export.open {
        block = block.push(
            row![
                buttons::outlined(
                    ui,
                    Button::new(
                        catalog.text(Key::PackExportOpen),
                        intent(PackMessage::OpenExport)
                    )
                    .leading(IconKind::Folder),
                ),
                frame::text_role(
                    ui,
                    TypeRole::BodySmall,
                    false,
                    export.format_note,
                    ui.color(Role::OnSurfaceVariant),
                ),
            ]
            .spacing(ui.space(Space::X2))
            .align_y(Alignment::Center),
        );
    } else {
        block = block.push(frame::text_role(
            ui,
            TypeRole::TitleSmall,
            true,
            catalog.text(Key::PackExportTitle),
            ui.color(Role::OnSurface),
        ));
        block = block.push(frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            catalog.text(Key::PackExportFormat),
            ui.color(Role::OnSurfaceVariant),
        ));
        block = block.push(
            row(export
                .formats
                .iter()
                .map(|(format, label)| {
                    chip(
                        ui,
                        Chip::filter(
                            label,
                            export.format == *format,
                            intent(PackMessage::SetFormat(*format)),
                        ),
                    )
                })
                .collect::<Vec<_>>())
            .spacing(ui.space(Space::X2)),
        );
        block = block.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            export.format_note,
            ui.color(Role::OnSurfaceVariant),
        ));
        block = block.push(text_field(
            ui,
            Field::new(export.name.as_str()).label(catalog.text(Key::PackExportName)),
            move |text| intent(PackMessage::SetExportName(text)),
        ));
        block = block.push(text_field(
            ui,
            Field::new(export.version.as_str()).label(catalog.text(Key::PackExportVersion)),
            move |text| intent(PackMessage::SetExportVersion(text)),
        ));
        // 标签在上、占位在下：**别让占位文字看起来像已填的值**（复审提过这张图的观感）
        block = block.push(text_field(
            ui,
            Field::new(export.output.as_str())
                .label(catalog.text(Key::PackExportOutput))
                .placeholder(catalog.text(Key::PackExportOutputPlaceholder)),
            move |text| intent(PackMessage::SetOutput(text)),
        ));
        // 有预检结果时这里只留「预检」（`Back` 交给预览块那一个）——复审 r2 P3-1：
        // 搬确认按钮进来时把 `Back` 带出了两份，同屏两个一样的按钮
        let mut actions = row![buttons::filled(
            ui,
            Button::new(
                catalog.text(Key::PackExportPrecheck),
                intent(PackMessage::Precheck),
            )
            .leading(IconKind::Search),
        )]
        .spacing(ui.space(Space::X2));
        if export.preview.is_none() {
            actions = actions.push(buttons::text(
                ui,
                Button::new(
                    catalog.text(Key::PackExportAbort),
                    intent(PackMessage::AbortExport),
                ),
            ));
        }
        block = block.push(actions);
        if export.pending {
            block = block.push(frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                export.pending_line,
                ui.color(Role::OnSurfaceVariant),
            ));
        }
        if let Some(preview) = &export.preview {
            // 动作在 `preview_block` 里紧跟摘要（明细再长也不会把「确认写盘」顶出屏幕）
            block = block.push(preview_block(ui, catalog, preview));
        }
    }

    if let Some(done) = &export.done {
        block = block.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            done.as_str(),
            ui.color(Role::Primary),
        ));
    }
    if let Some(failed) = &export.failed {
        block = block.push(frame::text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.render(Key::PackExportFailed, &[("reason", failed)]),
            ui.color(Role::Error),
        ));
    }

    block.into()
}

/// 预检块：`将内联 N / 无法导出 M` + 逐条跳过项 + 已知损失。
pub(super) fn preview_block<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    preview: &'a Preview,
) -> Element<'a, PageMessage> {
    let mut block = column![
        frame::text_role(
            ui,
            TypeRole::TitleSmall,
            true,
            preview.head.as_str(),
            ui.color(if preview.complete {
                Role::Primary
            } else {
                Role::Tertiary
            }),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            preview.complete_line.as_str(),
            ui.color(if preview.complete {
                Role::OnSurfaceVariant
            } else {
                Role::Tertiary
            }),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            preview.remote_line.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            preview.inline_line.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            preview.target_line.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    // **动作紧跟摘要**：明细（13 条 skipped + 样例 + 已知损失）再长，也不会把
    // 「Write the pack」顶出 900px（复审 r1 P2-3 的正解：先给结论与动作，明细往下滚）
    block = block.push(
        row![
            buttons::filled(
                ui,
                Button::new(
                    catalog.text(Key::PackExportConfirm),
                    intent(PackMessage::Confirm),
                ),
            ),
            buttons::text(
                ui,
                Button::new(
                    catalog.text(Key::PackExportAbort),
                    intent(PackMessage::AbortExport)
                ),
            ),
        ]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center),
    );

    if !preview.skipped.is_empty() {
        // 标题写成「Not in the pack: 13」而不是「逐条列出」：13 条**确实**全列出来了，
        // 但下面那个「…还有 N 个（这里不展开）」说的是**内联样例**，两句挨着容易被读成矛盾
        block = block.push(tolerable_title(
            ui,
            catalog.render(
                Key::PackPreviewSkippedTitle,
                &[("count", &preview.skipped.len().to_string())],
            ),
        ));
        // 13 条是真实形态：摊开会把「Write the pack」顶出 900px（复审 r1 P2-3）
        block = block.push(bounded(
            ui,
            rows_block(ui, catalog, &preview.skipped),
            preview.skipped.len(),
            3,
            110.0,
        ));
    }

    // 样例 + 已知损失一起套有界滚动：预检是「摊开给你看」的块，条目多时它会把
    // 「确认写盘」顶出屏幕（出图时实测到了）
    let mut extras = column![].spacing(ui.space(Space::X1)).width(Length::Fill);
    if !preview.inline_sample.is_empty() {
        // 给样例一句自己的标题 + 说明只列前几条（截断要说出来）
        extras = extras.push(frame::text_role_owned(
            ui,
            TypeRole::LabelMedium,
            false,
            catalog.render(
                Key::PackPreviewInlineTitle,
                &[("shown", &preview.inline_sample.len().to_string())],
            ),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    for path in &preview.inline_sample {
        extras = extras.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            path.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    if let Some(more) = &preview.more_line {
        extras = extras.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            more.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    if !preview.notes.is_empty() {
        extras = extras.push(frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            catalog.text(Key::PackPreviewNotes),
            ui.color(Role::OnSurfaceVariant),
        ));
        for note in &preview.notes {
            extras = extras.push(frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                note.as_str(),
                ui.color(Role::OnSurfaceVariant),
            ));
        }
    }
    let extras_rows = preview.inline_sample.len() + preview.notes.len();
    block = block.push(bounded(ui, extras, extras_rows, 3, 120.0));

    block.into()
}
