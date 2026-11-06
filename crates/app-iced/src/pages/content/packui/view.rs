//! 导入/导出区的画法：入口（拖拽 + 路径）→ 进度（**下载页那块面板**）→ 结果 → 导出预检。
//!
//! ## 版面
//!
//! ```text
//! ┌ 导入整合包 ──────────────────────────────────┐
//! │ ┌ 拖拽区（hover 时变主色描边） ─────────────┐ │
//! │ └ 路径输入框 ──────────┘ [导入]             │ │
//! │ [装到实例：chips] [实例名]                   │ │
//! ├ 进度：步骤 2/5 · 解包覆盖内容                 │
//! │   <下载页的安装面板：进度条 + 计数 + 取消>      │
//! ├ 探测失败：看起来像 X，但缺 Y                  │
//! ├ 结果：三态标题 + 缺文件逐条 + 冲突逐条 + 致命块  │
//! └ 导出：格式 chips + 预检（将内联 N / 无法导出 M）│
//! ```
//!
//! ## 三处「视觉可区分」不是靠颜色一种通道
//!
//! | 域 | 图标 | 颜色角色 | 容器 | 措辞 |
//! |---|---|---|---|---|
//! | 容忍（缺文件 / 被拒 component） | `Warn` | `Tertiary` | 无底色 | 「可容忍」 |
//! | 致命（已回滚） | `Error` | `Error` | `ErrorContainer` | 「已回滚 —— 零残留」 |
//! | 成功 | `Check` | `Primary` | 无底色 | 「已导入」 |
//!
//! 关掉颜色也还认得出（图标 + 措辞），这是任务书「必须视觉可区分」的落点。
//!
//! ## 进度只有一份
//!
//! 进行中的进度**整块交给** `pages::download::install_panel`（同一个函数、同一套几何与文案）；
//! 本区只额外画一行 [`Running::stage_line`] 补足「解包 / overrides / 补文件」三档的粒度。
//!
//! ## 文件分工（守 400 行）
//!
//! | 文件 | 职责 |
//! |---|---|
//! | 本文件 | 区入口 + 拖拽区 + 路径输入 + 目标实例 |
//! | [`parts`] | 三个共用小件（底色容器 / 容忍标题 / 逐条列表） |
//! | [`progress`] | 进度块（复用面板）与探测失败块 |
//! | [`report`] | 结果块：三态 + 冲突 + 致命 |
//! | [`export`] | 导出块与预检块 |

mod export;
mod parts;
mod progress;
mod report;

use iced::widget::{column, container, row};
use iced::{Alignment, Background, Border, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::app::components::divider;
use crate::app::components::inputs::{text_field, Field};
use crate::app::components::selection::{chip, Chip};
use crate::pages::frame;
use crate::pages::Message as PageMessage;
use crate::theme::Ui;

use self::export::export_block;
use self::progress::{probe_block, running_block};
use self::report::report_block;
use super::model::Pack;
use super::PackMessage;

/// 内容库意图 → 页面消息（唯一的一层包装）。
pub(super) fn intent(message: PackMessage) -> PageMessage {
    PageMessage::Content(super::super::Message::Pack(message))
}

/// 整块（导入入口 + 进度 + 结果 + 导出）。
pub(crate) fn section<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Pack,
) -> Element<'a, PageMessage> {
    let mut block = column![].spacing(ui.space(Space::X3)).width(Length::Fill);

    // **一次只摊开一件事**：入口（拖什么/装到哪）、进度、结果、导出预检各占一态。
    // 全都堆在一起时，内容库页正列没有整页滚动 ⇒ 动作按钮被顶出屏幕（出图实测：冲突态与
    // 导出预检态的「确认写盘 / 关闭」都落在 900px 以下）。收起的入口不会丢功能：拖拽是窗口级
    // 事件，任何时候把文件拖进来都算数。
    if model.export.open {
        block = block.push(export_block(ui, catalog, &model.export));
    } else if let Some(running) = &model.running {
        block = block.push(running_block(ui, catalog, running));
    } else if let Some(report) = &model.report {
        block = block.push(report_block(ui, catalog, report));
    } else if model.cancelled {
        // 取消过、还没有新结果：把「后台还在跑」这句话留在屏上（复审 r1 P1-2）
        block = block.push(
            column![
                frame::text_role(
                    ui,
                    TypeRole::BodySmall,
                    false,
                    model.cancelled_note,
                    ui.color(Role::Tertiary),
                ),
                buttons::text(
                    ui,
                    Button::new(catalog.text(Key::PackDismiss), intent(PackMessage::Dismiss)),
                ),
            ]
            .spacing(ui.space(Space::X1))
            .width(Length::Fill),
        );
    } else {
        block = block.push(entry(ui, catalog, model));
        if let Some(probe) = &model.probe {
            // 探测失败要和路径输入框在一起：用户下一步就是改路径
            block = block.push(probe_block(ui, catalog, probe));
        }
        block = block.push(divider::full(ui));
        block = block.push(export_block(ui, catalog, &model.export));
    }

    container(block).width(Length::Fill).into()
}

/// 入口：拖拽区 + 路径输入 + 目标实例。
fn entry<'a>(ui: &'a Ui, catalog: Catalog, model: &'a Pack) -> Element<'a, PageMessage> {
    let zone = container(
        column![
            frame::text_role(
                ui,
                TypeRole::BodyMedium,
                false,
                if model.hover {
                    catalog.text(Key::PackDropActive)
                } else {
                    model.drop_hint
                },
                ui.color(if model.hover {
                    Role::Primary
                } else {
                    Role::OnSurface
                }),
            ),
            // 没有系统文件框这件事**写在界面上**：不能让用户点了半天「选择文件」找不到按钮
            frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                model.no_dialog_note,
                ui.color(Role::OnSurfaceVariant),
            ),
        ]
        .spacing(ui.space(Space::X1))
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding(ui.space(Space::X4))
    .style(move |_theme| container::Style {
        border: Border {
            color: ui.color(if model.hover {
                Role::Primary
            } else {
                Role::Outline
            }),
            width: 1.0,
            radius: ui.radius_dp(ShapeScale::Large),
        },
        background: model
            .hover
            .then(|| Background::Color(ui.color(Role::PrimaryContainer))),
        ..container::Style::default()
    });

    let field = text_field(
        ui,
        Field::new(model.draft.as_str())
            .label(catalog.text(Key::PackPathLabel))
            .placeholder(catalog.text(Key::PackPathPlaceholder))
            .leading(IconKind::Folder),
        move |text| intent(PackMessage::PathChanged(text)),
    );
    let submit = buttons::filled(
        ui,
        Button::new(
            catalog.text(Key::PackImportAction),
            intent(PackMessage::Import),
        )
        .leading(IconKind::Folder),
    );

    let mut block = column![
        frame::text_role(
            ui,
            TypeRole::TitleSmall,
            true,
            catalog.text(Key::PackImportTitle),
            ui.color(Role::OnSurface),
        ),
        zone,
        row![field, submit]
            .spacing(ui.space(Space::X2))
            .align_y(Alignment::End)
            .width(Length::Fill),
        targets(ui, catalog, model),
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill);

    if model.instance.is_none() {
        block = block.push(text_field(
            ui,
            Field::new(model.instance_name.as_str()).label(catalog.text(Key::PackInstanceName)),
            move |text| intent(PackMessage::SetInstanceName(text)),
        ));
    }

    block.into()
}

/// 目标实例那一行（chips + 「新建实例」）。
fn targets<'a>(ui: &'a Ui, catalog: Catalog, model: &'a Pack) -> Element<'a, PageMessage> {
    let mut chips: Vec<Element<'a, PageMessage>> = Vec::new();
    for target in &model.targets {
        chips.push(chip(
            ui,
            Chip::filter(
                target.label.as_str(),
                model.instance.as_deref() == Some(target.id.as_str()),
                intent(PackMessage::SetInstance(Some(target.id.clone()))),
            ),
        ));
    }
    chips.push(chip(
        ui,
        Chip::filter(
            catalog.text(Key::PackTargetNew),
            model.instance.is_none(),
            intent(PackMessage::SetInstance(None)),
        ),
    ));

    column![
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            catalog.text(Key::PackTargetLabel),
            ui.color(Role::OnSurfaceVariant),
        ),
        row(chips).spacing(ui.space(Space::X2)).wrap(),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}
