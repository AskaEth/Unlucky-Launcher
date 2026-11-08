//! 微软登录面板：设备码（用户码 + 链接 + **倒计时** + 过期重试）与授权码（浏览器 + 可复制链接 + 取消）。
//!
//! ## 为什么不是「一个转圈」
//!
//! 设备码流的等待期可以长到 15 分钟。只画转圈的话，用户既不知道要做什么、也不知道还剩多久，
//! 更不知道已经过期了——M114 把这三件事都做成可见状态：用户码（可复制）、链接（可复制/打开）、
//! 倒计时（`{secs}s left`）与过期后的「已过期，重试」。授权码流同理，另外必须能**取消**。

use iced::widget::{column, row};
use iced::{Alignment, Element};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::LoginFailure;

use crate::app::components::buttons::{self, Button};
use crate::pages::frame;
use crate::theme::Ui;

use crate::pages::account::model::DevicePanel;
use crate::pages::Message as PageMessage;

use super::Message as Account;

/// 微软登录面板。
pub fn panel<'a>(
    ui: &'a Ui,
    device: &'a DevicePanel,
    catalog: Catalog,
) -> Element<'a, PageMessage> {
    match device {
        DevicePanel::Hidden => idle(ui, catalog),
        DevicePanel::Requesting => requesting(ui, catalog),
        DevicePanel::Code {
            code,
            link,
            remaining,
            expired,
        } => device_code(ui, code, link, *remaining, *expired, catalog),
        DevicePanel::Browser {
            link,
            remaining,
            expired,
        } => browser(ui, link, *remaining, *expired, catalog),
        DevicePanel::Failed { failure } => failed(ui, *failure, catalog),
        DevicePanel::Succeeded { name } => succeeded(ui, name, catalog),
    }
}

/// 空闲：给出两条流的入口（GUI 默认授权码流，设备码是备选）。
fn idle<'a>(ui: &'a Ui, catalog: Catalog) -> Element<'a, PageMessage> {
    let body = column![row![
        buttons::filled(
            ui,
            Button::new(
                catalog.text(Key::AccountDeviceBrowser),
                Account::StartBrowser.into_page(),
            )
        ),
        buttons::outlined(
            ui,
            Button::new(
                catalog.text(Key::AccountDeviceUseCode),
                Account::StartDevice.into_page(),
            )
        ),
        buttons::text(
            ui,
            Button::new(
                catalog.text(Key::AccountDeviceCancel),
                Account::Close.into_page()
            )
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center),]
    .spacing(ui.space(Space::X3));
    frame::card(ui, body)
}

/// 「正在申请」：一句 i18n 状态 + 取消。
///
/// presenter 那边的进度串是中文诊断（只进日志），面板态里**根本没有**这个字段可画。
fn requesting<'a>(ui: &'a Ui, catalog: Catalog) -> Element<'a, PageMessage> {
    let body = column![
        frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            catalog.text(Key::AccountDeviceRequesting),
            ui.color(Role::OnSurfaceVariant),
        ),
        row![buttons::text(
            ui,
            Button::new(
                catalog.text(Key::AccountDeviceCancel),
                Account::Cancel.into_page()
            )
        )]
        .align_y(Alignment::Center),
    ]
    .spacing(ui.space(Space::X3));
    frame::card(ui, body)
}

/// 设备码等待态：**码 + 链接 + 倒计时**。
fn device_code<'a>(
    ui: &'a Ui,
    code: &'a str,
    link: &'a str,
    remaining: Option<u64>,
    expired: bool,
    catalog: Catalog,
) -> Element<'a, PageMessage> {
    let mut actions = row![
        buttons::tonal(
            ui,
            Button::new(
                catalog.text(Key::AccountDeviceCodeCopy),
                Account::Copy(code.to_string()).into_page(),
            )
        ),
        buttons::outlined(
            ui,
            Button::new(
                catalog.text(Key::AccountDeviceLinkCopy),
                Account::Copy(link.to_string()).into_page(),
            )
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center);

    if expired {
        actions = actions.push(buttons::filled(
            ui,
            Button::new(
                catalog.text(Key::AccountDeviceRetry),
                Account::StartDevice.into_page(),
            ),
        ));
    }
    actions = actions.push(buttons::text(
        ui,
        Button::new(
            catalog.text(Key::AccountDeviceCancel),
            Account::Cancel.into_page(),
        ),
    ));

    let countdown = countdown_line(ui, remaining, expired, catalog);

    let body = column![
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            catalog.text(Key::AccountDeviceCodeLabel),
            ui.color(Role::OnSurfaceVariant),
        ),
        frame::text_role(
            ui,
            TypeRole::HeadlineSmall,
            true,
            code,
            ui.color(Role::OnSurface),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            link,
            ui.color(Role::OnSurfaceVariant),
        ),
        countdown,
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::AccountDeviceHint),
            ui.color(Role::OnSurfaceVariant),
        ),
        actions,
    ]
    .spacing(ui.space(Space::X3));
    frame::card(ui, body)
}

/// 授权码等待态：浏览器已拉起，链接可复制；过期可重试。
fn browser<'a>(
    ui: &'a Ui,
    link: &'a str,
    remaining: Option<u64>,
    expired: bool,
    catalog: Catalog,
) -> Element<'a, PageMessage> {
    let mut actions = row![buttons::outlined(
        ui,
        Button::new(
            catalog.text(Key::AccountDeviceLinkCopy),
            Account::Copy(link.to_string()).into_page(),
        )
    )]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center);
    if expired {
        actions = actions.push(buttons::filled(
            ui,
            Button::new(
                catalog.text(Key::AccountDeviceRetry),
                Account::StartBrowser.into_page(),
            ),
        ));
    }
    actions = actions.push(buttons::text(
        ui,
        Button::new(
            catalog.text(Key::AccountDeviceCancel),
            Account::Cancel.into_page(),
        ),
    ));

    let body = column![
        frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            catalog.text(Key::AccountDeviceBrowserHint),
            ui.color(Role::OnSurface),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            link,
            ui.color(Role::OnSurfaceVariant),
        ),
        countdown_line(ui, remaining, expired, catalog),
        actions,
    ]
    .spacing(ui.space(Space::X3));
    frame::card(ui, body)
}

/// 倒计时 / 过期提示（**这一行是「不得只转圈」的落点**）。
fn countdown_line<'a>(
    ui: &'a Ui,
    remaining: Option<u64>,
    expired: bool,
    catalog: Catalog,
) -> Element<'a, PageMessage> {
    if expired {
        return frame::text_role(
            ui,
            TypeRole::LabelMedium,
            true,
            catalog.text(Key::AccountDeviceExpired),
            ui.color(Role::Error),
        );
    }
    let text = remaining.map_or_else(String::new, |secs| {
        // mm:ss（`15:00 left`）而不是 `900s left`：人读的是分秒，而且不随语言换格式
        // （数字与冒号在两种语言里同形，只有 `left`/`剩` 走 catalog）。
        let clock = format!("{:02}:{:02}", secs / 60, secs % 60);
        catalog.render(Key::AccountDeviceCountdown, &[("time", &clock)])
    });
    // 运行时格式化的串走 `text_role_owned`（`text_role` 只借 `&'a str`）
    frame::text_role_owned(
        ui,
        TypeRole::LabelMedium,
        false,
        text,
        ui.color(Role::OnSurfaceVariant),
    )
}

/// 失败态：**M3 内联 alert**（左侧 `ErrorContainer` 图标块 + 标题/正文/Code + 重试/关闭）。
///
/// 三点修订叠在这里：
/// - **语言泄漏（r1 p1）**：只画原因码对应的 i18n + 一行 `Code: …`，后端中文原文不上屏；
/// - **不许通栏实心红（tower r2）**：面板底走 `SurfaceContainerHigh` + 描边，
///   `ErrorContainer`/`OnErrorContainer` 只作左侧图标块的强调（角色不变，面积收小）；
/// - **可操作的失败要有可读说明（M132）**：缺 client id 换专属标题 + 「要设哪个环境变量」的
///   正文，且**不给「重试」**——进程内的环境变量改不了，重试必然还是同一个错。
fn failed<'a>(ui: &'a Ui, failure: LoginFailure, catalog: Catalog) -> Element<'a, PageMessage> {
    let body = catalog.text(failure_body_key(failure)).to_string();
    let code = catalog.render(Key::AccountFailureCode, &[("code", failure.code())]);
    let mut actions: Vec<Element<'a, PageMessage>> = Vec::new();
    if !matches!(failure, LoginFailure::MsaNotConfigured) {
        actions.push(buttons::tonal(
            ui,
            Button::new(
                catalog.text(Key::AccountDeviceRetry),
                Account::StartDevice.into_page(),
            ),
        ));
    }
    actions.push(buttons::text(
        ui,
        Button::new(
            catalog.text(Key::AccountDialogClose),
            Account::Close.into_page(),
        ),
    ));
    super::widgets::alert(
        ui,
        super::widgets::Tone::Error,
        catalog.text(failure_title_key(failure)),
        body,
        Some(code),
        actions,
    )
}

/// 成功态。
fn succeeded<'a>(ui: &'a Ui, name: &'a str, catalog: Catalog) -> Element<'a, PageMessage> {
    // 成功也走 key + 参数（名字是 `{name}` 参数）——presenter 不再拼任何人类可读句子
    // （复审 r1 P1-1 那条「报告与代码不符」的收口：`已登录：{name}` 这类拼接全部消失）。
    let text = catalog.render(Key::AccountLoginSucceeded, &[("name", name)]);
    let body = column![
        frame::text_role_owned(
            ui,
            TypeRole::BodyMedium,
            true,
            text,
            ui.color(Role::OnSurface),
        ),
        row![buttons::text(
            ui,
            Button::new(
                catalog.text(Key::AccountDialogClose),
                Account::Close.into_page()
            )
        )]
        .align_y(Alignment::Center),
    ]
    .spacing(ui.space(Space::X3));
    frame::card(ui, body)
}

/// 原因码 → i18n 正文 key（页面里唯一的「码→文案」映射点）。
pub(super) fn failure_body_key(failure: LoginFailure) -> Key {
    match failure {
        LoginFailure::Denied => Key::AccountFailureDenied,
        LoginFailure::NotWhitelisted => Key::AccountFailureWhitelist,
        LoginFailure::Unreachable => Key::AccountFailureUnreachable,
        LoginFailure::NotAuthlibServer => Key::AccountFailureNotAuthlib,
        LoginFailure::BadCredentials => Key::AccountFailureBadCredentials,
        LoginFailure::NoProfile => Key::AccountFailureNoProfile,
        LoginFailure::StoreFailed => Key::AccountFailureStore,
        LoginFailure::MsaNotConfigured => Key::AccountFailureMsaNotConfigured,
        LoginFailure::Other => Key::AccountFailureOther,
    }
}

/// 原因码 → i18n **标题** key。
///
/// 只有「缺 client id」有专属标题（M132）：标题写着 `Sign-in failed` 的话，用户会以为是自己
/// 的账号/网络出了问题，而不会被指向那一项要补的配置。
pub(super) fn failure_title_key(failure: LoginFailure) -> Key {
    match failure {
        LoginFailure::MsaNotConfigured => Key::AccountLoginNotConfigured,
        _ => Key::AccountLoginFailed,
    }
}
