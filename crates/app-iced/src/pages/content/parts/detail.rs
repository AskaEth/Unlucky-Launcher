//! 详情面板：选中项的支持范围 + 目标实例 + 装入（不兼容时**禁用并逐条给原因**）。
//!
//! 从 `pages/content/parts.rs` 拆出（那个文件已贴 400 行硬上限）。
//!
//! 这块是「一键装入指定实例」的**唯一**入口：按钮能不能按由 [`Content::can_install`] 决定，
//! 按不动时旁边逐条列出原因——判定与画法分开，`model/tests.rs` 才能不开图形环境就钉住它。

use iced::widget::{column, container};
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::app::components::divider;
use crate::app::components::selection::{chip, Chip};
use crate::theme::Ui;

use super::super::model::{capitalize, Content};
use super::super::Message as ContentMessage;
use super::{intent, labelled};
use crate::pages::frame;
use crate::pages::Message as PageMessage;

/// 详情面板：选中项的支持范围 + 目标实例 + 装入（不兼容时**禁用并给原因**）。
pub(crate) fn detail<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
) -> Element<'a, PageMessage> {
    let Some(hit) = model.selected_hit() else {
        return container(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::ContentDetailEmpty),
            ui.color(Role::OnSurfaceVariant),
        ))
        .width(Length::Fill)
        .into();
    };

    let head = column![
        frame::text_role(
            ui,
            TypeRole::TitleMedium,
            true,
            hit.title.as_str(),
            ui.color(Role::OnSurface)
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            hit.supporting.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            hit.summary.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    let meta = column![
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            hit.downloads.as_str(),
            ui.color(Role::OnSurfaceVariant)
        ),
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            hit.updated.as_str(),
            ui.color(Role::OnSurfaceVariant)
        ),
        support_line(ui, catalog, Key::ContentDetailVersions, &hit.versions),
        support_line(ui, catalog, Key::ContentDetailLoaders, &hit.loaders),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    let mut body = column![head, divider::full(ui), meta].spacing(ui.space(Space::X3));

    // M162：可用版本（详情通道拉的；最多画 5 行——列表页负责「多」，这里只负责「有哪些」）
    if model.available_versions.is_empty() {
        if let Some(note) = model.versions_note.as_deref() {
            body = body.push(frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                note,
                ui.color(Role::OnSurfaceVariant),
            ));
        }
    } else {
        let count = model.available_versions.len().to_string();
        let mut versions = column![frame::text_role_owned(
            ui,
            TypeRole::LabelMedium,
            true,
            catalog.render(Key::ContentDetailVersionCount, &[("count", count.as_str())]),
            ui.color(Role::OnSurfaceVariant),
        )]
        .spacing(ui.space(Space::X1));
        for version in model.available_versions.iter().take(5) {
            versions = versions.push(frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                version.line.as_str(),
                ui.color(Role::OnSurfaceVariant),
            ));
        }
        body = body.push(versions);
    }

    // 目标实例：一行 chip（选中的那个高亮）
    if model.targets.is_empty() {
        body = body.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::ContentNoInstances),
            ui.color(Role::OnSurfaceVariant),
        ));
    } else {
        let mut targets = Vec::new();
        for target in &model.targets {
            targets.push(chip(
                ui,
                Chip::filter(
                    target.name.as_str(),
                    model.target.as_deref() == Some(target.id.as_str()),
                    intent(ContentMessage::SetTarget(target.id.clone())),
                ),
            ));
        }
        body = body.push(labelled(
            ui,
            catalog.text(Key::ContentTargetInstance),
            targets,
        ));
    }

    // 装入：能装 ⇒ 实心按钮；不能 ⇒ 禁用 + 逐条原因（**点之前**就说清楚）
    if model.can_install() {
        body = body.push(buttons::filled(
            ui,
            Button::new(
                catalog.text(Key::ContentInstall),
                intent(ContentMessage::Install),
            )
            .leading(IconKind::Download),
        ));
    } else {
        body = body.push(buttons::outlined(
            ui,
            Button::disabled(catalog.text(Key::ContentInstall)),
        ));
        for reason in model.blocker_reasons(catalog) {
            body = body.push(frame::text_role_owned(
                ui,
                TypeRole::BodySmall,
                false,
                reason,
                ui.color(Role::OnSurfaceVariant),
            ));
        }
    }

    container(body.spacing(ui.space(Space::X3)))
        .width(Length::Fill)
        .into()
}

/// 「游戏版本：1.20.1, 1.20.4」一行（空列表给 `—`，不编造）。
fn support_line<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    key: Key,
    values: &[String],
) -> Element<'a, PageMessage> {
    let list = if values.is_empty() {
        "—".to_string()
    } else {
        // 加载器是站点给的小写 facet（`fabric`）：这一层只管怎么显示
        values
            .iter()
            .map(|value| capitalize(value))
            .collect::<Vec<_>>()
            .join(", ")
    };
    frame::text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        catalog.render(key, &[("list", &list)]),
        ui.color(Role::OnSurfaceVariant),
    )
}
