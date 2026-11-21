//! 三个设置键的**编辑区**（M197）：`java.installRoot` / `java.vendorPriority` / `java.auto_install`。
//!
//! 每一段都是同一种形状：**能改的控件 + 现在生效什么 + 这个生效值从哪一层来**。第三件事是任务书
//! 点名的（「两者都要能解释『生效值现在是什么、从哪一层来』」），所以三段的最后一行永远不是空的：
//!
//! | 段 | 控件 | 生效值从哪来 |
//! |---|---|---|
//! | 安装目录 | 文本框（可以留空） | `java.installRoot` 或 `core.data_mode` |
//! | 发行版顺序 | 4 行 + 上移 / 下移 + 恢复默认 | `java.vendorPriority` 或地区默认序 |
//! | 缺 JDK 时自动准备 | 开 / 关两片 | 设置本身（出厂开） |
//!
//! 文案一律走 [`Catalog`]（本组要 `Lang`，走 `settings/mod.rs` 的按下标分发）；渲染器自己不判断
//! 层级，`RootLayer` / `OrderLayer` 是 app 层算好的结论。

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::app::components::inputs::{text_field, Field, FieldVariant};
use crate::app::components::selection::{chip, Chip};
use crate::pages::frame::{divider, text_role_owned, BASE};
use crate::pages::Message;
use crate::theme::Ui;

use super::prefs::{Effective, OrderLayer, RootLayer};
use super::state::State;
use super::view::msg;
use super::Message as JavaMessage;

/// 三段 + 保存按钮（整块的标题由设置页的外壳给，这里不再画）。
pub(super) fn section<'a>(ui: &'a Ui, catalog: Catalog, state: &'a State) -> Element<'a, Message> {
    let mut block = column![].spacing(ui.space(Space::X2)).width(Length::Fill);

    block = block.push(install_root(ui, catalog, state));
    block = block.push(divider(ui));
    block = block.push(vendor_order(ui, catalog, state));
    block = block.push(divider(ui));
    block = block.push(auto_install(ui, catalog, state));
    block = block.push(actions(ui, catalog, state));
    block.into()
}

/// `java.installRoot`：文本框 + 生效落点 + 来源层。
fn install_root<'a>(ui: &'a Ui, catalog: Catalog, state: &'a State) -> Element<'a, Message> {
    let prefs = &state.prefs;
    let field = Field::new(&prefs.install_root)
        .variant(FieldVariant::Outlined)
        .label(catalog.text(Key::SettingsJavaInstallRoot))
        .placeholder(catalog.text(Key::SettingsJavaInstallRootPlaceholder))
        .supporting(catalog.text(Key::SettingsJavaInstallRootHint))
        .base(BASE);
    let field = if prefs.validate().is_err() {
        field.error()
    } else {
        field
    };
    let effective = effective_line(ui, catalog, &state.effective);
    column![
        text_field(ui, field, |value| msg(JavaMessage::EditInstallRoot(value))),
        effective,
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}

/// 「生效：模式 · 路径」+「从哪一层来」（解析不出标准目录时说清下一步）。
fn effective_line<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    effective: &'a Effective,
) -> Element<'a, Message> {
    if effective.root_unresolved {
        return text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog
                .text(Key::SettingsJavaInstallRootUnresolved)
                .to_string(),
            ui.color(Role::Error),
        );
    }
    let text = catalog.render(
        Key::SettingsJavaInstallRootEffective,
        &[
            ("mode", effective.root_mode),
            ("path", effective.root.as_str()),
        ],
    );
    let layer = match effective.root_layer {
        RootLayer::Setting => catalog
            .text(Key::SettingsJavaInstallRootLayerSetting)
            .to_string(),
        RootLayer::DataMode => catalog.render(
            Key::SettingsJavaInstallRootLayerDataMode,
            &[("mode", effective.root_mode)],
        ),
    };
    column![
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            text,
            ui.color(Role::OnSurface),
        ),
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            layer,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}

/// `java.vendorPriority`：4 行顺序 + 上移 / 下移 + 恢复地区默认序。
fn vendor_order<'a>(ui: &'a Ui, catalog: Catalog, state: &'a State) -> Element<'a, Message> {
    let prefs = &state.prefs;
    let order = prefs.order(state.effective.domestic);
    let mut list = column![text_role_owned(
        ui,
        TypeRole::BodyMedium,
        true,
        catalog.text(Key::SettingsJavaVendorPriority).to_string(),
        ui.color(Role::OnSurface),
    ),]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill);

    for (index, vendor) in order.iter().enumerate() {
        list = list.push(vendor_row(ui, catalog, index, *vendor));
    }

    let joined = state.effective.order.join(" → ");
    let layer = match state.effective.order_layer {
        OrderLayer::Setting => catalog
            .text(Key::SettingsJavaVendorOrderLayerSetting)
            .to_string(),
        OrderLayer::Region => catalog.render(
            Key::SettingsJavaVendorOrderLayerRegion,
            &[(
                "region",
                catalog
                    .text(if state.effective.domestic {
                        Key::SettingsJavaRegionDomestic
                    } else {
                        Key::SettingsJavaRegionGlobal
                    })
                    .to_string()
                    .as_str(),
            )],
        ),
    };
    list = list.push(text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        catalog.render(
            Key::SettingsJavaVendorOrderEffective,
            &[("order", joined.as_str())],
        ),
        ui.color(Role::OnSurface),
    ));
    list = list.push(text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        layer,
        ui.color(Role::OnSurfaceVariant),
    ));
    // 「恢复地区默认序」只在真的排过顺序时才有意义（已经跟地区走时它是个空操作）。
    if !prefs.priority.is_empty() {
        list = list.push(buttons::outlined(
            ui,
            Button::new(
                catalog.text(Key::SettingsJavaVendorReset),
                msg(JavaMessage::ResetPriority),
            )
            .base(BASE),
        ));
    }
    list.into()
}

/// 一家发行版一行：名 + 它发哪些大版本 + 上移 / 下移。
fn vendor_row<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    index: usize,
    vendor: unlucky_core::game::java::JdkVendor,
) -> Element<'a, Message> {
    let majors = vendor
        .hint_majors()
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join("/");
    let supporting = catalog.render(
        Key::SettingsJavaVendorSupports,
        &[("majors", majors.as_str())],
    );
    // 第一条的「上移」与最后一条的「下移」都是空操作：照旧可点，点了不改顺序也不标脏
    // （比灰掉更省心——灰掉的 chip 在 424 宽的列里同样占位，用户还得去猜为什么灰）。
    let up = chip(
        ui,
        Chip::filter(
            catalog.text(Key::SettingsJavaVendorUp),
            false,
            msg(JavaMessage::MoveVendor { index, up: true }),
        )
        .base(BASE),
    );
    let down = chip(
        ui,
        Chip::filter(
            catalog.text(Key::SettingsJavaVendorDown),
            false,
            msg(JavaMessage::MoveVendor { index, up: false }),
        )
        .base(BASE),
    );
    row![
        container(
            column![
                text_role_owned(
                    ui,
                    TypeRole::BodyMedium,
                    index == 0,
                    format!("{}. {}", index + 1, vendor.display()),
                    ui.color(Role::OnSurface),
                ),
                text_role_owned(
                    ui,
                    TypeRole::BodySmall,
                    false,
                    supporting,
                    ui.color(Role::OnSurfaceVariant),
                ),
            ]
            .spacing(ui.space(Space::X1))
            .width(Length::Fill),
        )
        .width(Length::Fill),
        up,
        down,
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .into()
}

/// `java.auto_install`：开 / 关两片 + 「它到底做什么」那句。
fn auto_install<'a>(ui: &'a Ui, catalog: Catalog, state: &'a State) -> Element<'a, Message> {
    let on = state.prefs.auto_install;
    let chips = row![
        chip(
            ui,
            Chip::filter(
                catalog.text(Key::SettingsJavaAutoInstallOn),
                on,
                msg(JavaMessage::ToggleAutoInstall(true)),
            )
            .base(BASE),
        ),
        chip(
            ui,
            Chip::filter(
                catalog.text(Key::SettingsJavaAutoInstallOff),
                !on,
                msg(JavaMessage::ToggleAutoInstall(false)),
            )
            .base(BASE),
        ),
    ]
    .spacing(ui.space(Space::X1));
    column![
        text_role_owned(
            ui,
            TypeRole::BodyMedium,
            true,
            catalog.text(Key::SettingsJavaAutoInstall).to_string(),
            ui.color(Role::OnSurface),
        ),
        chips,
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::SettingsJavaAutoInstallHint).to_string(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}

/// 保存按钮 + 「有未保存改动」那行。
fn actions<'a>(ui: &'a Ui, catalog: Catalog, state: &'a State) -> Element<'a, Message> {
    let label = catalog.text(Key::SettingsJavaPrefsSave);
    let save = if state.prefs.dirty() && state.storage {
        buttons::filled(
            ui,
            Button::new(label, msg(JavaMessage::SavePrefs)).base(BASE),
        )
    } else {
        buttons::filled(ui, Button::disabled(label).base(BASE))
    };
    let hint = if state.prefs.dirty() {
        catalog.text(Key::SettingsJavaPrefsUnsaved).to_string()
    } else {
        String::new()
    };
    column![
        save,
        text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            hint,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}
