//! Preview / Failed / Done 三段。
//!
//! | 段 | 画什么 | 判据 |
//! |---|---|---|
//! | Preview | 生效运行目录（**回程给的**）+ 隔离档 + `notes` 原文逐条 | `AdoptEvent::Planned` |
//! | Failed | 稳定码 + `problems[]` 逐条 | `AdoptEvent::Failed` |
//! | Done | 实例名 + 运行目录 + `notes` 原文逐条 | `AdoptEvent::Done` |
//!
//! 三段都不自己算路径、不自己写后果句：`run_dir` 与 `notes` 是 core 的返回值原样。
//! 缺省档会带 `-2`/`-3` 后缀（两个不同展示名可能落到同一个物理目录），**只有 core 知道**
//! 最后用了哪个——页面对它再做一次派生就会与真正启动时用的目录分叉。

use iced::widget::column;
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space as Sp;
use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{AdoptDoneView, AdoptFailureView, AdoptPlanView};

use crate::theme::Ui;

use super::super::message::Message;
use super::text::{field_line, isolation_label, mono, problem_line, wrapped};

/// 预检结果：落点 + 隔离档 + 后果原文。
pub(super) fn preview<'a>(
    ui: &'a Ui,
    plan: Option<&'a AdoptPlanView>,
    catalog: Catalog,
) -> Element<'a, Message> {
    let Some(plan) = plan else {
        return wrapped(
            ui,
            catalog.text(Key::AdoptScanning).to_string(),
            ui.color(Role::OnSurfaceVariant),
        );
    };
    let mut body = column![
        field_line(ui, catalog.text(Key::AdoptInstanceName), &plan.name),
        field_line(
            ui,
            catalog.text(Key::AdoptPlanRunDir),
            &plan.run_dir.display().to_string()
        ),
        field_line(
            ui,
            catalog.text(Key::AdoptPlanIsolation),
            &isolation_label(plan.isolation, catalog),
        ),
    ]
    .spacing(ui.space(Sp::X2))
    .width(Length::Fill);

    // 后果话术：core `target_notes()` 的原文，一条一行、一个字不改
    for note in &plan.notes {
        body = body.push(wrapped(ui, note.clone(), ui.color(Role::OnSurfaceVariant)));
    }
    body.into()
}

/// 失败：稳定码 + **problems 逐条**。
pub(super) fn failed<'a>(
    ui: &'a Ui,
    failure: Option<&'a AdoptFailureView>,
    catalog: Catalog,
) -> Element<'a, Message> {
    let Some(failure) = failure else {
        return wrapped(
            ui,
            catalog.text(Key::AdoptScanning).to_string(),
            ui.color(Role::OnSurfaceVariant),
        );
    };
    let mut body = column![mono(ui, failure.code.clone())]
        .spacing(ui.space(Sp::X2))
        .width(Length::Fill);

    if failure.problems.is_empty() {
        body = body.push(wrapped(
            ui,
            failure.message.clone(),
            ui.color(Role::OnSurfaceVariant),
        ));
        return body.into();
    }
    body = body.push(wrapped(
        ui,
        catalog.text(Key::AdoptProblemCount).to_string(),
        ui.color(Role::OnSurfaceVariant),
    ));
    for problem in &failure.problems {
        body = body.push(problem_line(ui, problem));
    }
    body.into()
}

/// 完成：实例名 + 运行目录 + 提示原文。
pub(super) fn done<'a>(
    ui: &'a Ui,
    done: Option<&'a AdoptDoneView>,
    catalog: Catalog,
) -> Element<'a, Message> {
    let Some(done) = done else {
        return wrapped(ui, String::new(), ui.color(Role::OnSurfaceVariant));
    };
    let mut body = column![
        field_line(ui, catalog.text(Key::AdoptInstanceName), &done.name),
        field_line(
            ui,
            catalog.text(Key::AdoptPlanRunDir),
            &done.run_dir.display().to_string()
        ),
        field_line(
            ui,
            catalog.text(Key::AdoptPlanIsolation),
            &isolation_label(done.isolation, catalog),
        ),
    ]
    .spacing(ui.space(Sp::X2))
    .width(Length::Fill);

    for note in &done.notes {
        body = body.push(wrapped(ui, note.clone(), ui.color(Role::OnSurfaceVariant)));
    }
    body.into()
}
