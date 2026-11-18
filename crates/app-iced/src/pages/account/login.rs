//! 微软登录面板：设备码（用户码 + 链接 + **倒计时** + 过期重试）与授权码（浏览器 + 可复制链接 + 取消）。
//!
//! ## 为什么不是「一个转圈」
//!
//! 设备码的等待期可以长到 15 分钟。只画转圈的话，用户既不知道要做什么、也不知道还剩多久，
//! 更不知道已经过期了——M114 把这三件事都做成可见状态：用户码、链接、倒计时与「已过期，重试」。
//!
//! ## 文案一律**借用模型里的串**（M178）
//!
//! 倒计时/回退说明这类运行时格式化出来的句子由 [`super::panels`] 在模型层拼好（带 i18n），
//! 视图只拿 `&'a str` 画。为什么不在这里 `format!` 后交给 `frame::text_role_owned`：
//! 那条 owned 路径上的**中文画不出来**（实测：同一位置 ASCII 能画、`剩 15:00` 一片空白），
//! 而借用路径正常。模型持有字符串、视图借用，两个问题一起消掉。

use iced::widget::{column, row};
use iced::{Alignment, Element};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{BrowserLoginNote, LoginFailure};

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
            countdown,
            expired,
            note,
        } => device_code(ui, code, link, countdown, *expired, *note, catalog),
        DevicePanel::Browser {
            link,
            countdown,
            expired,
        } => browser(ui, link, countdown, *expired, catalog),
        DevicePanel::Failed { failure, note } => failed(ui, *failure, *note, catalog),
        DevicePanel::Succeeded { name } => succeeded(ui, name, catalog),
    }
}

/// 空闲：两条流的入口。**主按钮是设备码**（M178）：它不依赖应用注册里的任何一项，是官方
/// 产物上唯一开箱可用的路；浏览器流是右手的次要按钮（要注册 `http://localhost/auth`，
/// 没登记时在预检处直接回退设备码）。
///
/// 文案（复审 r1 P2-5）：主按钮写「使用 Microsoft 账户登录」而不是「改用设备码」；
/// 「没有浏览器时用这个」放在按钮下面一行说明里。
fn idle<'a>(ui: &'a Ui, catalog: Catalog) -> Element<'a, PageMessage> {
    let body = column![row![
        buttons::filled(
            ui,
            Button::new(
                catalog.text(Key::AccountDeviceSignIn),
                Account::StartDevice.into_page(),
            )
        ),
        buttons::outlined(
            ui,
            Button::new(
                catalog.text(Key::AccountDeviceBrowser),
                Account::StartBrowser.into_page(),
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
    .spacing(ui.space(Space::X3))
    .push(frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        catalog.text(Key::AccountDeviceFallbackHint),
        ui.color(Role::OnSurfaceVariant),
    ));
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

/// 设备码等待态：码 + 链接 + 倒计时（`note` = 回退说明）。
fn device_code<'a>(
    ui: &'a Ui,
    code: &'a str,
    link: &'a str,
    countdown: &'a str,
    expired: bool,
    note: Option<BrowserLoginNote>,
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

    let mut body = column![
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
        countdown_line(ui, countdown, expired),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::AccountDeviceHint),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X3));
    // 回退说明放在**动作行之前**：它解释的是「这个面板为什么是设备码」，
    // 用户读完再决定复制码/取消（放在按钮下面会像脚注，读不到）。
    if let Some(text) = note_text(ui, note, catalog) {
        body = body.push(text);
    }
    frame::card(ui, body.push(actions))
}

/// 授权码等待态：浏览器已拉起，链接可复制；过期可重试。
fn browser<'a>(
    ui: &'a Ui,
    link: &'a str,
    countdown: &'a str,
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
        countdown_line(ui, countdown, expired),
        actions,
    ]
    .spacing(ui.space(Space::X3));
    frame::card(ui, body)
}

/// 倒计时 / 过期提示（**这一行是「不得只转圈」的落点**；文案由模型拼好，这里只定颜色与强调）。
fn countdown_line<'a>(ui: &'a Ui, text: &'a str, expired: bool) -> Element<'a, PageMessage> {
    if expired {
        return frame::text_role(ui, TypeRole::LabelMedium, true, text, ui.color(Role::Error));
    }
    frame::text_role(
        ui,
        TypeRole::LabelMedium,
        false,
        text,
        ui.color(Role::OnSurfaceVariant),
    )
}

/// 回退说明的**唯一文案出口**（视图与页面级断言共用：断言取样走这里，才是「视图会画的那句」）。
pub(super) fn browser_note_text(note: BrowserLoginNote, catalog: Catalog) -> &'static str {
    catalog.text(super::model::note_key(note))
}

/// 回退说明那一行（`None` = 没有这回事）。
fn note_text<'a>(
    ui: &'a Ui,
    note: Option<BrowserLoginNote>,
    catalog: Catalog,
) -> Option<Element<'a, PageMessage>> {
    let note = note?;
    // 用正文色而不是 `Error`：这不是失败态（设备码接着跑），画成红色会让人以为登录已经挂了。
    Some(frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        browser_note_text(note, catalog),
        ui.color(Role::OnSurfaceVariant),
    ))
}

/// 失败态：**M3 内联 alert**（左侧 `ErrorContainer` 图标块 + 标题/正文/Code + 重试/关闭）。
///
/// - 语言泄漏（r1 p1）：只画原因码对应的 i18n + 一行 `Code: …`，后端中文原文不上屏；
/// - 不许通栏实心红（tower r2）：面板底 `SurfaceContainerHigh` + 描边，`ErrorContainer` 只做图标块强调；
/// - 可操作的失败要有可读说明（M132）：缺 client id 换专属标题 + 「要设哪个环境变量」，且不给「重试」；
/// - M178：`note`（浏览器登录没走成）另起一行画在正文下面，两者都能看见。
fn failed<'a>(
    ui: &'a Ui,
    failure: LoginFailure,
    note: Option<BrowserLoginNote>,
    catalog: Catalog,
) -> Element<'a, PageMessage> {
    let mut body = catalog.text(failure_body_key(failure)).to_string();
    if let Some(note) = note {
        body.push_str("\n\n");
        body.push_str(browser_note_text(note, catalog));
    }
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
