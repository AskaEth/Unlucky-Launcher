//! M38：认证失败事件落盘（`.ULL/logs/auth.jsonl`）与凭据红线。
//!
//! 验收场景：`ull accounts add msa` 失败后，终端文案会滚走；日志区里要留下
//! 一行结构化记录（`code` / `detail_code` / `status` / `content_type` / `body_kind` /
//! 已脱敏摘要），且**不含任何 token**。便携数据根还没初始化时先补建 `logs/`。

use serde_json::json;
use unlucky_core::auth::{AuthError, AuthFailureEvent, AuthJournal, BodyKind};

use crate::support::{
    assert_no_credentials, auth_for, expired_session, full_routes, routes_until_login, temp_dir,
    AuthMock, MockReply, HTML_BLOCK_PAGE,
};

const JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlLXZhbHVlLXRoYXQtaXMtbG9uZy1lbm91Z2g";

#[tokio::test]
async fn body_digest_never_carries_credentials() {
    let mut routes = routes_until_login();
    routes.push((
        "/login_with_xbox",
        vec![MockReply::json_status(
            403,
            json!({ "error": "DENIED", "echo": "xsts-token-value", "jwt": JWT }),
        )],
    ));
    let mock = AuthMock::start(routes).await;
    let error = auth_for(&mock.base())
        .device_code_login(|_| {})
        .await
        .expect_err("应失败");

    let message = error.message();
    assert_no_credentials(&message);
    // 长不透明串（JWT）必须打码，别把 token 回显抄进摘要。
    assert!(!message.contains("eyJhbGci"), "JWT 必须脱敏：{message}");
    assert!(message.contains("redacted"), "{message}");

    // 事件层同样不落凭据。
    let event = AuthFailureEvent::of("device_code_login", &error);
    let json = event.to_json().to_string();
    assert_no_credentials(&json);
    assert!(!json.contains("eyJhbGci"), "{json}");
    assert!(json.contains("login_with_xbox_403"), "{json}");
    mock.stop();
}

#[tokio::test]
async fn failure_journal_initializes_logs_and_records_the_403() {
    let root = temp_dir("diag-journal");
    let logs = root.join(".ULL").join("logs");
    let journal = AuthJournal::in_logs_dir(&logs);
    assert!(!logs.exists(), "模拟未初始化的便携数据根");

    let mut routes = routes_until_login();
    routes.push((
        "/login_with_xbox",
        vec![MockReply::html(403, HTML_BLOCK_PAGE)],
    ));
    let mock = AuthMock::start(routes).await;
    let error = auth_for(&mock.base())
        .with_journal(Some(journal.clone()))
        .device_code_login(|_| {})
        .await
        .expect_err("应失败");

    let text = std::fs::read_to_string(journal.path()).expect("失败日志必须落盘");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "一次失败一条记录：{text}");
    let record: serde_json::Value = serde_json::from_str(lines[0]).expect("合法 JSONL");
    assert_eq!(record["event"], "auth_failure");
    assert_eq!(record["level"], "error");
    assert_eq!(record["scope"], "device_code_login");
    assert_eq!(record["step"], "login_with_xbox");
    assert_eq!(record["detail_code"], "login_with_xbox_non_json");
    assert_eq!(record["status"], 403);
    assert_eq!(record["body_kind"], "html");
    assert_eq!(record["transient"], true);
    assert_eq!(
        record["message"],
        error.message(),
        "事件里的文案要与返回的错误一致"
    );
    assert_no_credentials(&text);
    mock.stop();
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn journal_event_shape_covers_non_chain_errors() {
    let event = AuthFailureEvent::of("refresh", &AuthError::Canceled);
    assert_eq!(event.detail_code, "canceled");
    assert_eq!(event.step, None);
    assert_eq!(event.status, None);
    assert_eq!(event.body_kind, None);
    assert!(!event.transient);
    assert_eq!(event.to_json()["scope"], "refresh");

    let detail = unlucky_core::auth::ResponseDetail {
        step: "profile".to_string(),
        what: "拉取 Minecraft 档案被拒".to_string(),
        status: 401,
        content_type: Some("application/json".to_string()),
        kind: BodyKind::Json,
        server_message: Some("Unauthorized".to_string()),
        digest: "{\"error\":\"Unauthorized\"}".to_string(),
    };
    let chain = AuthFailureEvent::of("device_code_login", &AuthError::Response(Box::new(detail)));
    assert_eq!(chain.step.as_deref(), Some("profile"));
    assert_eq!(chain.detail_code, "profile_401");
    assert_eq!(chain.status, Some(401));
    assert!(!chain.transient, "JSON 拒绝不是可重试的网络问题");
}

#[tokio::test]
async fn successful_login_writes_no_journal_and_refresh_failures_do() {
    let root = temp_dir("diag-journal-ok");
    let journal = AuthJournal::in_logs_dir(root.join("logs"));

    // 成功：不落盘。
    let mock = AuthMock::start(full_routes()).await;
    auth_for(&mock.base())
        .with_journal(Some(journal.clone()))
        .device_code_login(|_| {})
        .await
        .expect("应登录成功");
    assert!(!journal.path().exists(), "成功路径不写失败日志");
    mock.stop();

    // 刷新失败：落一条（凭据被撤销 = 要人重新登录）。
    let failing = AuthMock::start(vec![(
        "/token",
        vec![MockReply::json_status(
            400,
            json!({ "error": "invalid_grant", "error_description": "AADSTS70000" }),
        )],
    )])
    .await;
    let error = auth_for(&failing.base())
        .with_journal(Some(journal.clone()))
        .refresh(&expired_session())
        .await
        .expect_err("应失败");
    assert!(error.is_credential_expired());
    let text = std::fs::read_to_string(journal.path()).expect("刷新失败也要落盘");
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("refresh"), "{text}");
    assert!(text.contains("credential_expired"), "{text}");
    failing.stop();
    let _ = std::fs::remove_dir_all(&root);
}
