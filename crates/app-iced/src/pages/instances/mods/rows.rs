//! MODS 区的**画法**（M312 从 `mods.rs` 原样搬出）。
//!
//! 为什么搬：`mods.rs` 加完「重复 / 前置」的视图模型会越过 400 行硬线（与 `saves.rs` →
//! `saves/rows.rs` 同一条先例）。**纯搬移**：四个函数的签名、注释、字面量一字未动，
//! 判据与文案仍留在 `mods.rs`（`Key::Instances*` 只在那边出现，见 `tests/i18n.rs` 的死 key
//! 门禁）。参加者是**目录派生**的 `pages/instances/**` 全族（非测试文件，见
//! `../tests/i18n/sources.rs`），本文件在族里自动参加——搬过来不用去任何清单登记。
//!
//! 三档开关的语义、为什么不做乐观回显、为什么不再套一层滚动区，全在 `mods.rs` 的模块文档里。

use iced::widget::{column, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_presenter::router::Route;

use crate::app::components::buttons::{self, Button};
use crate::app::components::interaction::Interaction;
use crate::app::components::selection::{switch, Switch};
use crate::theme::Ui;

use super::super::forced_state;
use super::super::frame;
use super::super::state::Message as PageMessage;
use super::super::Message;
use super::super::Texts;
use super::{Item, Mods, Problem};

/// 画 MODS 区（实例页唯一入口，`detail.rs` 调它）。
pub(in crate::pages::instances) fn section<'a>(
    ui: &'a Ui,
    mods: &'a Mods,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    let mut body = column![head(ui, mods, texts)]
        .spacing(ui.space(Space::X3))
        .width(Length::Fill);

    if mods.loading {
        body = body.push(note(ui, texts.mods_loading()));
    } else if let Some(error) = mods.error.as_deref() {
        body = body.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            error,
            ui.color(Role::Error),
        ));
    } else if !mods.exists {
        body = body.push(note(ui, texts.mods_missing_dir()));
    } else {
        // 问题区在列表**上面**：用户先看到"哪里不对"，再去逐条动开关。
        if !mods.problems.is_empty() {
            body = body.push(problems(ui, mods, forced));
        }
        if mods.items.is_empty() {
            body = body.push(note(ui, texts.mods_empty()));
        } else {
            // 只读原因放在列表**上面**：用户先看到"为什么点不动"，再去点。
            if mods.running {
                body = body.push(note(ui, texts.mods_running()));
            }
            for item in &mods.items {
                body = body.push(entry_row(ui, item, mods.running, forced));
            }
            if mods.skipped > 0 {
                body = body.push(note(ui, &mods.skipped_note));
            }
        }
    }

    frame::card(ui, body)
}

/// 问题区（M312）：重复一族一段、必需缺失一段、提示整句若干。
///
/// **只读 + 一个导航入口**：重复项的处置仍然是列表里那一行的启停开关（不在这里再发一条
/// `SetEnabled`——两个入口改同一个文件只会让「在飞」判定更难写）。这里唯一的按钮是
/// 「去模组库找」，落点是 `Route::Content`（装 mod 的地方）。
fn problems<'a>(ui: &'a Ui, mods: &'a Mods, forced: Option<Interaction>) -> Element<'a, Message> {
    let problems = &mods.problems;
    let mut block = column![].spacing(ui.space(Space::X1)).width(Length::Fill);

    if !problems.duplicates.is_empty() {
        block = block.push(small(ui, &problems.duplicate_heading, Role::Tertiary));
        for problem in &problems.duplicates {
            block = block.push(problem_line(ui, problem));
        }
    }
    if !problems.missing.is_empty() {
        block = block.push(small(ui, &problems.missing_heading, Role::Tertiary));
        for problem in &problems.missing {
            block = block.push(problem_line(ui, problem));
        }
        block = block.push(buttons::text(
            ui,
            forced_state(
                Button::new(mods.missing_cta.as_str(), Message::Navigate(Route::Content))
                    .base(frame::BASE),
                forced,
            ),
        ));
    }
    for note in &problems.notes {
        block = block.push(small(ui, note, Role::OnSurfaceVariant));
    }

    column![]
        .spacing(ui.space(Space::X1))
        .width(Length::Fill)
        .push(block)
        .into()
}

/// 问题区的一行：主句 + 副句（副句空着就不画）。
fn problem_line<'a>(ui: &'a Ui, problem: &'a Problem) -> Element<'a, Message> {
    let mut line = column![small(ui, problem.text.as_str(), Role::OnSurface)]
        .spacing(ui.space(Space::X1))
        .width(Length::Fill);
    if !problem.detail.is_empty() {
        line = line.push(small(ui, problem.detail.as_str(), Role::OnSurfaceVariant));
    }
    line.into()
}

/// 问题区的一句小字（副标题性质）。
fn small<'a>(ui: &'a Ui, text: &'a str, role: Role) -> Element<'a, Message> {
    frame::text_role(ui, TypeRole::BodySmall, false, text, ui.color(role))
}

/// 小标行：`MODS` + 计数（`12 启用 · 3 禁用`）。
fn head<'a>(ui: &'a Ui, mods: &'a Mods, texts: Texts) -> Element<'a, Message> {
    let mut line = row![frame::text_role(
        ui,
        TypeRole::LabelMedium,
        false,
        texts.mods_title(),
        ui.color(Role::OnSurfaceVariant),
    )]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center)
    .width(Length::Fill);

    // 计数只在"真有一份可用快照"时画：读盘中 / 读失败 / 空目录画 0·0 会像"这里什么都没有"。
    let show_count = mods.loaded() && mods.error.is_none() && !mods.items.is_empty();
    if show_count {
        line = line.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            mods.count.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    // 问题徽标（M312）：只在真有问题时出现，位置在小标右侧、计数之后。
    if let Some(badge) = mods.problem_badge.as_deref() {
        line = line.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            badge,
            ui.color(Role::Tertiary),
        ));
    }
    line.into()
}

/// 一条 mod 的行：名字 + 副文案 + 开关。
fn entry_row<'a>(
    ui: &'a Ui,
    item: &'a Item,
    running: bool,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let label = column![
        frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            item.name.as_str(),
            ui.color(if item.enabled {
                Role::OnSurface
            } else {
                Role::OnSurfaceVariant
            }),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            item.detail.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    let toggle = if running || item.pending {
        Switch::disabled("", item.enabled)
    } else {
        Switch::new(
            "",
            item.enabled,
            Message::Instances(PageMessage::SetModEnabled {
                file: item.file.clone(),
                enabled: !item.enabled,
            }),
        )
    };
    // M237：开关住在 `frame::card` 里，禁用态的 12% 轨道 / 38% 滑块要叠**卡底**（`frame::BASE`），
    // 不是画布底——否则「游戏在跑 / 请求在飞」那两档的开关会与卡片底色脱节。
    let toggle = toggle.base(frame::BASE);
    // 出图取证（`--force-state`）把开关也钉到同一档：否则截图里只有按钮是 hover、
    // 开关还是静息态，视觉复审没法判"悬停时的开关长什么样"。
    let toggle = match forced {
        Some(state) => toggle.state(state),
        None => toggle,
    };
    row![label, switch(ui, toggle)]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

fn note<'a>(ui: &'a Ui, text: &'a str) -> Element<'a, Message> {
    frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        text,
        ui.color(Role::OnSurfaceVariant),
    )
}
