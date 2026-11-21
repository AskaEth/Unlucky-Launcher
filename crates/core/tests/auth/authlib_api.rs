//! Yggdrasil（authlib-injector）四动作的报文级对接：登录 / 刷新 / 校验 / 注销。

use serde_json::json;
use unlucky_core::auth::authlib::{
    AuthlibAuth, AuthlibEndpoints, AuthlibProfile, AuthlibServerMeta, AuthlibSession,
};
use unlucky_core::auth::{AuthError, Secret};

use crate::support::{AuthMock, MockReply};

const API_ROOT: &str = "https://example.invalid/api/yggdrasil/";
const PROFILE_ID: &str = "0123456789abcdef0123456789abcdef";

fn paths() -> (&'static str, &'static str, &'static str, &'static str) {
    (
        "/api/yggdrasil/authserver/authenticate",
        "/api/yggdrasil/authserver/refresh",
        "/api/yggdrasil/authserver/validate",
        "/api/yggdrasil/authserver/invalidate",
    )
}

/// 把 API 根改指到 mock（`base` 代替 https://example.invalid）。
fn mock_root(mock: &AuthMock) -> String {
    format!("{}/api/yggdrasil", mock.base())
}

fn session() -> AuthlibSession {
    AuthlibSession {
        access_token: Secret::new("access-token-value"),
        client_token: Secret::new("client-token-value"),
        selected_profile: Some(AuthlibProfile {
            id: PROFILE_ID.to_string(),
            name: "Steve".to_string(),
        }),
    }
}

#[tokio::test]
async fn authenticate_sends_the_canonical_body() {
    let (authenticate, _, _, _) = paths();
    let mock = AuthMock::start(vec![(
        authenticate,
        vec![MockReply::json(json!({
            "accessToken": "access-token-value",
            "clientToken": "client-token-value",
            "selectedProfile": { "id": PROFILE_ID, "name": "Steve" }
        }))],
    )])
    .await;
    let auth = AuthlibAuth::new().expect("构造");
    let login = auth
        .authenticate(
            &mock_root(&mock),
            "steve@example.com",
            &Secret::new("hunter2-password"),
            &Secret::new("client-token-value"),
        )
        .await
        .expect("登录成功");

    assert_eq!(login.access_token.expose(), "access-token-value");
    assert_eq!(login.client_token.expose(), "client-token-value");
    assert!(login.has_profile());
    assert_eq!(
        login.selected_profile.as_ref().map(|p| p.name.as_str()),
        Some("Steve")
    );

    let body = &mock.bodies_on(authenticate)[0];
    assert!(
        body.contains("\"agent\":{\"name\":\"Minecraft\",\"version\":1}"),
        "{body}"
    );
    assert!(body.contains("\"requestUser\":true"), "{body}");
    assert!(
        body.contains("\"username\":\"steve@example.com\""),
        "{body}"
    );
    assert!(
        body.contains("\"clientToken\":\"client-token-value\""),
        "{body}"
    );
    // 密码确实发了（否则登录不了），但会话 Debug 里看不到它。
    assert!(body.contains("hunter2-password"));
    assert!(!format!("{login:?}").contains("hunter2-password"));
    mock.stop();
}

#[tokio::test]
async fn wrong_password_is_denied_with_server_message() {
    let (authenticate, _, _, _) = paths();
    let mock = AuthMock::start(vec![(
        authenticate,
        vec![MockReply::json_status(
            403,
            json!({
                "error": "ForbiddenOperationException",
                "errorMessage": "Invalid credentials. Invalid username or password."
            }),
        )],
    )])
    .await;
    let auth = AuthlibAuth::new().expect("构造");
    let error = auth
        .authenticate(
            &mock_root(&mock),
            "steve",
            &Secret::new("wrong"),
            &Secret::new("client-token-value"),
        )
        .await
        .expect_err("应失败");
    match error {
        AuthError::Denied { code, description } => {
            assert_eq!(code, "ForbiddenOperationException");
            assert!(description.contains("Invalid credentials"));
        }
        other => panic!("应是 Denied，实际 {other:?}"),
    }
    mock.stop();
}

#[tokio::test]
async fn server_must_echo_the_client_token() {
    let (authenticate, _, _, _) = paths();
    let mock = AuthMock::start(vec![(
        authenticate,
        vec![MockReply::json(json!({
            "accessToken": "access-token-value",
            "clientToken": "someone-elses-token",
            "selectedProfile": { "id": PROFILE_ID, "name": "Steve" }
        }))],
    )])
    .await;
    let auth = AuthlibAuth::new().expect("构造");
    let error = auth
        .authenticate(
            &mock_root(&mock),
            "steve",
            &Secret::new("pw"),
            &Secret::new("client-token-value"),
        )
        .await
        .expect_err("应拒绝");
    assert_eq!(error.code(), "malformed");
    assert!(
        error.message().contains("clientToken"),
        "{}",
        error.message()
    );
    mock.stop();
}

#[tokio::test]
async fn refresh_can_select_a_character() {
    let (_, refresh, _, _) = paths();
    let mock = AuthMock::start(vec![(
        refresh,
        vec![MockReply::json(json!({
            "accessToken": "access-token-2",
            "clientToken": "client-token-value",
            "selectedProfile": { "id": PROFILE_ID, "name": "Alex" }
        }))],
    )])
    .await;
    let auth = AuthlibAuth::new().expect("构造");
    let wanted = AuthlibProfile {
        id: PROFILE_ID.to_string(),
        name: "Alex".to_string(),
    };
    let refreshed = auth
        .refresh(&mock_root(&mock), &session(), Some(&wanted))
        .await
        .expect("刷新成功");
    assert_eq!(refreshed.access_token.expose(), "access-token-2");
    assert_eq!(refreshed.selected_profile.expect("角色").name, "Alex");
    // 新的 refresh_token 不存在于 Yggdrasil：clientToken 保持不变。
    assert_eq!(refreshed.client_token.expose(), "client-token-value");

    let body = &mock.bodies_on(refresh)[0];
    assert!(body.contains("\"selectedProfile\":{\"id\":\"0123456789abcdef0123456789abcdef\""));
    assert!(body.contains("\"accessToken\":\"access-token-value\""));
    mock.stop();
}

#[tokio::test]
async fn refresh_that_did_not_switch_character_is_malformed() {
    let (_, refresh, _, _) = paths();
    let mock = AuthMock::start(vec![(
        refresh,
        vec![MockReply::json(json!({
            "accessToken": "access-token-2",
            "clientToken": "client-token-value",
            "selectedProfile": { "id": PROFILE_ID, "name": "Steve" }
        }))],
    )])
    .await;
    let auth = AuthlibAuth::new().expect("构造");
    let wanted = AuthlibProfile {
        id: "ffffffffffffffffffffffffffffffff".to_string(),
        name: "Ghost".to_string(),
    };
    let error = auth
        .refresh(&mock_root(&mock), &session(), Some(&wanted))
        .await
        .expect_err("应失败");
    assert_eq!(error.code(), "malformed");
    mock.stop();
}

#[tokio::test]
async fn forbidden_on_refresh_means_credential_expired() {
    let (_, refresh, _, _) = paths();
    let mock = AuthMock::start(vec![(
        refresh,
        vec![MockReply::json_status(
            403,
            json!({ "error": "ForbiddenOperationException", "errorMessage": "Invalid token." }),
        )],
    )])
    .await;
    let auth = AuthlibAuth::new().expect("构造");
    let error = auth
        .refresh(&mock_root(&mock), &session(), None)
        .await
        .expect_err("应失败");
    assert!(error.is_credential_expired(), "{error:?}");
    mock.stop();
}

#[tokio::test]
async fn validate_distinguishes_invalid_token_from_server_trouble() {
    let (_, _, validate, _) = paths();
    // 空体 200 → 有效
    let ok = AuthMock::start(vec![(validate, vec![MockReply::status(200)])]).await;
    let auth = AuthlibAuth::new().expect("构造");
    assert!(auth
        .validate(&mock_root(&ok), &session())
        .await
        .expect("有效"));
    ok.stop();

    // 403 ForbiddenOperationException → 无效（返回值 false，不抛错）
    let invalid = AuthMock::start(vec![(
        validate,
        vec![MockReply::json_status(
            403,
            json!({ "error": "ForbiddenOperationException", "errorMessage": "Invalid token." }),
        )],
    )])
    .await;
    assert!(!auth
        .validate(&mock_root(&invalid), &session())
        .await
        .expect("无效"));
    invalid.stop();

    // 500 → 服务器问题，必须抛错（调用方才会重试而不是刷新）
    let broken = AuthMock::start(vec![(validate, vec![MockReply::status(500)])]).await;
    let error = auth
        .validate(&mock_root(&broken), &session())
        .await
        .expect_err("应失败");
    assert_eq!(error.code(), "network");
    broken.stop();
}

#[tokio::test]
async fn invalidate_tolerates_already_invalid_tokens() {
    let (_, _, _, invalidate) = paths();
    let auth = AuthlibAuth::new().expect("构造");
    let ok = AuthMock::start(vec![(invalidate, vec![MockReply::status(200)])]).await;
    auth.invalidate(&mock_root(&ok), &session())
        .await
        .expect("注销成功");
    ok.stop();

    let gone = AuthMock::start(vec![(
        invalidate,
        vec![MockReply::json_status(
            403,
            json!({ "error": "ForbiddenOperationException" }),
        )],
    )])
    .await;
    auth.invalidate(&mock_root(&gone), &session())
        .await
        .expect("已失效也算成功");
    gone.stop();
}

#[test]
fn endpoints_and_metadata_stay_in_sync_with_the_protocol() {
    let endpoints = AuthlibEndpoints::from_api_root(API_ROOT).expect("端点");
    assert_eq!(
        endpoints.authenticate,
        format!("{API_ROOT}authserver/authenticate")
    );
    let meta = AuthlibServerMeta::from_value(&json!({
        "meta": { "serverName": "Example", "feature.non_email_login": true },
        "skinDomains": ["example.invalid"]
    }));
    assert_eq!(meta.name.as_deref(), Some("Example"));
    assert!(meta.non_email_login);
    assert_eq!(meta.skin_domains, vec!["example.invalid".to_string()]);
}
