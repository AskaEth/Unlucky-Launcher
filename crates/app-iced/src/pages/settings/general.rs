//! 设置页「General」分组：语言选择器 + 主题 / 启动页 / 关闭按钮（M106 接真控件）。
//!
//! **这一组归 language+appearance 线**（M101 的分线口径）。
//!
//! ## 这一组为什么特殊
//!
//! 它**不走** `Group::body`：那个渲染器的签名是 `fn(&Ui, States) -> Option<Element>`，`States` 只有
//! `appearance` + `groups` 两格，装不下「当前语言档位 + 生效语言」。语言行要的两件在
//! `pages::Language` 里，由 `pages::settings::view` 的 `language` 形参一路递进来、`form` 按下标
//! 转交 [`body`]；所以 `GROUP.body` 仍是 `None`，`tests/mod.rs` 那条「只有外观组带真控件」照样成立。
//!
//! ## 语言切换**没有**走 M101 的 `GroupMessage` 投递口（偏离，已登记）
//!
//! `settings/mod.rs` 的文档说本线要加消息就加到 `settings/language_appearance.rs` 的 `Message` 上。
//! M106 走的是 `pages::Message::Language(choice)` 这条独立增量，理由：语言档位不是「某一组的状态」，
//! 它要**落盘**（`ui.language`）+ **重建整棵视图模型**（目录是 `pages::Model` 的一部分），而落点
//! `app/update/settings/language_appearance.rs` 与消息口 `language_appearance.rs` 都不在本 mission 的
//! scope 内。代价：`page_messages.rs` 多一支（那是 M101 想收掉的共享锚点）。**收口方向**：等本线
//! 再动设置页时，把 `Message::Language` 折进 `GroupMessage::LanguageAppearance(SetChoice)`，并把
//! `pages/mod.rs` 的这支删掉——finding 已登记。
//!
//! ## 语言行的三个事实
//!
//! 1. **选中态显示的是档位**（跟随系统 / 简体中文 / 英文），不是「猜出来的语言」——用户点过什么
//!    就得画回什么，否则「我明明选了跟随系统」这件事在界面上无从确认；
//! 2. **旁边回显生效语言**（`界面当前使用：简体中文`）：中文系统 + 跟随系统时，这一行是「界面
//!    真的用了中文」的唯一可见证据（用户报的 bug 正是「识别到了但没用上」）；
//! 3. **文案全部走 catalog**（[`Key`]），三档选项名跟着界面语言走（见 `catalog/general.rs`
//!    的头注释：为什么不写语言自称）。
//!
//! ## 其余三行仍只读
//!
//! 主题 / 启动页 / 关闭按钮还没有后端（`pages::Action` 里是占位动作），所以它们是**只读行**；
//! 但标签与取值已经进 catalog，中文界面下不会再冒出英文。
//!
//! 分组元数据只有一份（这里的 [`GROUP`]）：`settings/mod.rs` 的 `GROUPS` 直接引用它。

use iced::widget::{button, column, container, row};
use iced::{Alignment, Background, Border, Element, Length, Padding, Shadow, Theme as IcedTheme};

use unlucky_design::color::{Role, StateLayer};
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::catalog::Key;
use unlucky_i18n::LangChoice;

use crate::pages::{frame, Language, Message};
use crate::theme::Ui;

use super::frame::{card, divider};
use super::Group;

/// General 分组（左列名固定英文：左列是**导航**，与导航轨同源，不进 catalog）。
pub(super) const GROUP: Group = Group {
    name: "General",
    icon: IconKind::Settings,
    // 只给 harness（`pages_demo` 走 `appearance: None` 时）用的演示行：产品路径走 [`body`]。
    // 语言那一行**不在**这里——它的值是要跟着语言变的，写死在静态表里就是用户看到的那条假值。
    rows: &[
        ("Theme", "Follow system"),
        ("Startup page", "Instances"),
        ("Close button", "Exit to tray"),
    ],
    body: None,
};

/// 这一组的正文（**产品路径**）。
pub(super) fn body<'a>(ui: &'a Ui, language: Language) -> Element<'a, Message> {
    let catalog = language.effective.catalog();
    let mut stacked = column![].spacing(0.0).width(Length::Fill);
    stacked = stacked.push(language_row(ui, language));

    // 主题 / 启动页 / 关闭按钮：还没有后端，只读；文案已经进 catalog。
    for (label, value) in [
        (Key::SettingsTheme, Key::SettingsThemeFollowSystem),
        (Key::SettingsStartupPage, Key::SettingsStartupInstances),
        (Key::SettingsCloseButton, Key::SettingsCloseToTray),
    ] {
        stacked = stacked.push(divider(ui));
        stacked = stacked.push(setting_row(ui, catalog.text(label), catalog.text(value)));
    }

    card(ui, stacked)
}

/// 语言行：标题 + 生效语言回显 + 三档互斥按钮 + 一句说明。
fn language_row<'a>(ui: &'a Ui, language: Language) -> Element<'a, Message> {
    let catalog = language.effective.catalog();
    let effective = LangChoice::Fixed(language.effective).label_key();

    let head = row![
        container(frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            catalog.text(Key::SettingsLanguage),
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill)
        .align_y(Alignment::Center),
        frame::text_role_owned(
            ui,
            TypeRole::LabelLarge,
            true,
            catalog.render(
                Key::SettingsLanguageEffective,
                &[("language", catalog.text(effective))],
            ),
            ui.color(Role::OnPrimaryFixedVariant),
        ),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center);

    let chips = row(LangChoice::ALL
        .iter()
        .map(|choice| {
            chip(
                ui,
                catalog.text(choice.label_key()),
                *choice == language.choice,
                *choice,
            )
        })
        .collect::<Vec<_>>())
    .spacing(ui.space(Space::X1))
    .align_y(Alignment::Center);

    column![
        head,
        chips,
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::SettingsLanguageHint),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .padding(Padding::from([ui.space(Space::X2), 0.0]))
    .into()
}

/// 一个只读的「标签 + 取值」行（与 `frame::demo_card` 的一行同高同排版）。
fn setting_row<'a>(ui: &'a Ui, label: &'a str, value: &'a str) -> Element<'a, Message> {
    let height = ui.metrics().control_height + Space::X2.dp();

    row![
        container(frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            label,
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill)
        .align_y(Alignment::Center),
        frame::text_role(
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

/// 一个互斥按钮（选中态 `SecondaryContainer`，未选中透明 + `OnSurfaceVariant`）。
///
/// 与 `pages/appearance/section.rs` 的 `choice` 同款，但文案来自 catalog（那份是英文基线）。
fn chip<'a>(ui: &'a Ui, text: &'a str, selected: bool, choice: LangChoice) -> Element<'a, Message> {
    let height = ui.metrics().control_height;
    let content_color = if selected {
        ui.color(Role::OnSecondaryContainer)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    button(
        container(frame::text_role(
            ui,
            TypeRole::LabelLarge,
            selected,
            text,
            content_color,
        ))
        .align_y(Alignment::Center),
    )
    .height(height)
    .padding(Padding::from([0.0, ui.space(Space::X3)]))
    .style(move |_theme: &IcedTheme, status| {
        let background = if selected {
            Some(Background::Color(ui.color(Role::SecondaryContainer)))
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Some(Background::Color(ui.state_over(
                StateLayer::Hover,
                Role::OnSurface,
                Role::Surface,
            )))
        } else {
            None
        };

        button::Style {
            background,
            text_color: content_color,
            border: Border {
                radius: ui.radius(ShapeScale::Full, height),
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }
    })
    .on_press(Message::Language(choice))
    .into()
}
