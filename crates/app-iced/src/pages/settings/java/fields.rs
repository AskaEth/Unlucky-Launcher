//! 「Java」分组的**编辑区**（M175）：手动路径 / 默认内存 / JVM 参数 / 保存与回执。
//!
//! 从 `view.rs` 拆出来的（那个文件加了守卫之后贴到 438 行，越过 400 行硬线）：这里全是
//! 「用户能改的东西」，`view.rs` 只剩「扫到了什么」与版面。两半共用 `view::msg` 这一个消息口，
//! 所以「一条控件对应哪条消息」不会各写一遍。

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::app::components::inputs::{text_field, Field, FieldVariant};
use crate::pages::frame::{text_role_owned, BASE};
use crate::pages::Message;
use crate::theme::Ui;

use super::state::{Layer, Notice, Refusal, State, MAX_MEMORY_MB, MIN_MEMORY_MB};
use super::view::msg;
use super::Message as JavaMessage;

/// 手动添加一行：输入框 + 「添加」按钮 + 原因。
///
/// M237：这一行住在 `frame::card` 里，输入框与按钮都把底传成 `frame::BASE`（禁用/状态层叠卡底）。
pub(super) fn add_row<'a>(ui: &'a Ui, catalog: Catalog, state: &'a State) -> Element<'a, Message> {
    let field = Field::new(&state.add_draft)
        .variant(FieldVariant::Outlined)
        .label(catalog.text(Key::SettingsJavaAdd))
        .placeholder(catalog.text(Key::SettingsJavaAddPlaceholder))
        .supporting(catalog.text(Key::SettingsJavaAddHint))
        .base(BASE);
    let add = if state.add_draft.trim().is_empty() || !state.storage {
        buttons::outlined(
            ui,
            Button::disabled(catalog.text(Key::SettingsJavaAddAction)).base(BASE),
        )
    } else {
        buttons::outlined(
            ui,
            Button::new(
                catalog.text(Key::SettingsJavaAddAction),
                msg(JavaMessage::Add),
            )
            .base(BASE),
        )
    };
    row![
        container(text_field(ui, field, |value| msg(JavaMessage::AddDraft(
            value
        ))))
        .width(Length::Fill),
        add,
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .into()
}

/// 默认内存：数字框 + 范围 + 「生效值来自哪一层」。
pub(super) fn memory_field<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    state: &'a State,
) -> Element<'a, Message> {
    let range = catalog.render(
        Key::SettingsJavaMemoryHint,
        &[
            ("min", MIN_MEMORY_MB.to_string().as_str()),
            ("max", MAX_MEMORY_MB.to_string().as_str()),
        ],
    );
    let field = Field::new(&state.memory)
        .variant(FieldVariant::Outlined)
        .label(catalog.text(Key::SettingsJavaMemory))
        .base(BASE);
    // 校验不过就把输入框本身标红（不只是下面那行字）：用户一眼看到「是这个框有问题」。
    let field = match state.validate() {
        Err(Refusal::MemoryEmpty | Refusal::MemoryNotNumber | Refusal::MemoryRange) => {
            field.error()
        }
        _ => field,
    };
    let effective = match state.layer() {
        Layer::Global => catalog.render(
            Key::SettingsJavaLayerGlobal,
            &[("mb", state.saved_memory.to_string().as_str())],
        ),
        Layer::Instance { instance, mb } => catalog.render(
            Key::SettingsJavaLayerInstance,
            &[
                ("mb", mb.to_string().as_str()),
                ("instance", instance.as_str()),
            ],
        ),
    };
    column![
        text_field(ui, field, |value| msg(JavaMessage::EditMemory(value))),
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            effective,
            ui.color(Role::OnSurfaceVariant),
        ),
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            range,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}

/// JVM 额外参数：文本框 + 「会拼到每次启动」的说明。
pub(super) fn jvm_field<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    state: &'a State,
) -> Element<'a, Message> {
    let field = Field::new(&state.jvm)
        .variant(FieldVariant::Outlined)
        .label(catalog.text(Key::SettingsJavaJvm))
        .placeholder(catalog.text(Key::SettingsJavaJvmPlaceholder))
        .supporting(catalog.text(Key::SettingsJavaJvmHint))
        .base(BASE);
    // 两种拒绝都长在这个框上：引号没配对、或者粘了超过 1024 字符的一整段（M199）。
    let field = if matches!(
        state.validate(),
        Err(Refusal::JvmQuotes | Refusal::JvmTooLong)
    ) {
        field.error()
    } else {
        field
    };
    // 已保存值：空的时候说「还没有保存任何额外参数」，而不是让「已保存的值：」后面空着。
    let saved = if state.saved_jvm.trim().is_empty() {
        catalog.text(Key::SettingsJavaJvmSavedNone).to_string()
    } else {
        catalog.render(
            Key::SettingsJavaJvmSaved,
            &[("value", state.saved_jvm.as_str())],
        )
    };
    column![
        text_field(ui, field, |value| msg(JavaMessage::EditJvm(value))),
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            saved,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}

pub(super) fn actions_row<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    state: &'a State,
) -> Element<'a, Message> {
    let label = catalog.text(Key::SettingsJavaSave);
    if state.can_save() {
        buttons::filled(ui, Button::new(label, msg(JavaMessage::Save)).base(BASE))
    } else {
        buttons::filled(ui, Button::disabled(label).base(BASE))
    }
}

/// 按钮为什么灰着 / 下一步做什么——**永远说清**。
pub(super) fn hint_line<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    state: &'a State,
) -> Element<'a, Message> {
    let (text, error) = if !state.storage {
        (catalog.text(Key::SettingsJavaNoStorage).to_string(), true)
    } else if state.dirty {
        (catalog.text(Key::SettingsJavaUnsaved).to_string(), false)
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

pub(super) fn notice_line<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    notice: &Notice,
) -> Element<'a, Message> {
    let (text, error) = match notice {
        Notice::Saved => (catalog.text(Key::SettingsJavaSaved).to_string(), false),
        Notice::Refused(reason) => (
            catalog.render(
                Key::SettingsJavaRefused,
                &[("reason", catalog.text(refusal_key(*reason)))],
            ),
            true,
        ),
        Notice::Unwritable(reason) => (
            catalog.render(
                Key::SettingsJavaRefused,
                &[("reason", catalog.text(reason.label()))],
            ),
            true,
        ),
        Notice::WriteFailed => (
            catalog.render(
                Key::SettingsJavaRefused,
                &[("reason", catalog.text(Key::SettingsJavaReasonWriteFailed))],
            ),
            true,
        ),
        Notice::NoStorage => (catalog.text(Key::SettingsJavaNoStorage).to_string(), true),
        Notice::DefaultSet(path) => (
            catalog.render(Key::SettingsJavaDefaultSet, &[("path", path.as_str())]),
            false,
        ),
        Notice::Added(path) => (
            catalog.render(Key::SettingsJavaAdded, &[("path", path.as_str())]),
            false,
        ),
        Notice::PrefsSaved => (catalog.text(Key::SettingsJavaPrefsSaved).to_string(), false),
    };
    text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        text,
        ui.color(if error { Role::Error } else { Role::OnSurface }),
    )
}

/// 拒绝原因 → 文案 key（`Refusal::label` 已是这张表，这里只是给 `match` 一个名字）。
pub(super) fn refusal_key(refusal: Refusal) -> Key {
    refusal.label()
}
