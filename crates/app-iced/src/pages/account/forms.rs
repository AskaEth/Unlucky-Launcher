//! 三条轨道的入口卡与两张表单（本地档案 / 第三方皮肤站）。
//!
//! ## 为什么是「三条轨道」而不是「一个添加按钮 + 一个类型下拉」
//!
//! 三条轨道的**字段完全不同**（微软没有密码输入框、本地档案没有服务器、第三方站两样都有），
//! 而且失败原因也不同（403 / 白名单未批 / 服务器不可达）。把它们压进一个表单只会让每种情况
//! 都要解释自己不属于另外两种——所以入口就是三张卡，各自展开自己的面板。

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key, Lang};

use crate::app::components::buttons::{self, Button};
use crate::app::components::feedback::empty_state::{empty_state, EmptyState};
use crate::app::components::inputs::{text_field, Field, FieldVariant};
use crate::pages::frame;
use crate::theme::Ui;

use crate::pages::account::model::{AuthlibPanel, Page, Track};
use crate::pages::Message as PageMessage;

use super::{login, widgets, Message as Account};

/// 空态：不是一张白页——图标块 + 标题 + 说明，下面紧跟**三张轨道卡**（真正的入口）。
///
/// 复审 r1 的 P2-4：空态原先有**三套**同一件事的入口（右上按钮 / 三个 chip / 三张卡），
/// chip 还被当成操作按钮用。现在只剩「右上 Add an account（展开/收起）+ 三张卡」这一套：
/// 空态时卡片本来就可见，有账号时收在按钮后面。
pub fn empty<'a>(ui: &'a Ui, catalog: Catalog, lang: Lang) -> Element<'a, PageMessage> {
    let _ = lang;
    let state = empty_state(
        ui,
        EmptyState::new(
            unlucky_design::icons::IconKind::User,
            catalog.text(Key::AccountEmptyTitle),
            catalog.text(Key::AccountEmptyBody),
        ),
    );
    container(state)
        .height(Length::Fixed(ui.space(Space::X8) * 8.0))
        .width(Length::Fill)
        .into()
}

/// 三条轨道的入口卡（面板没打开时画这一排）。
pub fn tracks<'a>(ui: &'a Ui, catalog: Catalog, lang: Lang) -> Element<'a, PageMessage> {
    let _ = lang;
    let mut row = iced::widget::Row::new().spacing(ui.space(Space::X4));
    for track in Track::ALL {
        row = row.push(track_card(ui, track, catalog));
    }
    row.width(Length::Fill).into()
}

fn track_card<'a>(ui: &'a Ui, track: Track, catalog: Catalog) -> Element<'a, PageMessage> {
    let body = column![
        frame::text_role(
            ui,
            TypeRole::TitleSmall,
            true,
            catalog.text(track.title_key()),
            ui.color(Role::OnSurface),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(track.body_key()),
            ui.color(Role::OnSurfaceVariant),
        ),
        buttons::outlined(
            ui,
            Button::new(
                catalog.text(track.action_key()),
                Account::Open(track).into_page(),
            ),
        ),
    ]
    .spacing(ui.space(Space::X3));
    container(frame::card(ui, body))
        .width(Length::FillPortion(1))
        .into()
}

/// 当前打开的面板。
pub fn panel<'a>(
    ui: &'a Ui,
    page: &'a Page,
    catalog: Catalog,
    lang: Lang,
) -> Element<'a, PageMessage> {
    let _ = lang;
    match page.panel {
        crate::pages::account::model::Panel::None => tracks(ui, catalog, Lang::En),
        crate::pages::account::model::Panel::Microsoft => login::panel(ui, &page.device, catalog),
        crate::pages::account::model::Panel::Authlib => authlib_form(ui, page, catalog),
        crate::pages::account::model::Panel::Offline => offline_form(ui, page, catalog),
    }
}

/// 本地档案表单（**说明里带「仅本地/隔离网络测试用」这条限定**）。
fn offline_form<'a>(ui: &'a Ui, page: &'a Page, catalog: Catalog) -> Element<'a, PageMessage> {
    let ready = page.form.offline_ready();
    let submit = if ready {
        buttons::filled(
            ui,
            Button::new(
                catalog.text(Key::AccountOfflineSubmit),
                Account::Submit.into_page(),
            ),
        )
    } else {
        buttons::filled(
            ui,
            Button::disabled(catalog.text(Key::AccountOfflineSubmit)),
        )
    };

    let body = column![
        // 标题下**不再复读**同一句限定：下面那条 `ErrorContainer` 警告条就是它
        // （两处一模一样的句子在 1180×760 下看着像重复渲染）。
        widgets::section(ui, catalog.text(Key::AccountTrackOfflineTitle), None),
        warning(ui, catalog.text(Key::AccountOfflineWarning)),
        text_field(
            ui,
            Field::new(&page.form.offline_name)
                .variant(FieldVariant::Outlined)
                .label(catalog.text(Key::AccountOfflineNameLabel))
                .placeholder(catalog.text(Key::AccountOfflineNameHint))
                .supporting(catalog.text(Key::AccountOfflineNameHint)),
            |value| Account::OfflineName(value).into_page(),
        ),
        row![
            submit,
            buttons::text(
                ui,
                Button::new(
                    catalog.text(Key::AccountDialogClose),
                    Account::Close.into_page()
                )
            ),
        ]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center),
    ]
    .spacing(ui.space(Space::X3));
    frame::card(ui, body)
}

/// 第三方站表单：地址（可单独校验）+ 登录名 + 密码 + 登录。
fn authlib_form<'a>(ui: &'a Ui, page: &'a Page, catalog: Catalog) -> Element<'a, PageMessage> {
    let busy = page.authlib.busy();
    let ready = page.form.authlib_ready() && !busy;

    let submit = if ready {
        buttons::filled(
            ui,
            Button::new(
                catalog.text(Key::AccountAuthlibSubmit),
                Account::Submit.into_page(),
            ),
        )
    } else {
        buttons::filled(
            ui,
            Button::disabled(catalog.text(Key::AccountAuthlibSubmit)),
        )
    };

    let mut body = column![
        widgets::section(
            ui,
            catalog.text(Key::AccountTrackAuthlibTitle),
            Some(catalog.text(Key::AccountTrackAuthlibBody)),
        ),
        text_field(
            ui,
            Field::new(&page.form.server)
                .variant(FieldVariant::Outlined)
                .label(catalog.text(Key::AccountServerLabel))
                .placeholder(catalog.text(Key::AccountServerHint))
                .supporting(if page.checked_server.is_empty() {
                    catalog.text(Key::AccountServerHint)
                } else {
                    page.checked_server.as_str()
                }),
            |value| Account::Server(value).into_page(),
        ),
        text_field(
            ui,
            Field::new(&page.form.login)
                .variant(FieldVariant::Outlined)
                .label(catalog.text(Key::AccountLoginNameLabel))
                .placeholder(""),
            |value| Account::Login(value).into_page(),
        ),
        text_field(
            ui,
            Field::new(&page.form.password)
                .variant(FieldVariant::Outlined)
                .label(catalog.text(Key::AccountPasswordLabel))
                .supporting(catalog.text(Key::AccountPasswordHint))
                // 密码**掩码**显示：明文只在内存里，不上屏
                .secure(true),
            |value| Account::Password(value).into_page(),
        ),
        status_line(ui, page, catalog),
        row![
            buttons::outlined(
                ui,
                Button::new(
                    catalog.text(Key::AccountServerProbe),
                    Account::Probe.into_page()
                )
            ),
            submit,
            buttons::text(
                ui,
                Button::new(
                    catalog.text(Key::AccountDialogClose),
                    Account::Close.into_page()
                )
            ),
        ]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center),
    ]
    .spacing(ui.space(Space::X3));

    if !ready && !busy {
        body = body.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::AccountAuthlibIdleHint),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    frame::card(ui, body)
}

/// 第三方轨的状态行（校验通过 / 进行中 / 失败原文）。
///
/// 进行中与通过都走 **i18n**：presenter 给的 `message` 是后端诊断串（中文），
/// 直接上屏会让英文界面里混一段中文；失败态才用它（那是要人读的错误原因）。
fn status_line<'a>(ui: &'a Ui, page: &'a Page, catalog: Catalog) -> Element<'a, PageMessage> {
    match &page.authlib {
        AuthlibPanel::Idle => frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            "",
            ui.color(Role::OnSurfaceVariant),
        ),
        AuthlibPanel::Running => frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            catalog.text(Key::AccountServerChecking),
            ui.color(Role::OnSurfaceVariant),
        ),
        AuthlibPanel::Ready { server_name, .. } => frame::text_role_owned(
            ui,
            TypeRole::LabelMedium,
            true,
            catalog.render(Key::AccountServerOk, &[("server", server_name.as_str())]),
            ui.color(Role::Primary),
        ),
        // 失败只画**原因码对应的 i18n** + 一行 Code（原文是中文诊断串，上屏就是语言泄漏）
        AuthlibPanel::Failed { failure } => column![
            frame::text_role(
                ui,
                TypeRole::LabelMedium,
                false,
                catalog.text(super::login::failure_body_key(*failure)),
                ui.color(Role::Error),
            ),
            frame::text_role_owned(
                ui,
                TypeRole::LabelSmall,
                false,
                catalog.render(Key::AccountFailureCode, &[("code", failure.code())]),
                ui.color(Role::Error),
            ),
        ]
        .spacing(ui.space(Space::X1))
        .into(),
        AuthlibPanel::Succeeded { name } => frame::text_role(
            ui,
            TypeRole::LabelMedium,
            true,
            name,
            ui.color(Role::Primary),
        ),
    }
}

/// 「仅本地/隔离网络测试用」这条限定：**带色块**，不是一句灰字（用户要点得明白）。
fn warning<'a>(ui: &'a Ui, text: &'a str) -> Element<'a, PageMessage> {
    container(frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        text,
        ui.color(Role::OnErrorContainer),
    ))
    .width(Length::Fill)
    .padding(ui.space(Space::X2))
    .style(move |_theme| container::Style {
        background: Some(iced::Background::Color(ui.color(Role::ErrorContainer))),
        border: iced::Border {
            radius: ui.radius_dp(unlucky_design::shape::ShapeScale::Small),
            ..iced::Border::default()
        },
        ..container::Style::default()
    })
    .into()
}
