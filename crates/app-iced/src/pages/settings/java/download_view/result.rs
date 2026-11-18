//! 「下载成功 / 下载失败」那两行 + 字节格式化（M197）。
//!
//! 从 `download_view.rs` 拆出来守 400 行硬线：那个文件只剩「目录 / 下载中」两态，
//! 结果两态与单位换算在这里。

use iced::widget::column;
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};

use crate::pages::frame::text_role_owned;
use crate::pages::Message;
use crate::theme::Ui;

use super::super::downloads::{Failure, Success, Try};

/// 成功：装到哪 + 命中的源（或「复用已装，没走网络」）。
pub(super) fn done<'a>(ui: &'a Ui, catalog: Catalog, success: &Success) -> Element<'a, Message> {
    let text = if success.reused {
        catalog.render(
            Key::SettingsJavaDownloadDoneReused,
            &[
                ("major", success.major.to_string().as_str()),
                ("vendor", success.vendor),
                ("root", success.root.as_str()),
            ],
        )
    } else {
        catalog.render(
            Key::SettingsJavaDownloadDone,
            &[
                ("major", success.major.to_string().as_str()),
                ("vendor", success.vendor),
                ("root", success.root.as_str()),
                ("source", success.source.as_deref().unwrap_or("?")),
            ],
        )
    };
    text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        text,
        ui.color(Role::OnSurface),
    )
}

/// 失败：大版本 + 试过哪些源（各自的失败分类）+ 下一步（core 的原句只进日志）。
pub(super) fn failed<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    failure: &Failure,
    tries: &[Try],
) -> Element<'a, Message> {
    let text = match (&failure.source, failure.kind) {
        (Some(source), _) => catalog.render(
            Key::SettingsJavaDownloadFailed,
            &[
                ("major", failure.major.to_string().as_str()),
                ("source", source.as_str()),
            ],
        ),
        (None, _) => catalog.render(
            Key::SettingsJavaDownloadFailedNoSource,
            &[("major", failure.major.to_string().as_str())],
        ),
    };
    let mut block = column![text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        text,
        ui.color(Role::Error)
    )]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);
    // 「试过哪些源、各自为什么失败」：任务书那条「失败要能说出命中哪个源与退避原因」在
    // **最终失败**那一刻的落点（过程中的实时退避在 `download_view::running` 那一块）。
    if !tries.is_empty() {
        block = block.push(text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog
                .text(Key::SettingsJavaDownloadTriedHeader)
                .to_string(),
            ui.color(Role::OnSurfaceVariant),
        ));
        for attempt in tries.iter().take(4) {
            block = block.push(text_role_owned(
                ui,
                TypeRole::BodySmall,
                false,
                catalog.render(
                    Key::SettingsJavaDownloadTried,
                    &[
                        ("source", attempt.source.as_str()),
                        (
                            "reason",
                            super::source_reason(catalog, attempt.kind).as_str(),
                        ),
                    ],
                ),
                ui.color(Role::OnSurfaceVariant),
            ));
        }
    }
    block = block.push(text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        catalog
            .text(Key::SettingsJavaDownloadFailedHint)
            .to_string(),
        ui.color(Role::OnSurfaceVariant),
    ));
    block.into()
}

/// 字节 → 「88.4 MB」（分母未知时也只说下了多少）。
pub(super) fn mega(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mb = bytes as f64 / (1024.0 * 1024.0);
    format!("{mb:.1} MB")
}
