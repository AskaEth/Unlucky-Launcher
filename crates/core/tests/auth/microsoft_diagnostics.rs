//! M38：非 2xx 应答正文诊断（`AuthError::Response`）。
//!
//! 覆盖 MSA 真机验收里分不开的那几类：
//! - **JSON 403**：真实服务拒绝（服务端文案直出，引导查许可/区域）；
//! - **HTML 403**：网关/风控拦截页（引导查网络/代理，独立细分码 `*_non_json`）；
//! - **空体 403**：同样按「不确定」分流（`*_empty_body`）；
//! - 4xx / 5xx 各一，外加 XErr 优先级与 JSON 码的向后兼容。
//!
//! 失败日志（JSONL）与凭据红线在同一目录的 `auth_failure_journal.rs`。

use serde_json::json;
use unlucky_core::auth::{AuthError, BodyKind};

use crate::support::{
    assert_no_credentials, auth_for, full_routes, response_of, routes_until_login, AuthMock,
    MockReply, HTML_BLOCK_PAGE,
};

#[tokio::test]
async fn json_403_surfaces_server_message_and_points_at_license() {
    let mut routes = routes_until_login();
    routes.push((
        "/login_with_xbox",
        vec![MockReply::json_status(
            403,
            json!({
                "path": "/authentication/login_with_xbox",
                "error": "UNAUTHORIZED",
                "errorMessage": "You do not own Minecraft Java Edition"
            }),
        )],
    ));
    let mock = AuthMock::start(routes).await;
    let error = auth_for(&mock.base())
        .device_code_login(|_| {})
        .await
        .expect_err("应失败");

    assert_eq!(error.code(), "denied", "粗分流层保持 M31 的 denied");
    assert_eq!(error.detail_code(), "login_with_xbox_403");
    assert!(!error.is_transient(), "服务端明确拒绝，重试无意义");
    let detail = response_of(&error);
    assert_eq!(detail.status, 403);
    assert_eq!(detail.kind, BodyKind::Json);
    assert_eq!(detail.content_type.as_deref(), Some("application/json"));
    let message = error.message();
    // 服务端文案直出（验收要看到真实原因）。
    assert!(message.contains("You do not own"), "{message}");
    assert!(message.contains("UNAUTHORIZED"), "{message}");
    assert!(message.contains("许可"), "应引导查许可：{message}");
    assert!(
        !message.contains("代理"),
        "JSON 拒绝不该往网络引：{message}"
    );
    assert!(message.contains("响应摘要"), "{message}");
    assert_no_credentials(&message);
    mock.stop();
}

#[tokio::test]
async fn html_403_is_routed_to_network_and_proxy() {
    let mut routes = routes_until_login();
    routes.push((
        "/login_with_xbox",
        vec![MockReply::html(403, HTML_BLOCK_PAGE)],
    ));
    let mock = AuthMock::start(routes).await;
    let error = auth_for(&mock.base())
        .device_code_login(|_| {})
        .await
        .expect_err("应失败");

    assert_eq!(error.code(), "denied");
    assert_eq!(
        error.detail_code(),
        "login_with_xbox_non_json",
        "拦截页要有独立细分码"
    );
    assert!(error.is_transient(), "拦截页可以换代理重试");
    let detail = response_of(&error);
    assert_eq!(detail.kind, BodyKind::Html);
    assert!(detail.is_interception());
    assert_eq!(detail.status, 403);
    assert!(
        detail
            .content_type
            .as_deref()
            .is_some_and(|value| value.contains("text/html")),
        "{:?}",
        detail.content_type
    );
    let message = error.message();
    assert!(message.contains("响应非 JSON"), "{message}");
    assert!(message.contains("网络"), "{message}");
    assert!(message.contains("代理"), "{message}");
    assert!(
        message.contains("blocked by gateway"),
        "摘要要留下证据：{message}"
    );
    assert!(
        !message.contains("许可"),
        "拦截页不该把用户引向许可：{message}"
    );
    assert_no_credentials(&message);
    mock.stop();
}

#[tokio::test]
async fn empty_403_reports_the_empty_body() {
    let mut routes = routes_until_login();
    routes.push(("/login_with_xbox", vec![MockReply::status(403)]));
    let mock = AuthMock::start(routes).await;
    let error = auth_for(&mock.base())
        .device_code_login(|_| {})
        .await
        .expect_err("应失败");

    assert_eq!(error.detail_code(), "login_with_xbox_empty_body");
    let detail = response_of(&error);
    assert_eq!(detail.kind, BodyKind::Empty);
    assert!(detail.digest.is_empty());
    assert!(error.message().contains("为空"), "{}", error.message());
    mock.stop();
}

#[tokio::test]
async fn profile_4xx_keeps_the_step_status_code() {
    let mut routes = full_routes();
    routes[6].1 = vec![MockReply::json_status(
        401,
        json!({ "path": "/minecraft/profile", "error": "Unauthorized" }),
    )];
    let mock = AuthMock::start(routes).await;
    let error = auth_for(&mock.base())
        .device_code_login(|_| {})
        .await
        .expect_err("应失败");
    assert_eq!(error.detail_code(), "profile_401");
    assert!(error.message().contains("档案"), "{}", error.message());
    assert_no_credentials(&error.message());
    mock.stop();
}

#[tokio::test]
async fn entitlements_5xx_text_is_an_interception_not_a_license_verdict() {
    let mut routes = full_routes();
    routes[5].1 = vec![MockReply::text(503, "Service Unavailable")];
    let mock = AuthMock::start(routes).await;
    let error = auth_for(&mock.base())
        .device_code_login(|_| {})
        .await
        .expect_err("应失败");
    // 关键：网关页不能报成「你没买游戏」（M31 的 no_entitlement 只留给 JSON/空体）。
    assert_eq!(error.detail_code(), "entitlements_non_json");
    let detail = response_of(&error);
    assert_eq!(detail.kind, BodyKind::Text);
    assert_eq!(detail.status, 503);
    assert!(
        error.message().contains("Service Unavailable"),
        "{}",
        error.message()
    );
    mock.stop();
}

#[tokio::test]
async fn entitlements_404_with_empty_body_still_reports_no_entitlement() {
    let mut routes = full_routes();
    routes[5].1 = vec![MockReply::status(404)];
    let mock = AuthMock::start(routes).await;
    let error = auth_for(&mock.base())
        .device_code_login(|_| {})
        .await
        .expect_err("应失败");
    // 向后兼容：M31 的稳定码 `no_entitlement` 不动。
    match error {
        AuthError::Denied { code, description } => {
            assert_eq!(code, "no_entitlement");
            assert!(description.contains("404"), "{description}");
        }
        other => panic!("应是 Denied，实际 {other:?}"),
    }
    mock.stop();
}

#[tokio::test]
async fn entitlements_403_with_empty_body_is_not_a_license_verdict() {
    let mut routes = full_routes();
    routes[5].1 = vec![MockReply::status(403)];
    let mock = AuthMock::start(routes).await;
    let error = auth_for(&mock.base())
        .device_code_login(|_| {})
        .await
        .expect_err("应失败");
    // 403 且没正文：说不清是许可还是拦截，按「不确定」报，不下「没买游戏」的结论。
    assert_eq!(error.detail_code(), "entitlements_empty_body");
    mock.stop();
}

#[tokio::test]
async fn xsts_401_without_xerr_still_reports_body_context() {
    let mut routes = routes_until_login();
    routes[3].1 = vec![MockReply::html(401, HTML_BLOCK_PAGE)];
    let mock = AuthMock::start(routes).await;
    let error = auth_for(&mock.base())
        .device_code_login(|_| {})
        .await
        .expect_err("应失败");
    assert_eq!(error.detail_code(), "xsts_non_json");
    assert!(error.message().contains("XSTS"), "{}", error.message());
    mock.stop();
}

#[tokio::test]
async fn xerr_still_wins_over_the_body_diagnostic() {
    let mut routes = routes_until_login();
    routes[3].1 = vec![MockReply::json_status(
        401,
        json!({ "XErr": 2_148_916_235_u32, "Message": "country unavailable" }),
    )];
    let mock = AuthMock::start(routes).await;
    let error = auth_for(&mock.base())
        .device_code_login(|_| {})
        .await
        .expect_err("应失败");
    match error {
        AuthError::Xbox { code } => assert_eq!(code, 2_148_916_235),
        other => panic!("XErr 判定必须优先，实际 {other:?}"),
    }
    mock.stop();
}
