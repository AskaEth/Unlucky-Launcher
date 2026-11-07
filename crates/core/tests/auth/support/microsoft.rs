//! 微软轨的共用夹具：`MicrosoftAuth` 构造、五步链路由表、诊断断言助手。
//!
//! 从 `support/mod.rs` 拆出来（M38）：mock 服务器本体与「业务路由/断言」各自长大，
//! 分文件后两边都不顶 400 行。路由表默认用 `[0]/[3]/[5]/[6]` 下标对应
//! `xsts` / `login_with_xbox` / `entitlements/mcstore` / `profile`，用例按需替换。

use serde_json::json;
use unlucky_core::auth::microsoft::{
    MicrosoftAuth, MicrosoftEndpoints, MicrosoftSession, PollPolicy,
};
use unlucky_core::auth::{AuthError, ResponseDetail, Secret};

use super::MockReply;

/// 测试用档案 UUID（32 位无连字符）。
pub const PROFILE_ID: &str = "0f8fad5bd9cb469fa16570867728950e";
/// 测试用 client id。
pub const CLIENT_ID: &str = "test-client-id";
/// 网关/风控拦截页（HTML 403 的典型正文）。
pub const HTML_BLOCK_PAGE: &str = "<!DOCTYPE html><html><head><title>Service unavailable</title></head><body>blocked by gateway</body></html>";

/// 小间隔轮询策略（设备码轮询在测试里要秒级跑完）。
pub fn fast_poll() -> PollPolicy {
    PollPolicy {
        min_interval: std::time::Duration::from_millis(1),
        slow_down_step: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_secs(5),
    }
}

/// 一台整表指向 mock 的 `MicrosoftAuth`。
pub fn auth_for(base: &str) -> MicrosoftAuth {
    MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(MicrosoftEndpoints::all_on(base))
        .with_poll_policy(fast_poll())
}

/// 一路通到 `login_with_xbox` 的路由表（后续步骤留给用例替换）。
pub fn routes_until_login() -> Vec<(&'static str, Vec<MockReply>)> {
    vec![
        (
            "/devicecode",
            vec![MockReply::json(json!({
                "user_code": "ABCD-1234",
                "device_code": "device-code-secret-value",
                "verification_uri": "https://www.microsoft.com/link",
                "expires_in": 900,
                "interval": 0
            }))],
        ),
        (
            "/token",
            vec![MockReply::json(json!({
                "access_token": "live-access-token-value",
                "refresh_token": "live-refresh-token-value"
            }))],
        ),
        (
            "/xbl",
            vec![MockReply::json(json!({
                "Token": "xbl-token-value",
                "DisplayClaims": { "xui": [{ "uhs": "uhs-1" }] }
            }))],
        ),
        (
            "/xsts",
            vec![MockReply::json(json!({
                "Token": "xsts-token-value",
                "DisplayClaims": { "xui": [{ "uhs": "uhs-1" }] }
            }))],
        ),
    ]
}

/// 整链（含 entitlements 与 profile），便于替换中间任意一步。
pub fn full_routes() -> Vec<(&'static str, Vec<MockReply>)> {
    let mut routes = routes_until_login();
    routes.push((
        "/login_with_xbox",
        vec![MockReply::json(json!({
            "username": "Steve",
            "access_token": "mc-access-token-value",
            "expires_in": 86_400
        }))],
    ));
    routes.push((
        "/entitlements/mcstore",
        vec![MockReply::json(json!({ "items": [] }))],
    ));
    routes.push((
        "/profile",
        vec![MockReply::json(
            json!({ "id": PROFILE_ID, "name": "Steve" }),
        )],
    ));
    routes
}

/// 一个已过期的旧会话（刷新/校验用例用）。
pub fn expired_session() -> MicrosoftSession {
    MicrosoftSession::new(
        PROFILE_ID,
        "Steve",
        Secret::new("mc-access-token-1"),
        Secret::new("live-refresh-token-1"),
        3_600,
        0,
        None,
    )
}

/// 取 `AuthError::Response` 的诊断（不是这个变体就让用例炸掉，报出真实值）。
pub fn response_of(error: &AuthError) -> &ResponseDetail {
    match error {
        AuthError::Response(detail) => detail,
        other => panic!("应是 Response，实际 {other:?}"),
    }
}

/// 断言一段文案/事件里没有任何测试 token。
pub fn assert_no_credentials(text: &str) {
    for forbidden in [
        "xsts-token-value",
        "xbl-token-value",
        "live-access-token-value",
        "live-refresh-token-value",
        "device-code-secret-value",
        "mc-access-token-value",
    ] {
        assert!(
            !text.contains(forbidden),
            "文案泄漏了凭据 {forbidden}：{text}"
        );
    }
}
