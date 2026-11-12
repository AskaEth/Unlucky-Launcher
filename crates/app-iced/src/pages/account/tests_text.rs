//! 「**模型这一层会给出什么**」的取样器：视图模型字段 + **按码做的 i18n 查表**，拼成一串。
//!
//! ## 边界（复审 r2 P2-2 订正，别把这条读成「视图覆盖」）
//!
//! 这是**平行实现**：它自己调 `catalog` 查表，**不调用** `login::panel` / `forms::status_line`，
//! 也不遍历 iced 的元素树。所以它证明的是「模型 + i18n 表里没有中文」，**不证明**「视图真的照它画」。
//!
//! 视图层靠两道别的闸：
//! 1. **类型**：`DevicePanel` / `AuthlibPanel` 里**没有** `message`/`detail` 字段（r3 删掉了），
//!    想画后端原文是编译错误，不是测试问题；
//! 2. **出图**：`DevDocs/artifacts/m114/after/*` 由人（tower/复审）逐张看。
//!
//! 为什么单独一个文件：`pages/account/tests.rs` 贴着 400 行硬线，而这段是**纯取样**
//! （不给任何断言）——断言都在 `tests.rs` 里。

use unlucky_i18n::{Key, Lang};
use unlucky_presenter::dto::AccountNoticeCode;

use super::login::{browser_note_text, failure_body_key, failure_title_key};
use super::model::{AuthlibPanel, DevicePanel, Hint, Page};

/// 页面**会画什么**：模型里的数据 + 视图层对码的 i18n 查表结果。
pub(super) fn on_screen(page: &Page) -> String {
    let catalog = Lang::En.catalog();
    let mut out = String::new();
    for row in &page.rows {
        out.push_str(&row.name);
        out.push_str(&row.monogram);
        out.push_str(&row.kind_label);
        out.push_str(&row.added);
        out.push_str(&row.detail);
    }
    out.push_str(&page.form.offline_name);
    out.push_str(&page.form.server);
    if let Some(notice) = &page.notice {
        // 与 `widgets::banner` 同源：码 → i18n（主题名拼进去）
        out.push_str(match notice.code {
            AccountNoticeCode::OfflineNameInvalid => {
                catalog.text(Key::AccountNoticeOfflineNameInvalid)
            }
            AccountNoticeCode::AccountExists => catalog.text(Key::AccountNoticeAccountExists),
            AccountNoticeCode::StoreFailed => catalog.text(Key::AccountNoticeStoreFailed),
            AccountNoticeCode::NotFound => catalog.text(Key::AccountNoticeNotFound),
            AccountNoticeCode::Other => catalog.text(Key::AccountNoticeOther),
            AccountNoticeCode::Added => catalog.text(Key::AccountNoticeAdded),
            AccountNoticeCode::Removed => catalog.text(Key::AccountNoticeRemoved),
            AccountNoticeCode::Switched => catalog.text(Key::AccountNoticeSwitched),
        });
        out.push_str(&notice.subject);
    }
    if let Some(hint) = page.hint {
        out.push_str(match hint {
            Hint::AuthlibReauth => catalog.text(Key::AccountHintAuthlibReauth),
            Hint::OfflineNoRelogin => catalog.text(Key::AccountHintOfflineNoRelogin),
        });
    }
    match &page.device {
        DevicePanel::Hidden => {}
        DevicePanel::Requesting => out.push_str(catalog.text(Key::AccountDeviceRequesting)),
        DevicePanel::Code {
            code,
            link,
            countdown,
            note,
            ..
        } => {
            out.push_str(code);
            out.push_str(link);
            // 倒计时文案由模型拼好（M178：owned 文字路径画不出中文）。
            out.push_str(countdown);
            // M178：回退说明走**视图自己的**文案出口（`login::browser_note_text`），
            // 不是测试另写一份映射——否则视图哪天不画这一行，取样仍会绿。
            if let Some(note) = note {
                out.push_str(browser_note_text(*note, catalog));
            }
        }
        DevicePanel::Browser {
            link, countdown, ..
        } => {
            out.push_str(link);
            out.push_str(countdown);
        }
        DevicePanel::Failed { failure, note } => {
            out.push_str(catalog.text(failure_title_key(*failure)));
            out.push_str(catalog.text(failure_body_key(*failure)));
            out.push_str(&catalog.render(Key::AccountFailureCode, &[("code", failure.code())]));
            if let Some(note) = note {
                out.push_str(browser_note_text(*note, catalog));
            }
        }
        DevicePanel::Succeeded { name } => {
            out.push_str(&catalog.render(Key::AccountLoginSucceeded, &[("name", name)]));
        }
    }
    match &page.authlib {
        AuthlibPanel::Idle => {}
        AuthlibPanel::Running => out.push_str(catalog.text(Key::AccountServerChecking)),
        AuthlibPanel::Ready { server_name } => out.push_str(server_name),
        AuthlibPanel::Failed { failure } => out.push_str(failure.code()),
        AuthlibPanel::Succeeded { name } => out.push_str(name),
    }
    out
}

/// 有没有中日韩统一表意文字（语言泄漏的最直接判据）。
pub(super) fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{3000}'..='\u{303F}' | '\u{FF00}'..='\u{FFEF}'))
}
