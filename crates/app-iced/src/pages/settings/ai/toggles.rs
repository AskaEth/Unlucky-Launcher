//! 本组的**两个开关行**（M390 从 `view.rs` 纯搬移出来：那边加完 YOLO 行到 449 行，越过
//! `guards.yml` 的 file-size 400 行硬线）。判据与行为一字未改。
//!
//! | 行 | 键 | 什么时候落盘 |
//! |---|---|---|
//! | 启用助手 | `ai.enabled` | 草稿：点「保存」 |
//! | 跳过确认（YOLO） | `ai.yolo` | **点了就落盘**（安全开关，见 [`yolo_row`]） |

use iced::widget::column;
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};

use crate::app::components::selection::{switch, Switch};
use crate::pages::frame::text_role_owned;
use crate::pages::settings::ai::state::State;
use crate::pages::settings::{ai_about, Message};
use crate::theme::Ui;

/// 一条本组消息 → 页面消息（与 `view.rs` 的 `msg` 同一件事；两个文件各一份是为了不把
/// `view::msg` 抬成 `pub(super)`——它只服务那张卡片）。
fn msg(message: ai_about::Message) -> Message {
    Message::Settings(crate::pages::settings::GroupMessage::AiAbout(message))
}

pub(super) fn enable_row<'a>(ui: &'a Ui, catalog: Catalog, state: &State) -> Element<'a, Message> {
    column![
        switch(
            ui,
            Switch::new(
                catalog.text(Key::SettingsAiEnable),
                state.enabled,
                msg(ai_about::Message::ToggleEnabled(!state.enabled)),
            ),
        ),
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::SettingsAiEnableHint).to_string(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}

/// YOLO 开关（M390）：`ai.yolo`，**点了就落盘**，不等「保存」。
///
/// ## 为什么这句话要画在这里，而不只放一个光秃秃的 toggle
///
/// 这个开关会让 AI **不问就开始写**。一个只画轨道和滑块的开关说不出「它不会因此能改更多键」
/// 这件事，用户只会凭直觉赌。所以下面固定两条：①它是什么（只跳过「问」，一拨就生效）；
/// ②开着时红字说清护栏还在（凭据类键仍写不到、审计照记 `yolo=true`）。
///
/// ## 为什么开着才用红字
///
/// 红字在关着的时候也在喊，就会变成谁都不看的那种红字。危险只在「开着」这一态成立，所以
/// `Role::Error` 与加粗都跟着 `state.yolo` 走（文案**两种状态都画**：关着时它解释这个开关）。
///
/// ## 为什么没有设置文件时是禁用态
///
/// 没有可写的 `launcher.json` 就落不了盘，画成一个能拨的开关等于骗人（拨了也不生效）。禁用态
/// 由 `Switch::disabled` 表达，理由那一行由卡片末尾的 `hint_line`（`view.rs`）统一说。
pub(super) fn yolo_row<'a>(ui: &'a Ui, catalog: Catalog, state: &State) -> Element<'a, Message> {
    let label = catalog.text(Key::SettingsAiYolo);
    let toggle = if state.storage {
        switch(
            ui,
            Switch::new(
                label,
                state.yolo,
                msg(ai_about::Message::ToggleYolo(!state.yolo)),
            ),
        )
    } else {
        switch(ui, Switch::disabled(label, state.yolo))
    };
    let danger_color = if state.yolo {
        Role::Error
    } else {
        Role::OnSurfaceVariant
    };
    column![
        toggle,
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::SettingsAiYoloHint).to_string(),
            ui.color(Role::OnSurfaceVariant),
        ),
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            state.yolo,
            catalog.text(Key::SettingsAiYoloDanger).to_string(),
            ui.color(danger_color),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}
