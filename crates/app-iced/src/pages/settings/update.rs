//! 设置页「更新」分组（M159）：当前版本 / 渠道 / 自动检查开关 / 立即检查 / 上次结果。
//!
//! ## 为什么这一组不走 `Group.body`
//!
//! `Group.body` 的签名是 `fn(&Ui, States) -> Option<Element>`，而 `States` 只有「外观状态 + 各组
//! 自己的状态」两格。更新状态是 **presenter 快照**（不是某一组的字段），与 M106 的语言行同一种
//! 情形：`settings/mod.rs` 按下标（[`UPDATE_GROUP`]）把 `settings::view` 的 `update` 形参交给
//! [`body`]，`GROUP.body` 仍是 `None`。
//!
//! 消息也**不**走 `GroupMessage`：更新动作是「跨页的一次性意图」（检查 / 下载安装 / 忽略 /
//! 重启），落点是 `app/update/update_action.rs`，`pages::Message::Update(UpdateAction)` 一支就够。
//! 偏离已登记在本文件与 mission 报告里（与 M106 的 `Message::Language` 同一取舍：这两个都不是
//! 「某一组的内部状态」，折进 `GroupMessage` 只会多一层没人用的转运）。
//!
//! ## 四条状态行必须分开
//!
//! 「已是最新」「有新版本」「没有本平台产物」「检查失败」是四种不同的结论（mission 硬要求：
//! 失败不许说成最新）。第四种一定要把原因摆出来（连不上 / 清单坏了 / 渠道不符），
//! 否则用户只会看到一句「检查失败」而无从下手。

use iced::widget::{column, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key, Lang};
use unlucky_presenter::dto::human_bytes;

use crate::app::components::buttons::{self, Button};
use crate::app::components::selection::{chip, Chip};

use crate::pages::frame::text_role_owned;
use crate::pages::update::{Update, UpdateStatus};
use crate::pages::Message;
use crate::pages::UpdateAction;
use crate::theme::Ui;

use super::frame::{card, divider};
use super::Group;

/// 「更新」分组在 [`super::GROUPS`] 里的下标（**必须是最后一个**：加组只追加不重排）。
pub(super) const UPDATE_GROUP: usize = 4;

/// 更新分组（左列名固定英文：左列是导航，与导航轨同源，不进 catalog）。
pub(super) const GROUP: Group = Group {
    name: "Updates",
    icon: IconKind::Download,
    // 只给 harness / 窄窗「按组堆叠」路径用的演示行：产品路径走 [`body`]。
    rows: &[
        ("Current version", "0.2.2"),
        ("Channel", "Stable"),
        ("Check for updates on startup", "On"),
    ],
    body: None,
};

/// 渲染整组正文。
pub(super) fn body<'a>(ui: &'a Ui, update: &'a Update, lang: Lang) -> Element<'a, Message> {
    let catalog = lang.catalog();
    let mut stacked = column![].spacing(0.0).width(Length::Fill);

    stacked = stacked.push(info_row(
        ui,
        catalog.text(Key::SettingsUpdateCurrent),
        update.current_version.clone(),
    ));
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(info_row(
        ui,
        catalog.text(Key::SettingsUpdateChannel),
        channel_label(catalog, &update.channel),
    ));
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(auto_check_row(ui, catalog, update));
    stacked = stacked.push(divider(ui));
    stacked = stacked.push(status_block(ui, catalog, update));

    card(ui, stacked)
}

/// 渠道显示名：`stable` 走 key，其它渠道原样上屏（D50 现在只有 stable，但开 beta/alpha 时
/// 页面不该跟着改——渠道是数据，不是写死的字面量）。
fn channel_label(catalog: Catalog, channel: &str) -> String {
    match channel {
        "stable" => catalog.text(Key::SettingsUpdateChannelStable).to_string(),
        other => other.to_string(),
    }
}

/// 「标签 + 值」一行（与 `frame::setting_row` 同一套排版，但值要在调用处现算）。
fn info_row<'a>(ui: &'a Ui, label: &'a str, value: String) -> Element<'a, Message> {
    let height = ui.metrics().control_height + Space::X2.dp();
    row![
        iced::widget::container(text_role_owned(
            ui,
            TypeRole::BodyMedium,
            false,
            label.to_string(),
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill)
        .align_y(Alignment::Center),
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            value,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .height(height)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .into()
}

/// 自动检查开关 + 「立即检查」按钮。
///
/// **忙碌时「立即检查」置灰**（M161）：此前它永远可点，而 presenter 的 `begin_check` 在
/// inflight 时静默 `return None`——用户点了没反应，还以为启动器卡了。禁用态与状态行同源
/// （同一个 [`Update::busy`]）：按钮灰着的时候，下面那句状态行一定是「正在检查…」/「正在安装…」。
fn auto_check_row<'a>(ui: &'a Ui, catalog: Catalog, update: &Update) -> Element<'a, Message> {
    let check_now = catalog.text(Key::SettingsUpdateCheckNow);
    let check = if update.can_check() {
        buttons::text(
            ui,
            Button::new(check_now, Message::Update(UpdateAction::CheckNow)),
        )
    } else {
        buttons::text(ui, Button::disabled(check_now))
    };
    row![
        chip(
            ui,
            Chip::filter(
                catalog.text(Key::SettingsUpdateAutoCheck),
                update.auto_check,
                Message::Update(UpdateAction::SetAutoCheck(!update.auto_check)),
            ),
        ),
        check,
    ]
    .spacing(ui.space(Space::X3))
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .padding(iced::Padding::from([0.0_f32, 0.0]))
    .into()
}

/// 上次检查时间 + 结论 + （有候选时）下载按钮 + 说明摘要 + 一句校验提示。
fn status_block<'a>(ui: &'a Ui, catalog: Catalog, update: &'a Update) -> Element<'a, Message> {
    let mut block = column![].spacing(ui.space(Space::X2)).width(Length::Fill);

    let checked = if update.checked_at.trim().is_empty() {
        catalog.text(Key::SettingsUpdateNever).to_string()
    } else {
        update.checked_at.clone()
    };
    block = block.push(info_row(
        ui,
        catalog.text(Key::SettingsUpdateLastChecked),
        checked,
    ));

    // 下载/校验/替换期间先说这一句：设置页的按钮在这一刻也点不动（见下面 `applying` 分支），
    // 用户得知道「它正在干活」而不是「点了没反应」。
    let (text, error) = status_text(catalog, update);
    let color = ui.color(if error {
        Role::Error
    } else {
        Role::OnSurfaceVariant
    });
    block = block.push(text_role_owned(ui, TypeRole::BodySmall, false, text, color));

    if let Some((_version, _size, notes, forced)) = update.available() {
        // 有候选就**永远画出**这一颗按钮：忙碌时置灰（`can_apply` = false），而不是整颗消失。
        // 消失会让人以为「装到一半的功能没了」；灰着 + 状态行那句「正在安装…」才说得清。
        let apply_label = catalog.text(Key::SettingsUpdateApply);
        let apply = if update.can_apply() {
            buttons::filled(
                ui,
                Button::new(apply_label, Message::Update(UpdateAction::Apply)),
            )
        } else {
            buttons::filled(ui, Button::disabled(apply_label))
        };
        let mut actions = row![apply]
            .spacing(ui.space(Space::X2))
            .align_y(Alignment::Center);
        if forced {
            actions = actions.push(text_role_owned(
                ui,
                TypeRole::BodySmall,
                false,
                catalog.text(Key::SettingsUpdateForced).to_string(),
                ui.color(Role::Error),
            ));
        }
        block = block.push(actions);
        if !notes.trim().is_empty() {
            block = block.push(text_role_owned(
                ui,
                TypeRole::BodySmall,
                false,
                catalog.render(Key::SettingsUpdateNotes, &[("notes", notes.trim())]),
                ui.color(Role::OnSurfaceVariant),
            ));
        }
    }

    if let Some(version) = &update.ready {
        block = block.push(text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.render(Key::SettingsUpdateReady, &[("version", version)]),
            ui.color(Role::OnSurface),
        ));
        block = block.push(buttons::tonal(
            ui,
            Button::new(
                catalog.text(Key::UpdateBannerRestartNow),
                Message::Update(UpdateAction::RestartNow),
            ),
        ));
    }

    block = block.push(text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        catalog.text(Key::SettingsUpdateHint).to_string(),
        ui.color(Role::OnSurfaceVariant),
    ));

    block.into()
}

/// 状态行此刻那句话（**唯一**产地：`applying` 与四态都在这里分派）。
///
/// 拆成函数是为了可断言：`applying` 这一档此前只有截图兜着（复审 P2 附注），
/// `Element` 没有内省接口，而上屏文本只能从**取词的那一个入口**取。
pub(super) fn status_text(catalog: Catalog, update: &Update) -> (String, bool) {
    if update.applying {
        // 「正在下载/校验/替换」不是错误色（它只是在干活）。
        return (catalog.text(Key::SettingsUpdateApplying).to_string(), false);
    }
    status_line(catalog, update)
}

/// 一条状态行（文案 + 是不是「错」那一类）。
///
/// 四种结论各写各的句子，**不共用**「已是最新」那句：失败/缺产物时那句就是撒谎。
/// `pub(super)` 只为让 `settings/tests` 能断言「忙碌时状态行与按钮禁用态同源」。
pub(super) fn status_line(catalog: Catalog, update: &Update) -> (String, bool) {
    let (key, args, error) = match &update.status {
        UpdateStatus::Idle => (Key::SettingsUpdateNever, Vec::new(), false),
        UpdateStatus::Checking => (Key::SettingsUpdateChecking, Vec::new(), false),
        UpdateStatus::UpToDate { latest } => {
            let version = latest
                .clone()
                .unwrap_or_else(|| update.current_version.clone());
            (
                Key::SettingsUpdateUpToDate,
                vec![("version", version)],
                false,
            )
        }
        UpdateStatus::Available { version, size, .. } => (
            Key::SettingsUpdateAvailable,
            vec![("version", version.clone()), ("size", human_bytes(*size))],
            false,
        ),
        UpdateStatus::MissingArtifact { version } => (
            Key::SettingsUpdateMissingArtifact,
            vec![("version", version.clone())],
            true,
        ),
        UpdateStatus::Failed { reason, .. } => (
            Key::SettingsUpdateFailed,
            vec![("reason", reason.clone())],
            true,
        ),
    };

    let args: Vec<(&str, &str)> = args
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    (catalog.render(key, &args), error)
}
