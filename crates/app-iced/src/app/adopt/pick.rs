//! Pick 段：候选列表 + 目标档 + 实例名。
//!
//! **可导入的候选才可点**：不可导入的行把 `problems` 逐条列在下面但不接点击——点它只会换来
//! 一条注定被 core 拒绝的命令（`adopt_not_adoptable`），而用户从行上已经看到了原因。
//!
//! 目标档三枚 chip 的**短提示**在选中时立刻换（共享/版本目录档用 `Error` 色）；
//! **后果原文**不在这里——它在 Preview 段逐条画 `plan.notes`（core `target_notes()` 的原文）。

use iced::widget::{button, column, container, row};
use iced::{Alignment, Background, Border, Color, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space as Sp;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{AdoptCandidateView, AdoptPlanView, AdoptScanView, AdoptTargetView};

use crate::app::components::inputs::{text_field, Field};
use crate::app::components::selection::{chip, Chip};
use crate::pages::frame;
use crate::theme::Ui;

use super::super::message::Message;
use super::text::{label, problem_line, target_label, wrapped};
use super::{adopt, AdoptMessage, Form};

/// Pick 段整体。
pub(super) fn pick<'a>(
    ui: &'a Ui,
    form: &'a Form,
    scan: Option<&'a AdoptScanView>,
    plan: Option<&'a AdoptPlanView>,
    catalog: Catalog,
) -> Element<'a, Message> {
    let Some(scan) = scan else {
        // 扫描还没回来：一行进度文案（不做假进度条）
        return wrapped(
            ui,
            catalog.text(Key::AdoptScanning).to_string(),
            ui.color(Role::OnSurfaceVariant),
        );
    };

    let mut body = column![].spacing(ui.space(Sp::X3)).width(Length::Fill);
    if scan.is_empty() {
        let dir = scan.versions_dir.display().to_string();
        body = body.push(wrapped(
            ui,
            catalog.render(Key::AdoptEmpty, &[("dir", &dir)]),
            ui.color(Role::OnSurfaceVariant),
        ));
    } else {
        body = body.push(label(ui, catalog.text(Key::AdoptSelectLabel)));
        body = body.push(candidate_list(ui, form, scan, catalog));
    }

    body = body.push(frame::divider(ui).map(Message::Page));
    body = body.push(label(ui, catalog.text(Key::AdoptTargetLabel)));
    body = body.push(target_row(ui, form, catalog));
    // 后果原文（契约 §5.2）：共享档 / 版本目录档一被点选就显示 core `target_notes()` 的原文——
    // 这一条来自**零写盘**的自动预检（见 `AdoptMessage::Target` 的落点），不是界面另写的一句。
    if let Some(plan) = plan {
        for note in &plan.notes {
            body = body.push(wrapped(ui, note.clone(), ui.color(Role::OnSurfaceVariant)));
        }
    }
    body = body.push(text_field(
        ui,
        Field::new(form.name.as_str()).label(catalog.text(Key::AdoptNameLabel)),
        |value| adopt(AdoptMessage::Name(value)),
    ));
    body.into()
}

/// 候选列表（可滚，定高：列表再长页脚按钮也留在卡里）。
fn candidate_list<'a>(
    ui: &'a Ui,
    form: &'a Form,
    scan: &'a AdoptScanView,
    catalog: Catalog,
) -> Element<'a, Message> {
    let rows = column(
        scan.candidates
            .iter()
            .map(|candidate| candidate_row(ui, form, candidate, catalog))
            .collect::<Vec<_>>(),
    )
    .spacing(ui.space(Sp::X1))
    .width(Length::Fill);

    rows.into()
}

/// 一条候选。
fn candidate_row<'a>(
    ui: &'a Ui,
    form: &'a Form,
    candidate: &'a AdoptCandidateView,
    catalog: Catalog,
) -> Element<'a, Message> {
    let selected = form.selected.as_deref() == Some(candidate.version_id.as_str());

    let mut items = column![row![
        frame::text_role(
            ui,
            TypeRole::BodyLarge,
            selected,
            candidate.version_id.as_str(),
            ui.color(Role::OnSurface),
        )
        .map(Message::Page),
        container(status(ui, candidate, selected, catalog))
            .width(Length::Fill)
            .align_x(Alignment::End),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center)]
    .spacing(ui.space(Sp::X1))
    .width(Length::Fill);

    // 问题逐条（不折叠）：候选阶段就把「为什么不能导入」说清楚
    for problem in &candidate.problems {
        items = items.push(problem_line(ui, problem));
    }

    let body = container(items)
        .width(Length::Fill)
        .padding(ui.space(Sp::X2))
        .style(move |_theme| container::Style {
            background: selected.then(|| Background::Color(ui.color(Role::SecondaryContainer))),
            border: Border {
                radius: ui.radius_dp(ShapeScale::Medium),
                ..Border::default()
            },
            ..container::Style::default()
        });

    if !candidate.adoptable() {
        // 不可导入的行**不可点**（也就不存在「点了才被告知不行」这条路）
        return body.into();
    }
    button(body)
        .width(Length::Fill)
        .padding(0)
        .style(|_theme, _status| button::Style {
            background: None,
            text_color: Color::TRANSPARENT,
            border: Border::default(),
            shadow: iced::Shadow::default(),
            snap: true,
        })
        .on_press(adopt(AdoptMessage::Select(candidate.version_id.clone())))
        .into()
}

/// 候选右侧那枚状态（可导入 / N 个问题 / 已被某实例使用）。
fn status<'a>(
    ui: &'a Ui,
    candidate: &AdoptCandidateView,
    selected: bool,
    catalog: Catalog,
) -> Element<'a, Message> {
    if !candidate.adoptable() {
        let count = candidate.problems.len().to_string();
        return wrapped(
            ui,
            catalog.render(Key::AdoptBlocked, &[("count", &count)]),
            ui.color(Role::Error),
        );
    }
    if let Some(name) = candidate.registered.first() {
        return wrapped(
            ui,
            catalog.render(Key::AdoptRegistered, &[("name", name)]),
            ui.color(Role::OnSurfaceVariant),
        );
    }
    if selected {
        return chip(ui, Chip::current(catalog.text(Key::AdoptReady)));
    }
    wrapped(
        ui,
        catalog.text(Key::AdoptReady).to_string(),
        ui.color(Role::OnSurfaceVariant),
    )
}

/// 三枚目标 chip（缺省档在最前；共享 / 版本目录档必须**明确点选**）。
fn target_row<'a>(ui: &'a Ui, form: &'a Form, catalog: Catalog) -> Element<'a, Message> {
    let chips = AdoptTargetView::ALL
        .into_iter()
        .map(|target| {
            chip(
                ui,
                Chip::filter(
                    target_label(target, catalog),
                    form.target == target,
                    adopt(AdoptMessage::Target(target)),
                ),
            )
        })
        .collect::<Vec<_>>();

    column![
        row(chips).spacing(ui.space(Sp::X2)).width(Length::Fill),
        target_hint(ui, form.target, catalog),
    ]
    .spacing(ui.space(Sp::X1))
    .width(Length::Fill)
    .into()
}

/// 目标档的一句话说明（点选后立刻换）。**不是后果原文**：原文在 Preview 段逐条画
/// `plan.notes`（core `target_notes()`）。
fn target_hint<'a>(ui: &'a Ui, target: AdoptTargetView, catalog: Catalog) -> Element<'a, Message> {
    let key = match target {
        AdoptTargetView::InstanceDir => Key::AdoptTargetInstanceDirHint,
        AdoptTargetView::Shared => Key::AdoptTargetSharedHint,
        AdoptTargetView::VersionDir => Key::AdoptTargetVersionDirHint,
    };
    let color = if target.touches_foreign_dir() {
        ui.color(Role::Error)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };
    wrapped(ui, catalog.text(key).to_string(), color)
}
