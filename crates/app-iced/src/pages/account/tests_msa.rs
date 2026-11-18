//! M178 的登录回退判据（从 `tests.rs` 拆出来守 400 行硬线）。
//!
//! 这两条钉的是**页面这一半**：浏览器登录没走成、已回退设备码时，用户看到的是
//! i18n 那句「为什么 + 怎么修」，而不是 AAD 的原始报文。core 那一半（真的回退、不白开监听口）
//! 在 `crates/core/tests/auth/microsoft_msalogin.rs`。

use unlucky_i18n::Lang;
use unlucky_presenter::dto::{
    AppEvent, BrowserLoginNote, CoreEvent, DeviceLoginView, LoginFailure,
};
use unlucky_presenter::state::AppState;

use super::avatar::Cache;
use super::login::browser_note_text;
use super::model::{DevicePanel, Form, Page};
use super::tests::build;
use super::tests_text::{has_cjk, on_screen};

/// 中文界面的同一页（`build` 钉死英文，这里要中文那一套）。
fn build_zh(state: &AppState, form: &Form) -> Page {
    Page::build(state, form, &mut Cache::default(), Lang::ZhCn, 1180.0)
}

/// **M178 守卫（页面这一半）**：浏览器登录没走成、已回退设备码时，页面上要有一句**可读的**
/// 中文/英文说明（含「怎么修」），且**不是** AAD 的原始报文。
///
/// 抓得住：`browser_note` 在 `device_panel` 里被丢掉（面板就只剩用户码，用户不知道为什么
/// 变成了设备码）、或有人把 presenter 的原始诊断串（`invalid_request ...`）画上屏。
/// 抓不住：这句提示在 1180 宽下会不会换行难看（那靠出图给 tower 看）。
#[test]
fn a_browser_fallback_shows_a_readable_note_not_the_raw_aad_error() {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::DeviceLoginChanged(
        DeviceLoginView {
            message: "invalid_request: The provided value for the input parameter 'redirect_uri' \
is not valid. The expected value is a URI which matches a redirect URI registered for this client \
application."
                .to_string(),
            ..DeviceLoginView::waiting("ABCD-EFGH", "https://www.microsoft.com/link", 900, "")
        }
        .with_browser_note(BrowserLoginNote::RedirectNotRegistered),
    )));
    let page = build(&state, &Form::new());

    // 1. 面板态带着说明码（不是只剩一个用户码）
    match &page.device {
        DevicePanel::Code { note, code, .. } => {
            assert_eq!(*note, Some(BrowserLoginNote::RedirectNotRegistered));
            assert_eq!(code, "ABCD-EFGH");
        }
        other => panic!("回退后仍是设备码面板，实际 {other:?}"),
    }

    // 2. 英文界面上画出来的那一段：是 i18n 文案、含可照抄的修法，且没有 AAD 原始报文/中文
    let text = on_screen(&page);
    assert!(!has_cjk(&text), "英文界面不许出现中文：{text}");
    assert!(
        text.contains("http://localhost/auth"),
        "提示里要给出可照抄的重定向 URI：{text}"
    );
    assert!(
        text.to_lowercase().contains("redirect uri"),
        "提示里要点名「重定向 URI」这一项：{text}"
    );
    assert!(
        !text.contains("invalid_request"),
        "原始报文不许上屏：{text}"
    );
    assert!(!text.contains("AADSTS"), "原始报文不许上屏：{text}");
    assert!(!text.contains("is not valid"), "原始报文不许上屏：{text}");

    // 3. 中文界面同样走一遍（复审 r1 P2-4）：文案来自**视图自己的出口**
    //    `login::browser_note_text`，中文要有、可照抄的修法要有、AAD 原始码不许有。
    let zh = build_zh(&state, &Form::new());
    let zh_text = on_screen(&zh);
    assert!(has_cjk(&zh_text), "中文界面要真有中文：{zh_text}");
    let note_text = browser_note_text(
        BrowserLoginNote::RedirectNotRegistered,
        Lang::ZhCn.catalog(),
    );
    assert!(
        note_text.contains("http://localhost/auth"),
        "中文文案也要给出可照抄的重定向 URI：{note_text}"
    );
    assert!(note_text.contains("重定向 URI"), "{note_text}");
    assert!(note_text.contains("设备码"), "{note_text}");
    for forbidden in ["invalid_request", "AADSTS", "is not valid", "{"] {
        assert!(
            !zh_text.contains(forbidden),
            "中文界面混进了原始报文：{zh_text}"
        );
        assert!(
            !note_text.contains(forbidden),
            "词条混进了原始报文：{note_text}"
        );
    }
    // 视图那一行的来源就是这一条文案：取样函数与视图共用同一个出口（不是测试另写映射）。
    assert_eq!(
        browser_note_text(BrowserLoginNote::PreflightUnavailable, Lang::En.catalog()),
        "Could not confirm that browser sign-in is usable, so it switched to a device code."
    );
}

/// 回退之后设备码又失败：失败面板上**两者都要在**（失败原因 + 为什么没走浏览器）。
#[test]
fn a_failed_device_login_keeps_the_fallback_note() {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::DeviceLoginChanged(
        DeviceLoginView::failed_as(
            LoginFailure::Unreachable,
            "连不上 login.microsoftonline.com",
        )
        .with_browser_note(BrowserLoginNote::PreflightUnavailable),
    )));
    let page = build(&state, &Form::new());
    match &page.device {
        DevicePanel::Failed { failure, note } => {
            assert_eq!(*failure, LoginFailure::Unreachable);
            assert_eq!(*note, Some(BrowserLoginNote::PreflightUnavailable));
        }
        other => panic!("应是失败面板，实际 {other:?}"),
    }
    let text = on_screen(&page);
    assert!(!has_cjk(&text), "{text}");
    assert!(text.contains("unreachable"), "失败码要画出来：{text}");
}
