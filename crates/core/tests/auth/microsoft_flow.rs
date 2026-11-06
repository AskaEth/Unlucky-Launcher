//! device-code 全链（打到进程内 mock 服务器）：报文形状 + 轮询状态机 + 错误分流。

use std::time::Duration;

use serde_json::json;
use unlucky_core::auth::microsoft::{
    DeviceCodePrompt, MicrosoftAuth, MicrosoftEndpoints, PollPolicy,
};
use unlucky_core::auth::AuthError;

use crate::support::{AuthMock, MockReply};

const PROFILE_ID: &str = "0f8fad5bd9cb469fa16570867728950e";
const CLIENT_ID: &str = "test-client-id";

fn fast_poll() -> PollPolicy {
    PollPolicy {
        min_interval: Duration::from_millis(1),
        slow_down_step: Duration::from_millis(1),
        max_wait: Duration::from_secs(5),
    }
}

fn device_code_reply() -> MockReply {
    MockReply::json(json!({
        "user_code": "ABCD-1234",
        "device_code": "device-code-secret-value",
        "verification_uri": "https://www.microsoft.com/link",
        "expires_in": 900,
        "interval": 0,
        "message": "go to the url"
    }))
}

fn chain_replies() -> Vec<(&'static str, Vec<MockReply>)> {
    vec![
        ("/devicecode", vec![device_code_reply()]),
        (
            "/token",
            vec![
                MockReply::json(json!({ "error": "authorization_pending" })),
                MockReply::json(json!({
                    "access_token": "live-access-token-value",
                    "refresh_token": "live-refresh-token-value"
                })),
            ],
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
        (
            "/login_with_xbox",
            vec![MockReply::json(json!({
                "username": "Steve",
                "access_token": "mc-access-token-value",
                "token_type": "Bearer",
                "expires_in": 86_400
            }))],
        ),
        (
            "/entitlements/mcstore",
            vec![MockReply::json(json!({ "items": [] }))],
        ),
        (
            "/profile",
            vec![MockReply::json(
                json!({ "id": PROFILE_ID, "name": "Steve" }),
            )],
        ),
    ]
}

#[tokio::test]
async fn device_code_login_runs_the_whole_chain() {
    let mock = AuthMock::start(chain_replies()).await;
    let auth = MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(MicrosoftEndpoints::all_on(&mock.base()))
        .with_poll_policy(fast_poll());

    let mut prompted: Option<DeviceCodePrompt> = None;
    let session = auth
        .device_code_login(|prompt| prompted = Some(prompt.clone()))
        .await
        .expect("登录成功");

    // 1. 用户码/URL 在轮询前就交给宿主了（CLI 才能先打印再等）。
    let prompt = prompted.expect("必须回调展示用户码");
    assert_eq!(prompt.user_code, "ABCD-1234");
    assert_eq!(prompt.verification_uri, "https://www.microsoft.com/link");
    assert_eq!(prompt.expires_in, Duration::from_secs(900));
    assert_eq!(prompt.interval, Duration::ZERO, "服务端 interval 原样收下");
    let (line1, line2) = prompt.lines();
    assert!(line1.contains("https://www.microsoft.com/link"));
    assert!(line2.contains("ABCD-1234"));

    // 2. 会话字段：MC token 落 access_token，Live refresh 落 refresh_token。
    assert_eq!(session.profile_id, PROFILE_ID);
    assert_eq!(session.profile_name, "Steve");
    assert_eq!(session.access_token.expose(), "mc-access-token-value");
    assert_eq!(session.refresh_token.expose(), "live-refresh-token-value");
    assert!(session.not_after_ms > 1_577_836_800_000);
    assert!(session.has_profile_name());
    // Debug 里不能出现任何 token。
    let debug = format!("{session:?}");
    assert!(!debug.contains("mc-access-token-value"), "{debug}");
    assert!(!debug.contains("live-refresh-token-value"), "{debug}");

    // 3. 报文形状（这几处抄错就会 400，必须逐条盯住）。
    let device_bodies = mock.bodies_on("/devicecode");
    assert_eq!(device_bodies.len(), 1);
    assert!(device_bodies[0].contains("client_id=test-client-id"));
    assert!(
        device_bodies[0].contains("scope=XboxLive.signin+offline_access")
            || device_bodies[0].contains("scope=XboxLive.signin%20offline_access")
    );

    let token_bodies = mock.bodies_on("/token");
    assert_eq!(token_bodies.len(), 2, "pending 一次后成功");
    for body in &token_bodies {
        assert!(body.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"));
        assert!(body.contains("code=device-code-secret-value"));
        assert!(!body.contains("{"), "表单体不该是 JSON：{body}");
    }

    let xbl = &mock.bodies_on("/xbl")[0];
    // `d=` 前缀是 MSA 消费版票据标记（漏了就 400）。
    assert!(
        xbl.contains("\"RpsTicket\":\"d=live-access-token-value\""),
        "{xbl}"
    );
    assert!(xbl.contains("user.auth.xboxlive.com"));

    let xsts = &mock.bodies_on("/xsts")[0];
    assert!(xsts.contains("rp://api.minecraftservices.com/"), "{xsts}");
    assert!(xsts.contains("\"SandboxId\":\"RETAIL\""), "{xsts}");
    assert!(xsts.contains("xbl-token-value"), "{xsts}");

    let login = &mock.bodies_on("/login_with_xbox")[0];
    assert!(
        login.contains("XBL3.0 x=uhs-1;xsts-token-value"),
        "identityToken 拼接错：{login}"
    );

    mock.stop();
}

#[tokio::test]
async fn slow_down_backs_off_instead_of_failing() {
    let mut routes = chain_replies();
    // 把 /token 换成 pending → slow_down → 成功。
    routes[1].1 = vec![
        MockReply::json(json!({ "error": "authorization_pending" })),
        MockReply::json(json!({ "error": "slow_down" })),
        MockReply::json(json!({
            "access_token": "live-access-token-value",
            "refresh_token": "live-refresh-token-value"
        })),
    ];
    let mock = AuthMock::start(routes).await;
    let auth = MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(MicrosoftEndpoints::all_on(&mock.base()))
        .with_poll_policy(fast_poll());
    let session = auth.device_code_login(|_| {}).await.expect("登录成功");
    assert_eq!(session.profile_name, "Steve");
    assert_eq!(mock.bodies_on("/token").len(), 3, "三次轮询");
    mock.stop();
}

#[tokio::test]
async fn access_denied_is_reported_as_canceled() {
    let mut routes = chain_replies();
    routes[1].1 = vec![MockReply::json(json!({ "error": "access_denied" }))];
    let mock = AuthMock::start(routes).await;
    let auth = MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(MicrosoftEndpoints::all_on(&mock.base()))
        .with_poll_policy(fast_poll());
    let error = auth.device_code_login(|_| {}).await.expect_err("应取消");
    assert_eq!(error, AuthError::Canceled);
    assert_eq!(error.code(), "canceled");
    mock.stop();
}

#[tokio::test]
async fn expired_device_code_is_a_timeout() {
    let mut routes = chain_replies();
    routes[1].1 = vec![MockReply::json(json!({ "error": "expired_token" }))];
    let mock = AuthMock::start(routes).await;
    let auth = MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(MicrosoftEndpoints::all_on(&mock.base()))
        .with_poll_policy(fast_poll());
    let error = auth.device_code_login(|_| {}).await.expect_err("应超时");
    assert_eq!(error.code(), "timeout");
    mock.stop();
}

#[tokio::test]
async fn polling_gives_up_after_the_wait_budget() {
    let mut routes = chain_replies();
    let pending = MockReply::json(json!({ "error": "authorization_pending" }));
    routes[1].1 = vec![pending; 64];
    let mock = AuthMock::start(routes).await;
    let auth = MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(MicrosoftEndpoints::all_on(&mock.base()))
        .with_poll_policy(PollPolicy {
            min_interval: Duration::from_millis(1),
            slow_down_step: Duration::from_millis(1),
            max_wait: Duration::from_millis(20),
        });
    let error = auth.device_code_login(|_| {}).await.expect_err("应超时");
    assert_eq!(error.code(), "timeout");
    assert!(mock.bodies_on("/token").len() < 64, "超时后必须停止轮询");
    mock.stop();
}

#[tokio::test]
async fn xsts_401_with_xerr_is_mapped() {
    let mut routes = chain_replies();
    routes[2].1 = vec![MockReply::json_status(
        401,
        json!({ "Identity": "0", "XErr": 2_148_916_233_u32, "Message": "no xbox account" }),
    )];
    let mock = AuthMock::start(routes).await;
    let auth = MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(MicrosoftEndpoints::all_on(&mock.base()))
        .with_poll_policy(fast_poll());
    let error = auth.device_code_login(|_| {}).await.expect_err("应失败");
    match error {
        AuthError::Xbox { code } => {
            assert_eq!(code, 2_148_916_233);
            assert!(unlucky_core::auth::error::xbox_hint(code).contains("Xbox 档案"));
        }
        other => panic!("应是 Xbox 错误，实际 {other:?}"),
    }
    mock.stop();
}

#[tokio::test]
async fn device_code_endpoint_failure_is_denied() {
    let mock = AuthMock::start(vec![(
        "/devicecode",
        vec![MockReply::json_status(
            400,
            json!({ "error": "invalid_client", "error_description": "bad client id" }),
        )],
    )])
    .await;
    let auth = MicrosoftAuth::new(CLIENT_ID)
        .expect("构造")
        .with_endpoints(MicrosoftEndpoints::all_on(&mock.base()))
        .with_poll_policy(fast_poll());
    let error = auth.device_code_login(|_| {}).await.expect_err("应失败");
    match error {
        AuthError::Denied { code, .. } => assert_eq!(code, "invalid_client"),
        other => panic!("应是 Denied，实际 {other:?}"),
    }
    mock.stop();
}
