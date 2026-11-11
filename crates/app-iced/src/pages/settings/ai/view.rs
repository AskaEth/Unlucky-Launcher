//! 「助手」分组的一张卡片（M174）。
//!
//! ## 版面（D30 的状态矩阵 + 数值）
//!
//! | 行 | 内容 | 数值 |
//! |---|---|---|
//! | 状态 | 未配置 / 已配置 / 连接已通过 / 测试失败 | 生效端点 + 模型名 |
//! | 开关 | 启用助手（`ai.enabled`） | — |
//! | 提供商 | 四态 filter chip（`ai.provider`） | 存的值 |
//! | 端点 | `base_url` 文本框 | 生效值（空 = 该形态默认） |
//! | 模型 | `ai.model` 文本框 | — |
//! | 密钥 | 密码框（默认掩码）+「显示」 | **只显示字符数**，不显示值 |
//! | 动作 | 保存 / 测试连接 | 测试结论带耗时（毫秒） |
//!
//! 「未配置」时「测试连接」是**禁用**的，并在下面直接说清缺哪一项——不是灰着什么都不说。

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key, Lang};

use crate::app::components::buttons::{self, Button};
use crate::app::components::inputs::{text_field, Field, FieldVariant};
use crate::app::components::selection::{chip, switch, Chip, Switch};
use crate::pages::frame::{card, divider, text_role_owned};
use crate::pages::settings::ai::outcome::ProbeKind;
use crate::pages::settings::ai::state::{Probe, ProviderId, SaveNotice, Secret, State, Status};
use crate::pages::settings::{ai_about, GroupMessage, States};
use crate::pages::Message;
use crate::theme::Ui;

/// 一条本组消息 → 页面消息（左列/正文都用这一处，别在下面各写一遍）。
fn msg(message: ai_about::Message) -> Message {
    Message::Settings(GroupMessage::AiAbout(message))
}

/// 一张卡片（`pages::settings::form` 按下标把 `Lang` 递进来，见 `settings/mod.rs`）。
pub(crate) fn body<'a>(ui: &'a Ui, states: States<'a>, lang: Lang) -> Element<'a, Message> {
    body_of(ui, &states.groups.ai_about, lang)
}

fn body_of<'a>(ui: &'a Ui, state: &'a State, lang: Lang) -> Element<'a, Message> {
    let catalog = lang.catalog();
    let mut stacked = column![].spacing(ui.space(Space::X2)).width(Length::Fill);

    stacked = stacked.push(status_row(ui, catalog, state));
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(enable_row(ui, catalog, state));
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(provider_row(ui, catalog, state));
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(endpoint_field(ui, catalog, state));
    stacked = stacked.push(model_field(ui, catalog, state));
    stacked = stacked.push(key_field(ui, catalog, state));
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(actions_row(ui, catalog, state));
    stacked = stacked.push(hint_line(ui, catalog, state));

    if let Some(notice) = &state.notice {
        stacked = stacked.push(notice_line(ui, catalog, notice));
    }
    if let Probe::Done(result) = &state.probe {
        stacked = stacked.push(probe_line(ui, catalog, result.kind));
        // 目标行只在真有值时才画（r1 复审 P2-1 的第二道保险）：失败类现在**都会**带上 target，
        // 但「没发请求」与将来的新档可能没有；空着画出来就是「目标：  · 模型  · N 毫秒」。
        if !(result.base_url.is_empty() && result.model.is_empty()) {
            stacked = stacked.push(text_role_owned(
                ui,
                TypeRole::BodySmall,
                false,
                catalog.render(
                    Key::SettingsAiTestTarget,
                    &[
                        ("base_url", result.base_url.as_str()),
                        ("model", result.model.as_str()),
                        ("ms", result.elapsed_ms.to_string().as_str()),
                    ],
                ),
                ui.color(Role::OnSurfaceVariant),
            ));
        }
        if !result.detail.trim().is_empty() {
            stacked = stacked.push(text_role_owned(
                ui,
                TypeRole::BodySmall,
                false,
                catalog.render(
                    Key::SettingsAiTestDetail,
                    &[("detail", result.detail.as_str())],
                ),
                ui.color(Role::OnSurfaceVariant),
            ));
        }
    }

    card(ui, stacked)
}

/// 状态行：四档文案 + 生效端点 / 模型（数值直接上屏，不藏在提示里）。
fn status_row<'a>(ui: &'a Ui, catalog: Catalog, state: &State) -> Element<'a, Message> {
    let status = state.status();
    let color = match status {
        Status::Failed => ui.color(Role::Error),
        Status::Verified => ui.color(Role::Primary),
        _ => ui.color(Role::OnSurfaceVariant),
    };
    let target = if state.model.trim().is_empty() {
        state.resolved_base_url()
    } else {
        format!("{} · {}", state.resolved_base_url(), state.model.trim())
    };
    let line = catalog.render(
        Key::SettingsAiStatusLabel,
        &[("status", catalog.text(status.label()))],
    );
    row![
        container(text_role_owned(ui, TypeRole::BodyMedium, true, line, color)).width(Length::Fill),
        text_role_owned(ui, TypeRole::BodySmall, false, target, color),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .into()
}

fn enable_row<'a>(ui: &'a Ui, catalog: Catalog, state: &State) -> Element<'a, Message> {
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

fn provider_row<'a>(ui: &'a Ui, catalog: Catalog, state: &State) -> Element<'a, Message> {
    // `.wrap()`：四颗 chip 在 424 宽的正文列里放不下一行时**整颗换行**，而不是把最后一颗
    // 压成竖排（「已禁 / 用」那种断字）。`Wrapping` 只提供 `push`，所以先建后 wrap。
    let mut chips = row![]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center);
    for provider in ProviderId::ALL {
        chips = chips.push(chip(
            ui,
            Chip::filter(
                catalog.text(provider.label()),
                state.provider == provider,
                msg(ai_about::Message::PickProvider(provider)),
            ),
        ));
    }
    row![
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::SettingsAiProviderLabel).to_string(),
            ui.color(Role::OnSurfaceVariant),
        ),
        chips.wrap(),
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .into()
}

fn endpoint_field<'a>(ui: &'a Ui, catalog: Catalog, state: &'a State) -> Element<'a, Message> {
    let field = Field::new(&state.base_url)
        .variant(FieldVariant::Outlined)
        .label(catalog.text(Key::SettingsAiBaseUrl))
        .placeholder(catalog.text(Key::SettingsAiBaseUrlPlaceholder))
        .supporting(catalog.text(Key::SettingsAiBaseUrlHint));
    // 「生效：…」只在**回落默认值**（草稿为空、该形态有默认端点）时才值得说一句：
    // 草稿就是生效值时它是废话，空着且没有默认值时它是空串（比不说更糟）。
    let effective = state.resolved_base_url();
    let mut block = column![text_field(ui, field, |value| msg(
        ai_about::Message::EditBaseUrl(value)
    ))]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);
    if !effective.is_empty() && effective != state.base_url.trim() {
        block = block.push(text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.render(Key::SettingsAiEffective, &[("value", effective.as_str())]),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    block.into()
}

fn model_field<'a>(ui: &'a Ui, catalog: Catalog, state: &'a State) -> Element<'a, Message> {
    let field = Field::new(&state.model)
        .variant(FieldVariant::Outlined)
        .label(catalog.text(Key::SettingsAiModel))
        .placeholder(catalog.text(Key::SettingsAiModelPlaceholder));
    text_field(ui, field, |value| msg(ai_about::Message::EditModel(value)))
}

/// 密钥行：密码框 + 「显示 / 隐藏」chip + 已存状态（**只有字符数**）。
fn key_field<'a>(ui: &'a Ui, catalog: Catalog, state: &'a State) -> Element<'a, Message> {
    let chars = state.key_chars.to_string();
    let supporting = if state.key_chars > 0 {
        catalog.render(
            Key::SettingsAiApiKeyConfigured,
            &[("chars", chars.as_str())],
        )
    } else {
        catalog.text(Key::SettingsAiApiKeyMissing).to_string()
    };
    let field = Field::new(state.key.expose())
        .variant(FieldVariant::Outlined)
        .label(catalog.text(Key::SettingsAiApiKey))
        .placeholder(catalog.text(Key::SettingsAiApiKeyPlaceholder))
        .secure(!state.reveal);
    let toggle_label = if state.reveal {
        catalog.text(Key::SettingsAiHide)
    } else {
        catalog.text(Key::SettingsAiShow)
    };
    let toggle = chip(
        ui,
        Chip::filter(
            toggle_label,
            state.reveal,
            msg(ai_about::Message::Reveal(!state.reveal)),
        ),
    );
    let field = container(text_field(ui, field, |value| {
        msg(ai_about::Message::EditKey(Secret::new(value)))
    }))
    .width(Length::Fill);
    column![
        row![field, toggle]
            .spacing(ui.space(Space::X2))
            .width(Length::Fill)
            .align_y(Alignment::Center),
        // 「已存 N 字符 / 还没有密钥」这一行是**格式化出来的**（`Field::supporting` 借的是
        // `&'a str`，临时 String 借不进去），所以单独一行；它只讲长度，不讲值。
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            supporting,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}

fn actions_row<'a>(ui: &'a Ui, catalog: Catalog, state: &State) -> Element<'a, Message> {
    let save_label = catalog.text(Key::SettingsAiSave);
    let save = if state.dirty && state.storage {
        buttons::filled(ui, Button::new(save_label, msg(ai_about::Message::Save)))
    } else {
        buttons::filled(ui, Button::disabled(save_label))
    };

    let test_label = if matches!(state.probe, Probe::Running) {
        catalog.text(Key::SettingsAiTesting)
    } else {
        catalog.text(Key::SettingsAiTest)
    };
    let test = if state.can_test() {
        buttons::tonal(ui, Button::new(test_label, msg(ai_about::Message::Test)))
    } else {
        buttons::tonal(ui, Button::disabled(test_label))
    };

    row![save, test]
        .spacing(ui.space(Space::X3))
        .align_y(Alignment::Center)
        .into()
}

/// 按钮为什么是灰的 / 下一步该做什么——**永远说清**，不留「点了没反应」。
fn hint_line<'a>(ui: &'a Ui, catalog: Catalog, state: &State) -> Element<'a, Message> {
    let (text, error) = if !state.storage {
        (catalog.text(Key::SettingsAiNoStorage).to_string(), true)
    } else if let Some(missing) = state.missing() {
        (
            catalog.render(
                Key::SettingsAiTestDisabled,
                &[("missing", catalog.text(missing.label()))],
            ),
            false,
        )
    } else if state.dirty {
        (catalog.text(Key::SettingsAiUnsaved).to_string(), false)
    } else {
        (String::new(), false)
    };
    text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        text,
        ui.color(if error {
            Role::Error
        } else {
            Role::OnSurfaceVariant
        }),
    )
}

fn notice_line<'a>(ui: &'a Ui, catalog: Catalog, notice: &SaveNotice) -> Element<'a, Message> {
    let (text, error) = match notice {
        SaveNotice::Saved => (catalog.text(Key::SettingsAiSaved).to_string(), false),
        SaveNotice::Refused(reason) => (
            catalog.render(
                Key::SettingsAiRefused,
                &[("reason", catalog.text(reason.label()))],
            ),
            true,
        ),
        SaveNotice::Failed(reason) => (
            catalog.render(Key::SettingsAiRefused, &[("reason", reason.as_str())]),
            true,
        ),
        SaveNotice::NoStorage => (catalog.text(Key::SettingsAiNoStorage).to_string(), true),
    };
    text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        text,
        ui.color(if error { Role::Error } else { Role::OnSurface }),
    )
}

/// 测试结论一行：分类一句话（目标/耗时/详情在它下面三行）。
fn probe_line<'a>(ui: &'a Ui, catalog: Catalog, kind: ProbeKind) -> Element<'a, Message> {
    let error = kind.is_failure();
    text_role_owned(
        ui,
        TypeRole::BodyMedium,
        true,
        catalog.text(kind.label()).to_string(),
        ui.color(if error { Role::Error } else { Role::Primary }),
    )
}
