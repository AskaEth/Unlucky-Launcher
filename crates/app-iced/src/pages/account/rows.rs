//! 账户列表：当前账户卡片 + 全部账户行 + 皮肤预览区。

use iced::widget::{column, container, image, row, space};
use iced::{Alignment, ContentFit, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key, Lang};

use crate::app::components::buttons::{self, Button};
use crate::pages::frame;
use crate::theme::Ui;

use crate::pages::account::{model::Page as Model, widgets};
use crate::pages::Message as PageMessage;

use super::Message as Account;

/// 当前账户卡片（含皮肤预览区）。
pub fn active_card<'a>(
    ui: &'a Ui,
    page: &'a Model,
    catalog: Catalog,
    lang: Lang,
) -> Element<'a, PageMessage> {
    let Some(current) = page.active_row() else {
        return container(space::vertical()).into();
    };
    let _ = lang;

    let mut actions = row![]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center);
    actions = actions.push(buttons::text(
        ui,
        Button::new(
            catalog.text(Key::AccountRelogin),
            Account::Relogin(current.id.clone()).into_page(),
        ),
    ));
    actions = actions.push(buttons::text(
        ui,
        Button::new(
            catalog.text(Key::AccountRemove),
            Account::Remove(current.id.clone()).into_page(),
        ),
    ));

    let mut name_line = row![
        frame::text_role(
            ui,
            TypeRole::TitleMedium,
            true,
            &current.name,
            ui.color(Role::OnSurface),
        ),
        widgets::badge(ui, &current.kind_label, false),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center);
    if current.active {
        name_line = name_line.push(widgets::badge(
            ui,
            catalog.text(Key::AccountActiveChip),
            true,
        ));
    }

    let mut identity = column![
        name_line,
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            &current.added,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1));
    // 与列表行同源：服务器地址 / 登录名这一行两边都要有（tower 看图意见 ①）
    if !current.detail.is_empty() {
        identity = identity.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            &current.detail,
            ui.color(Role::OnSurfaceVariant),
        ));
    }

    let left = row![
        widgets::avatar(ui, current, widgets::AVATAR_ACTIVE),
        container(identity).width(Length::Fill),
        actions,
    ]
    .spacing(ui.space(Space::X4))
    .align_y(Alignment::Center);

    let mut body = column![
        widgets::section(ui, catalog.text(Key::AccountCurrentTitle), None),
        left,
    ]
    .spacing(ui.space(Space::X4));

    match page.active_skin() {
        Some(skin) => body = body.push(skin_preview(ui, skin.clone(), catalog)),
        // 没有皮肤就**说出来**（离线轨永远没有；第三方轨没抓到也是这句），别让用户猜
        None => body = body.push(skin_absent(ui, catalog)),
    }

    frame::card(ui, body)
}

/// 无皮肤时的皮肤区：一句话说清「没有可用皮肤」，与皮肤预览区同一位置、同一节奏。
fn skin_absent<'a>(ui: &'a Ui, catalog: Catalog) -> Element<'a, PageMessage> {
    column![
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            true,
            catalog.text(Key::AccountSkinTitle),
            ui.color(Role::OnSurface),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::AccountSkinNone),
            ui.color(Role::OnSurfaceVariant),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::AccountSkinHint),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .into()
}

/// 皮肤预览区：**原图直出**（64×64），旁边一句话说清「皮肤从哪来」。
fn skin_preview<'a>(
    ui: &'a Ui,
    skin: iced::widget::image::Handle,
    catalog: Catalog,
) -> Element<'a, PageMessage> {
    let size = ui.space(Space::X8) * 2.0;
    row![
        container(
            image(skin)
                .width(Length::Fixed(size))
                .height(Length::Fixed(size))
                .content_fit(ContentFit::Contain),
        )
        .width(Length::Fixed(size))
        .height(Length::Fixed(size)),
        column![
            frame::text_role(
                ui,
                TypeRole::LabelMedium,
                true,
                catalog.text(Key::AccountSkinTitle),
                ui.color(Role::OnSurface),
            ),
            frame::text_role(
                ui,
                TypeRole::BodySmall,
                false,
                catalog.text(Key::AccountSkinHint),
                ui.color(Role::OnSurfaceVariant),
            ),
        ]
        .spacing(ui.space(Space::X1)),
    ]
    .spacing(ui.space(Space::X4))
    .align_y(Alignment::Center)
    .into()
}

/// 「全部账户」列表。
pub fn list<'a>(
    ui: &'a Ui,
    page: &'a Model,
    catalog: Catalog,
    lang: Lang,
) -> Element<'a, PageMessage> {
    let _ = lang;
    let mut items = column![widgets::section(
        ui,
        catalog.text(Key::AccountListTitle),
        None
    )]
    .spacing(ui.space(Space::X3));

    for row in &page.rows {
        items = items.push(item(ui, row, catalog));
    }
    frame::card(ui, items)
}

/// 一行账户。
fn item<'a>(
    ui: &'a Ui,
    row: &'a crate::pages::account::model::Row,
    catalog: Catalog,
) -> Element<'a, PageMessage> {
    let mut detail = column![
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            &row.name,
            ui.color(Role::OnSurface),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            &row.added,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1));

    if !row.detail.is_empty() {
        detail = detail.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            &row.detail,
            ui.color(Role::OnSurfaceVariant),
        ));
    }

    let mut trailing = row![widgets::badge(ui, &row.kind_label, false),]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center);

    if row.active {
        trailing = trailing.push(widgets::badge(
            ui,
            catalog.text(Key::AccountActiveChip),
            true,
        ));
    } else {
        trailing = trailing.push(buttons::text(
            ui,
            Button::new(
                catalog.text(Key::AccountSwitch),
                Account::Switch(row.id.clone()).into_page(),
            ),
        ));
    }
    trailing = trailing.push(buttons::text(
        ui,
        Button::new(
            catalog.text(Key::AccountRemove),
            Account::Remove(row.id.clone()).into_page(),
        ),
    ));

    row![
        widgets::avatar(ui, row, widgets::AVATAR_LIST),
        container(detail).width(Length::Fill),
        trailing,
    ]
    .spacing(ui.space(Space::X3))
    .align_y(Alignment::Center)
    .padding(iced::Padding::from([0.0, ui.space(Space::X2)]))
    .into()
}
