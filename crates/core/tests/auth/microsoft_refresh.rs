//! refresh 语义与 validate 分支：轮换、换号防线、404 二义、拥有权门禁。

use std::time::Duration;

use serde_json::json;
use unlucky_core::auth::microsoft::{
    MicrosoftAuth, MicrosoftEndpoints, MicrosoftSession, PollPolicy, RefreshDecision, RefreshReason,
};
use unlucky_core::auth::Secret;

use crate::support::{AuthMock, MockReply};

const PROFILE_ID: &str = "0f8fad5bd9cb469fa16570867728950e";
const OTHER_PROFILE_ID: &str = "11111111111111111111111111111111";
const CLIENT_ID: &str = "test-client-id";

fn auth_for(base: &str) -> MicrosoftAuth {
    MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(MicrosoftEndpoints::all_on(base))
        .with_poll_policy(PollPolicy {
            min_interval: Duration::from_millis(1),
            slow_down_step: Duration::from_millis(1),
            max_wait: Duration::from_secs(5),
        })
}

/// 一个「已过期」的旧会话（`not_after` 早于当前时刻）。
fn expired_session() -> MicrosoftSession {
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

/// 一个「新鲜」的会话。
fn fresh_session() -> MicrosoftSession {
    MicrosoftSession::new(
        PROFILE_ID,
        "Steve",
        Secret::new("mc-access-token-1"),
        Secret::new("live-refresh-token-1"),
        86_400,
        unlucky_core::auth::microsoft::now_ms(),
        None,
    )
}

/// 刷新链（Live refresh + 五步）。
fn refresh_routes(profile_id: &str, license: bool) -> Vec<(&'static str, Vec<MockReply>)> {
    vec![
        (
            "/token",
            vec![MockReply::json(json!({
                "access_token": "live-access-token-2",
                "refresh_token": "live-refresh-token-2"
            }))],
        ),
        (
            "/xbl",
            vec![MockReply::json(json!({
                "Token": "xbl-token-2",
                "DisplayClaims": { "xui": [{ "uhs": "uhs-2" }] }
            }))],
        ),
        (
            "/xsts",
            vec![MockReply::json(json!({
                "Token": "xsts-token-2",
                "DisplayClaims": { "xui": [{ "uhs": "uhs-2" }] }
            }))],
        ),
        (
            "/login_with_xbox",
            vec![MockReply::json(json!({
                "username": "Steve",
                "access_token": "mc-access-token-2",
                "expires_in": 86_400
            }))],
        ),
        (
            "/entitlements/mcstore",
            vec![MockReply::json(json!({ "items": [] }))],
        ),
        (
            "/entitlements/license",
            vec![MockReply::json(if license {
                json!({ "items": [{ "name": "game_minecraft" }] })
            } else {
                json!({ "items": [{ "name": "product_minecraft" }] })
            })],
        ),
        (
            "/profile",
            vec![MockReply::json(
                json!({ "id": profile_id, "name": "Steve" }),
            )],
        ),
    ]
}

#[test]
fn refresh_decision_table() {
    let expired = expired_session();
    assert_eq!(
        expired.refresh_decision(unlucky_core::auth::microsoft::now_ms()),
        RefreshDecision::Refresh(RefreshReason::Expired)
    );
    let fresh = fresh_session();
    assert_eq!(
        fresh.refresh_decision(unlucky_core::auth::microsoft::now_ms()),
        RefreshDecision::Reuse
    );
}

#[tokio::test]
async fn refresh_rotates_tokens_and_keeps_profile() {
    let mock = AuthMock::start(refresh_routes(PROFILE_ID, true)).await;
    let auth = auth_for(&mock.base());
    let old = expired_session();
    let (refreshed, reason) = auth.ensure_fresh(&old).await.expect("刷新成功");

    assert_eq!(reason, Some(RefreshReason::Expired));
    assert_eq!(refreshed.profile_id, PROFILE_ID);
    // 轮换：access 与 refresh **都**换新（refresh 返回新 refresh_token）。
    assert_eq!(refreshed.access_token.expose(), "mc-access-token-2");
    assert_eq!(refreshed.refresh_token.expose(), "live-refresh-token-2");
    assert_ne!(refreshed.access_token, old.access_token);
    assert_ne!(refreshed.refresh_token, old.refresh_token);
    assert!(refreshed.not_after_ms > old.not_after_ms);

    // 刷新报文：拿旧 refresh_token 走 refresh_token grant。
    let body = &mock.bodies_on("/token")[0];
    assert!(body.contains("grant_type=refresh_token"), "{body}");
    assert!(
        body.contains("refresh_token=live-refresh-token-1"),
        "{body}"
    );
    // 整链重跑：XBL 用的是**新的** Live access token。
    let xbl = &mock.bodies_on("/xbl")[0];
    assert!(xbl.contains("d=live-access-token-2"), "{xbl}");
    mock.stop();
}

#[tokio::test]
async fn fresh_session_needs_no_network() {
    let mock = AuthMock::start(vec![("/token", vec![MockReply::status(500)])]).await;
    let auth = auth_for(&mock.base());
    let (session, reason) = auth.ensure_fresh(&fresh_session()).await.expect("直接复用");
    assert!(reason.is_none());
    assert_eq!(session.access_token.expose(), "mc-access-token-1");
    assert!(mock.requests().is_empty(), "不该打网络");
    mock.stop();
}

#[tokio::test]
async fn refresh_that_changes_profile_is_rejected() {
    let mock = AuthMock::start(refresh_routes(OTHER_PROFILE_ID, true)).await;
    let auth = auth_for(&mock.base());
    let error = auth.refresh(&expired_session()).await.expect_err("应拒绝");
    assert_eq!(error.code(), "malformed");
    assert!(error.message().contains("档案变了"), "{}", error.message());
    mock.stop();
}

#[tokio::test]
async fn revoked_refresh_token_requires_relogin() {
    let mock = AuthMock::start(vec![(
        "/token",
        vec![MockReply::json_status(
            400,
            json!({
                "error": "invalid_grant",
                "error_description": "AADSTS70000: The grant is expired or revoked"
            }),
        )],
    )])
    .await;
    let auth = auth_for(&mock.base());
    let error = auth.refresh(&expired_session()).await.expect_err("应失败");
    assert!(error.is_credential_expired(), "{error:?}");
    mock.stop();
}

#[tokio::test]
async fn validate_reports_token_state_without_swallowing_profile_errors() {
    // 200 → 有效
    let ok = AuthMock::start(vec![(
        "/profile",
        vec![MockReply::json(
            json!({ "id": PROFILE_ID, "name": "Steve" }),
        )],
    )])
    .await;
    assert!(auth_for(&ok.base())
        .validate(&fresh_session())
        .await
        .expect("有效"));
    ok.stop();

    // 401 → token 失效（返回值，不是错误）
    let unauthorized = AuthMock::start(vec![("/profile", vec![MockReply::status(401)])]).await;
    assert!(!auth_for(&unauthorized.base())
        .validate(&fresh_session())
        .await
        .expect("失效"));
    unauthorized.stop();

    // 404 + 没有 game_minecraft → 「没买游戏」是真实错误，不当 token 失效
    let no_license = AuthMock::start(vec![
        ("/profile", vec![MockReply::status(404)]),
        (
            "/entitlements/license",
            vec![MockReply::json(
                json!({ "items": [{ "name": "product_minecraft" }] }),
            )],
        ),
    ])
    .await;
    let error = auth_for(&no_license.base())
        .validate(&fresh_session())
        .await
        .expect_err("应上抛");
    assert_eq!(error.code(), "denied");
    assert!(error.message().contains("没有购买"), "{}", error.message());
    no_license.stop();

    // 404 + 有 game_minecraft → 「买了没建档案」
    let no_profile = AuthMock::start(vec![
        ("/profile", vec![MockReply::status(404)]),
        (
            "/entitlements/license",
            vec![MockReply::json(
                json!({ "items": [{ "name": "game_minecraft" }] }),
            )],
        ),
    ])
    .await;
    let error = auth_for(&no_profile.base())
        .validate(&fresh_session())
        .await
        .expect_err("应上抛");
    assert!(
        error.message().contains("还没有创建档案"),
        "{}",
        error.message()
    );
    no_profile.stop();
}

#[tokio::test]
async fn missing_entitlement_fails_the_chain_early() {
    let mut routes = refresh_routes(PROFILE_ID, true);
    routes[4].1 = vec![MockReply::status(404)];
    let mock = AuthMock::start(routes).await;
    let auth = auth_for(&mock.base());
    let error = auth.refresh(&expired_session()).await.expect_err("应失败");
    match error {
        unlucky_core::auth::AuthError::Denied { code, .. } => assert_eq!(code, "no_entitlement"),
        other => panic!("应是 Denied，实际 {other:?}"),
    }
    mock.stop();
}

#[tokio::test]
async fn uhs_mismatch_between_xbl_and_xsts_is_malformed() {
    let mut routes = refresh_routes(PROFILE_ID, true);
    routes[2].1 = vec![MockReply::json(json!({
        "Token": "xsts-token-2",
        "DisplayClaims": { "xui": [{ "uhs": "uhs-diFFERENT" }] }
    }))];
    let mock = AuthMock::start(routes).await;
    let auth = auth_for(&mock.base());
    let error = auth.refresh(&expired_session()).await.expect_err("应失败");
    assert_eq!(error.code(), "malformed");
    assert!(error.message().contains("userhash"), "{}", error.message());
    mock.stop();
}
