//! 授权码流（`research/08` §3 方案 A）：本地回调服务器 + 授权 URL + code 换链。
//!
//! 覆盖验收口径：授权码换到 MS token → XBL → XSTS 全通；白名单未批前
//! `login_with_xbox` 的 403 只要求**诊断文案正确**（M38 版），不绕过。

use std::time::Duration;

use serde_json::json;
use unlucky_core::auth::microsoft::{
    AuthCodePolicy, AuthCodePrompt, CallbackServer, MicrosoftAuth, MicrosoftEndpoints,
};
use unlucky_core::auth::AuthError;

use crate::support::{full_routes, response_of, AuthMock, MockReply};

const CLIENT_ID: &str = "test-client-id";

fn policy(state: &str, timeout_ms: u64) -> AuthCodePolicy {
    AuthCodePolicy {
        callback_timeout: Duration::from_millis(timeout_ms),
        bind_attempts: 3,
        port: None,
        state: Some(state.to_string()),
    }
}

/// 模拟浏览器：把 `code` 送到本地回调地址（真发一次 HTTP，走 localhost 解析）。
async fn deliver(redirect_uri: &str, query: &str) {
    let url = format!("{redirect_uri}?{query}");
    // 回调服务器可能在 accept 中，给一拍让绑定先落地。
    tokio::time::sleep(Duration::from_millis(10)).await;
    let response = reqwest::get(&url).await.expect("浏览器回调");
    assert_eq!(response.status(), 200, "回调页应回 200");
    let page = response.text().await.expect("回调页正文");
    assert!(page.contains("登录完成"), "回调页应是给用户看的完成页");
}

#[tokio::test]
async fn authcode_login_completes_the_chain() {
    let mock = AuthMock::start(full_routes()).await;
    let auth = MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(MicrosoftEndpoints::all_on(&mock.base()))
        .with_authcode_policy(policy("state-abc", 5_000));

    let mut prompted: Option<AuthCodePrompt> = None;
    let session = auth
        .authcode_login(|prompt| {
            prompted = Some(prompt.clone());
            let redirect = prompt.redirect_uri.clone();
            tokio::spawn(
                async move { deliver(&redirect, "code=the-auth-code&state=state-abc").await },
            );
        })
        .await
        .expect("授权码登录成功");

    // 1. 宿主拿到的是「授权 URL + 本地回调地址」，两者端口必须一致。
    let prompt = prompted.expect("必须回调展示授权 URL");
    let redirect = prompt.redirect_uri.clone();
    assert!(redirect.starts_with("http://localhost:"), "{redirect}");
    assert!(redirect.ends_with("/auth"), "{redirect}");
    assert!(prompt.authorization_url.contains("prompt=select_account"));
    // 端点表在测试里整体指向 mock，所以授权页的路径是 `/authorize`（租户口径见向量单测）。
    assert!(
        prompt.authorization_url.starts_with(&mock.base())
            && prompt
                .authorization_url
                .contains("/authorize?client_id=test-client-id"),
        "{}",
        prompt.authorization_url
    );
    assert!(prompt.authorization_url.contains("state=state-abc"));
    assert_eq!(prompt.timeout, Duration::from_millis(5_000));

    // 2. code 换 token 的表单：grant_type + 原样重定向 URI（URI 严格一致那条坑）。
    let token_bodies = mock.bodies_on("/token");
    assert_eq!(token_bodies.len(), 1, "只该换一次 token");
    let body = &token_bodies[0];
    assert!(body.contains("grant_type=authorization_code"), "{body}");
    assert!(body.contains("code=the-auth-code"), "{body}");
    assert!(
        body.contains("redirect_uri=http%3A%2F%2Flocalhost%3A") && !body.contains(&redirect),
        "redirect_uri 必须是编码后的同一字面量：{body}"
    );
    assert!(
        body.contains("scope=XboxLive.signin+offline_access")
            || body.contains("scope=XboxLive.signin%20offline_access"),
        "{body}"
    );
    // 授权 URL 里的 redirect_uri 与 token 请求里的必须是同一个端口。
    let encoded = redirect.replace(':', "%3A").replace('/', "%2F");
    assert!(
        prompt.authorization_url.contains(&encoded),
        "{}",
        prompt.authorization_url
    );
    assert!(body.contains(&encoded), "{body}");

    // 3. 后链与会话：与设备码流同一套字段语义。
    assert_eq!(session.profile_name, "Steve");
    assert_eq!(session.access_token.expose(), "mc-access-token-value");
    assert_eq!(session.refresh_token.expose(), "live-refresh-token-value");
    assert!(!format!("{session:?}").contains("mc-access-token-value"));
    assert_eq!(mock.bodies_on("/xbl").len(), 1);
    assert_eq!(mock.bodies_on("/xsts").len(), 1);
    mock.stop();
}

#[tokio::test]
async fn callback_server_ignores_unrelated_requests_and_bad_state() {
    let server = CallbackServer::bind(None, 1, "state-ok")
        .await
        .expect("绑定");
    let port = server.port();
    let browser = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let base = format!("http://localhost:{port}");
        // favicon / 浏览器预热：404 但绝不能结束等待。
        let favicon = reqwest::get(format!("{base}/favicon.ico"))
            .await
            .expect("favicon");
        assert_eq!(favicon.status(), 404);
        // state 不匹配（旧标签页的迟到回调）：400，继续等。
        let stale = reqwest::get(format!("{base}/auth?code=stale-code&state=state-old"))
            .await
            .expect("迟到的回调");
        assert_eq!(stale.status(), 400);
        let good = reqwest::get(format!("{base}/auth?code=good-code&state=state-ok"))
            .await
            .expect("真回调");
        assert_eq!(good.status(), 200);
    });

    let code = server
        .wait_for_code(Duration::from_secs(5))
        .await
        .expect("应拿到 code");
    assert_eq!(code, "good-code");
    browser.await.expect("模拟浏览器任务");
}

#[tokio::test]
async fn callback_timeout_is_reported() {
    let auth = MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_authcode_policy(policy("state-abc", 30));
    let error = auth
        .authcode_login(|_prompt| {})
        .await
        .expect_err("没人回调就该超时");
    assert_eq!(error.code(), "timeout");
    assert!(
        error.message().contains("浏览器回调"),
        "{}",
        error.message()
    );
}

#[tokio::test]
async fn access_denied_on_the_callback_page_is_canceled() {
    let auth = MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_authcode_policy(policy("state-abc", 5_000));
    let error = auth
        .authcode_login(|prompt| {
            let redirect = prompt.redirect_uri.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                let url = format!("{redirect}?error=access_denied&state=state-abc");
                let _ = reqwest::get(&url).await;
            });
        })
        .await
        .expect_err("用户取消");
    assert_eq!(error, AuthError::Canceled);
}

#[tokio::test]
async fn busy_port_retries_then_reports_which_port() {
    // 先占住一个端口，再让回调服务器去绑同一个：必须失败并说清是哪个端口。
    let occupied = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("占端口");
    let port = occupied.local_addr().expect("地址").port();
    let error = CallbackServer::bind(Some(port), 2, "s")
        .await
        .expect_err("端口已被占");
    assert_eq!(error.code(), "network");
    assert!(
        error.message().contains(&port.to_string()),
        "文案要指出端口：{}",
        error.message()
    );
    assert!(error.message().contains("重试 2 次"), "{}", error.message());
    // 同一台机器上换个端口就能绑上（重试机制不是把错误吞掉）。
    let ok = CallbackServer::bind(None, 1, "s").await.expect("内核分配");
    assert_ne!(ok.port(), port);
}

#[tokio::test]
async fn login_with_xbox_403_keeps_the_m38_diagnostic() {
    // 白名单未批的真机状态：授权码换到 token、XBL/XSTS 全通，最后一步 403。
    let mut routes = full_routes();
    let (_, login) = routes
        .iter_mut()
        .find(|(path, _)| *path == "/login_with_xbox")
        .expect("路由表里有 login_with_xbox");
    *login = vec![MockReply::json_status(
        403,
        json!({ "path": "/authentication/login_with_xbox", "errorType": "FORBIDDEN",
                "error": "Invalid app registration" }),
    )];
    let mock = AuthMock::start(routes).await;
    let auth = MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(MicrosoftEndpoints::all_on(&mock.base()))
        .with_authcode_policy(policy("state-abc", 5_000));

    let error = auth
        .authcode_login(|prompt| {
            let redirect = prompt.redirect_uri.clone();
            tokio::spawn(async move { deliver(&redirect, "code=c&state=state-abc").await });
        })
        .await
        .expect_err("403 必须冒出来");

    let detail = response_of(&error);
    assert_eq!(detail.status, 403);
    assert_eq!(detail.detail_code(), "login_with_xbox_403");
    // 「授权码这条流本身是通的」——这正是验收止步的位置。
    assert_eq!(mock.bodies_on("/token").len(), 1);
    assert_eq!(mock.bodies_on("/xbl").len(), 1);
    assert_eq!(mock.bodies_on("/xsts").len(), 1);
    let message = error.message();
    assert!(message.contains("Minecraft 登录被拒"), "{message}");
    assert!(message.contains("Invalid app registration"), "{message}");
    mock.stop();
}
