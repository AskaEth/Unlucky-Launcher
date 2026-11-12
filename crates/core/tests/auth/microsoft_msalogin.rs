//! M178 守卫：微软登录在**官方产物**（应用注册里没有任何重定向 URI）上的行为。
//!
//! 用户 2026-11-08 实测的现象：GUI 直接走授权码流 → 授权端点回
//! `invalid_request ... 'redirect_uri' is not valid` → 浏览器里一张 AAD 原始错误页，
//! 启动器干等超时 ⇒ 官方产物上「微软登录」整体不可用。
//!
//! 本文件钉四件事（都是能给答案「能不能变红」的）：
//!
//! | 守卫 | 用例 | 抓到什么 |
//! |---|---|---|
//! | ① 回退且提示可读 | [`unregistered_redirect_falls_back_to_the_device_code`] | 回退没发生 / 原因码丢了（页面就只能画原文） |
//! | ② 不白开监听口 | 同上 + [`device_code_default_never_opens_a_listening_port`] | 预检被挪到绑定之后（`listeners_opened` 会变 1） |
//! | ③ 注册过就仍能走浏览器 | [`registered_loopback_still_runs_the_auth_code_flow`] | 把加速路彻底焊死（一刀切回退） |
//! | ④ 取消不被偷偷换流 | [`user_cancel_on_the_browser_page_does_not_fall_back`] | 取消被当成失败的后退理由 |
//!
//! 另：**真微软登录没有在这里验**（要真人拿自己账号点一次），这里只证错误处理与回退路径。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use unlucky_core::auth::microsoft::{
    AuthCodePolicy, BrowserFallbackReason, MicrosoftAuth, MicrosoftEndpoints,
};
use unlucky_core::auth::AuthError;

use crate::support::{full_routes, AuthMock, MockReply};

const CLIENT_ID: &str = "test-client-id";
/// 授权端点对**没登记**的回环 redirect_uri 的应答（2026-11-08 实测那张页面的形状）。
const INVALID_REDIRECT_PAGE: &str =
    "error=invalid_request&error_description=The+provided+value+for+the+input+\
parameter+%27redirect_uri%27+is+not+valid.+The+expected+value+is+a+URI+which+matches+a+\
+redirect+URI+registered+for+this+client+application.";
/// 授权端点对**已登记**回环 URI 的正常应答（登录页，无拒绝字样）。
const SIGN_IN_PAGE: &str = "<!DOCTYPE html><html><head><title>Sign in to your account</title>\
</head><body><form id=\"loginForm\"></form></body></html>";

/// 设备码面板里看到的 `(用户码, 回退原因码)`（跨闭包共享）。
type Seen = Arc<Mutex<Option<(String, Option<BrowserFallbackReason>)>>>;

fn policy(state: &str) -> AuthCodePolicy {
    AuthCodePolicy {
        callback_timeout: Duration::from_millis(2_000),
        bind_attempts: 1,
        port: None,
        state: Some(state.to_string()),
    }
}

/// 路由表 + 定向的 `/authorize` 应答（预检打的就是这个 path）。
fn routes_with_authorize(reply: MockReply) -> Vec<(&'static str, Vec<MockReply>)> {
    let mut routes = full_routes();
    routes.push(("/authorize", vec![reply]));
    routes
}

fn auth(base: &str, browser_first: bool) -> MicrosoftAuth {
    MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(MicrosoftEndpoints::all_on(base))
        .with_poll_policy(crate::support::microsoft::fast_poll())
        .with_authcode_policy(policy("state-abc"))
        .with_browser_first(browser_first)
}

/// 守卫 ① + ②：没登记回环 → 回退设备码、原因码是「没登记」、**监听口一次都不开**。
#[tokio::test]
async fn unregistered_redirect_falls_back_to_the_device_code() {
    let mock = AuthMock::start(routes_with_authorize(MockReply::text(
        400,
        INVALID_REDIRECT_PAGE,
    )))
    .await;

    let seen: Seen = Arc::new(Mutex::new(None));
    let seen_for_device = Arc::clone(&seen);
    let outcome = auth(&mock.base(), true)
        .browser_then_device_login(
            |_prompt| panic!("没登记回环时**不许**弹浏览器（用户只会看到 AAD 错误页）"),
            move |prompt, fallback| {
                *seen_for_device.lock().unwrap() =
                    Some((prompt.user_code.clone(), fallback.map(|note| note.reason)));
            },
        )
        .await
        .expect("必须回退到设备码并成功");

    // ① 回退确实发生，且宿主拿到的是**结构化原因码**（页面据此出可读文案，而不是画原文）
    let (user_code, reason) = seen.lock().unwrap().clone().expect("设备码必须被展示");
    assert_eq!(user_code, "ABCD-1234");
    assert_eq!(reason, Some(BrowserFallbackReason::RedirectNotRegistered));
    assert_eq!(
        reason.map(BrowserFallbackReason::code),
        Some("redirect_not_registered")
    );
    assert_eq!(outcome.session.profile_name, "Steve");
    let note = outcome.browser.expect("必须带回退说明");
    assert_eq!(note.reason, BrowserFallbackReason::RedirectNotRegistered);
    // 原文只进日志：说明里的 detail 有内容，但页面只拿码（i18n 在 app-iced 侧）。
    assert!(!note.detail.is_empty());

    // ② 预检在绑定之前：这次登录一个监听口都没开（用户机上不会出现「占了端口却没人回调」）
    assert_eq!(
        outcome.listeners_opened, 0,
        "没登记回环时不许绑本地端口（预检必须在绑定之前）"
    );
    // ③ **报文级证据**：预检真的发出去了（授权端点收到 1 次 GET），而不是被跳过。
    //    这条也是复审 P1-1 的回执：只要生产路径把 `browser_first` 置位，这条 GET 就会发生。
    assert_eq!(
        mock.bodies_on("/authorize").len(),
        1,
        "浏览器优先路必须真的发一次回环注册预检"
    );
    mock.stop();
}

/// 守卫 ②（默认路那一半）：设备码是默认，浏览器流根本没被启用 ⇒ 监听计数不涨。
#[tokio::test]
async fn device_code_default_never_opens_a_listening_port() {
    let mock = AuthMock::start(full_routes()).await;
    let device_seen = Arc::new(Mutex::new(None));
    let device_flag = Arc::clone(&device_seen);

    let outcome = auth(&mock.base(), false)
        .browser_then_device_login(
            |_| panic!("默认路不许碰浏览器回调"),
            move |prompt, fallback| {
                *device_flag.lock().unwrap() = Some(prompt.user_code.clone());
                assert!(fallback.is_none(), "没走浏览器就不该有回退说明");
            },
        )
        .await
        .expect("设备码登录成功");

    assert_eq!(
        device_seen.lock().unwrap().clone().as_deref(),
        Some("ABCD-1234")
    );
    assert!(outcome.browser.is_none());
    assert_eq!(
        outcome.session.profile_id,
        crate::support::microsoft::PROFILE_ID
    );
    assert_eq!(outcome.listeners_opened, 0, "设备码流不许开任何本地监听口");
    // 授权端点一次都没被请求（那正是会返回 invalid_request 的那一跳）
    assert!(mock.bodies_on("/authorize").is_empty());
    mock.stop();
}

/// 守卫 ③：回环**真的登记了**的时候，加速路必须还能走（否则就是把功能一刀切焊死）。
#[tokio::test]
async fn registered_loopback_still_runs_the_auth_code_flow() {
    let mock = AuthMock::start(routes_with_authorize(MockReply::html(200, SIGN_IN_PAGE))).await;

    let outcome = auth(&mock.base(), true)
        .browser_then_device_login(
            |prompt| {
                let redirect = prompt.redirect_uri.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    let url = format!("{redirect}?code=the-auth-code&state=state-abc");
                    let response = reqwest::get(&url).await.expect("浏览器回调");
                    assert_eq!(response.status(), 200);
                });
            },
            |_prompt, _note| panic!("登记了回环就不该回退设备码"),
        )
        .await
        .expect("授权码流应当走通");

    assert_eq!(outcome.session.profile_name, "Steve");
    assert!(outcome.browser.is_none(), "走通了就不该有回退说明");
    assert_eq!(
        outcome.listeners_opened, 1,
        "登记了回环就应当真的绑端口（否则加速路名存实亡）"
    );
    // 换 token 用的是**授权码** grant（不是设备码那条）。
    let token_bodies = mock.bodies_on("/token");
    assert_eq!(token_bodies.len(), 1, "{token_bodies:?}");
    assert!(token_bodies[0].contains("grant_type=authorization_code"));
    mock.stop();
}

/// 守卫 ④：用户自己取消**不算**失败——不许偷偷换一条流继续要码。
#[tokio::test]
async fn user_cancel_on_the_browser_page_does_not_fall_back() {
    let mock = AuthMock::start(routes_with_authorize(MockReply::html(200, SIGN_IN_PAGE))).await;
    let error = auth(&mock.base(), true)
        .browser_then_device_login(
            |prompt| {
                let redirect = prompt.redirect_uri.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    let url = format!("{redirect}?error=access_denied&state=state-abc");
                    let _ = reqwest::get(&url).await;
                });
            },
            |_prompt, _note| panic!("用户取消后不许自动换流"),
        )
        .await
        .expect_err("取消应当是最终结果");
    assert_eq!(error, AuthError::Canceled);
    assert!(
        mock.bodies_on("/devicecode").is_empty(),
        "取消之后不该再去要设备码"
    );
    mock.stop();
}

/// 预检问不出结论（连不上）也要回退，且原因码是「没确认」而不是「没登记」。
#[tokio::test]
async fn an_unreachable_precheck_falls_back_with_the_unverified_reason() {
    // 只有授权端点指到一个必然关闭的端口：预检必然失败，而设备码那条链照常打 mock。
    let mock = AuthMock::start(full_routes()).await;
    let mut endpoints = MicrosoftEndpoints::all_on(&mock.base());
    endpoints.authorize = "http://127.0.0.1:1/authorize".to_string();
    let reason = Arc::new(Mutex::new(None));
    let reason_flag = Arc::clone(&reason);
    let outcome = MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(endpoints)
        .with_poll_policy(crate::support::microsoft::fast_poll())
        .with_authcode_policy(policy("state-abc"))
        .with_browser_first(true)
        .browser_then_device_login(
            |_prompt| panic!("预检失败时不许弹浏览器"),
            move |_prompt, fallback| {
                *reason_flag.lock().unwrap() = fallback.map(|note| note.reason);
            },
        )
        .await
        .expect("预检失败不该拦住设备码");
    assert_eq!(
        *reason.lock().unwrap(),
        Some(BrowserFallbackReason::PreflightUnavailable)
    );
    assert_eq!(
        outcome.browser.map(|note| note.reason),
        Some(BrowserFallbackReason::PreflightUnavailable)
    );
    assert_eq!(outcome.session.profile_name, "Steve");
    assert_eq!(outcome.listeners_opened, 0, "预检失败时不该绑端口");
    mock.stop();
}

/// 与上两例配对：`/authorize` 回 200 但正文是 AADSTS50011 的拒绝页，也不许当「已登记」。
#[tokio::test]
async fn aadsts50011_page_is_not_treated_as_registered() {
    let mock = AuthMock::start(routes_with_authorize(MockReply::text(
        200,
        "AADSTS50011: The redirect URI specified in the request does not match the redirect URIs configured",
    )))
    .await;
    let note = Arc::new(Mutex::new(None));
    let note_flag = Arc::clone(&note);
    let outcome = auth(&mock.base(), true)
        .browser_then_device_login(
            |_| panic!("AADSTS50011 的页面不许当已登记"),
            move |_prompt, fallback| {
                *note_flag.lock().unwrap() = fallback.map(|note| note.reason);
            },
        )
        .await
        .expect("回退设备码后应当成功");
    assert_eq!(
        *note.lock().unwrap(),
        Some(BrowserFallbackReason::RedirectNotRegistered)
    );
    assert_eq!(
        outcome.browser.map(|note| note.reason),
        Some(BrowserFallbackReason::RedirectNotRegistered)
    );
    mock.stop();
}
