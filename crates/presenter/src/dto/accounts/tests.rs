//! 账户 DTO 的契约测试（凭据红线的主要断言都在这里）。

use super::*;

#[test]
fn kind_round_trips_through_stable_ids() {
    for kind in [
        AccountKind::Offline,
        AccountKind::Microsoft,
        AccountKind::Authlib,
    ] {
        assert_eq!(AccountKind::parse(kind.as_str()), kind);
        assert!(!kind.label().is_empty());
    }
}

#[test]
fn device_login_view_never_carries_tokens() {
    let view = DeviceLoginView::waiting("ABCD-EFGH", "https://www.microsoft.com/link", 900, "");
    assert_eq!(view.phase, DeviceLoginPhase::Waiting);
    assert!(view.phase.busy());
    assert!(view.qr_link.contains("ABCD-EFGH"));
    let json = serde_json::to_string(&view).expect("序列化");
    for forbidden in ["token", "secret", "password", "refresh"] {
        assert!(
            !json.to_lowercase().contains(forbidden),
            "视图里不该出现 {forbidden}"
        );
    }
}

#[test]
fn browser_waiting_shows_a_copyable_link_instead_of_a_user_code() {
    let view = DeviceLoginView::browser_waiting(
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?client_id=x",
        300,
        Some("拉起浏览器失败"),
    );
    assert_eq!(view.phase, DeviceLoginPhase::Waiting);
    assert!(view.phase.busy());
    assert!(view.user_code.is_empty(), "授权码流没有用户码");
    assert_eq!(view.qr_link, view.verification_uri);
    assert!(view.message.contains("浏览器"));
    assert!(view.message.contains("拉起浏览器失败"), "{}", view.message);
    let json = serde_json::to_string(&view).expect("序列化");
    for forbidden in ["refresh", "mc-access"] {
        assert!(!json.contains(forbidden), "视图里不该出现 {forbidden}");
    }
}

#[test]
fn succeeded_and_failed_clear_the_code() {
    let ok = DeviceLoginView::succeeded("Steve");
    assert_eq!(ok.phase, DeviceLoginPhase::Succeeded);
    assert!(ok.user_code.is_empty());
    let bad = DeviceLoginView::failed("网络超时");
    assert_eq!(bad.phase, DeviceLoginPhase::Failed);
    assert_eq!(bad.message, "网络超时");
}

/// 凭据红线（审核项）：JWT 形状串从**任何**入口进来都必须被抹掉。
///
/// 抓得住：`redact` 被摘掉、判据被放宽成「长度 ≥40 就行」、或者某个构造器绕过它。
/// 抓不住：真实凭据的明文值（那是 `core::launch::redact` 的活儿，按已知值替换）。
#[test]
fn jwt_shaped_text_is_redacted_at_every_entry_point() {
    let jwt =
        "eyJhbGciOiJSUzI1NiIsIng1dCI6IkFCQyJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlLXZhbHVl";

    let device = DeviceLoginView::failed(format!("登录失败：accessToken={jwt}"));
    assert!(!device.message.contains(jwt), "{}", device.message);
    assert!(device.message.contains(REDACTED), "{}", device.message);

    let authlib = AuthlibLoginView::failed(format!("服务器回包异常 {jwt}"));
    assert!(!authlib.message.contains(jwt), "{}", authlib.message);
    assert!(authlib.message.contains(REDACTED), "{}", authlib.message);

    let ready = AuthlibLoginView::reachable("https://littleskin.cn/", "LittleSkin");
    assert!(!ready.message.contains(jwt));
}

#[test]
fn authlib_request_never_prints_the_password() {
    let request = AuthlibRequest::new(
        "https://littleskin.cn/api/yggdrasil/",
        "me@example.com",
        "hunter2",
    );
    let shown = request.redacted();
    assert!(!shown.contains("hunter2"), "{shown}");
    assert!(shown.contains("littleskin"), "{shown}");
    assert!(shown.contains(REDACTED), "{shown}");
}

/// `Debug` 也不许带出密码（复审 r1 P3-5）：日志一行 `{:?}` 就破红线。
#[test]
fn authlib_request_debug_never_prints_the_password() {
    let request = AuthlibRequest::new("https://littleskin.cn/api/yggdrasil/", "me", "hunter2");
    let shown = format!("{request:?}");
    assert!(!shown.contains("hunter2"), "{shown}");
    assert!(shown.contains(REDACTED), "{shown}");
    assert!(shown.contains("littleskin"), "{shown}");
}

#[test]
fn authlib_view_carries_the_server_name_and_no_tokens() {
    let view = AuthlibLoginView::reachable("https://littleskin.cn/api/yggdrasil/", "LittleSkin");
    assert_eq!(view.phase, DeviceLoginPhase::Waiting);
    assert_eq!(view.server_name, "LittleSkin");
    assert!(view.message.contains("LittleSkin"));

    let ok = AuthlibLoginView::succeeded("LittleSkin", "Steve");
    assert_eq!(ok.phase, DeviceLoginPhase::Succeeded);
    assert_eq!(ok.account_name, "Steve");
    let json = serde_json::to_string(&ok).expect("序列化");
    for forbidden in ["accessToken", "clientToken", "refresh", "password"] {
        assert!(!json.contains(forbidden), "视图里不该出现 {forbidden}");
    }
}

#[test]
fn account_view_serializes_without_credentials() {
    let view = AccountView {
        id: "account:a".to_string(),
        name: "Steve".to_string(),
        kind: AccountKind::Authlib,
        profile_id: "0f8f".to_string(),
        login_name: "me@example.com".to_string(),
        created: "2026-10-31 20:12".to_string(),
        active: true,
        server_url: "https://littleskin.cn/api/yggdrasil/".to_string(),
        skin: Some(SkinImage {
            png: vec![1, 2, 3],
            source: "textures".to_string(),
        }),
    };
    let json = serde_json::to_string(&view).expect("序列化");
    for forbidden in ["token", "secret", "password", "refresh"] {
        assert!(
            !json.to_lowercase().contains(forbidden),
            "账户视图里不该出现 {forbidden}"
        );
    }
    assert!(json.contains("\"png\""));
}

#[test]
fn monogram_is_stable_and_never_empty() {
    let mut view = AccountView {
        id: "account:a".to_string(),
        name: "  steve".to_string(),
        kind: AccountKind::Microsoft,
        profile_id: "0f8f".to_string(),
        login_name: String::new(),
        created: String::new(),
        active: false,
        server_url: String::new(),
        skin: None,
    };
    assert_eq!(view.monogram(), "S");
    let tone = view.monogram_tone();
    assert!(tone < 6);
    view.profile_id = "0f8f".to_string();
    assert_eq!(view.monogram_tone(), tone, "同一个人每次同一个色块");
    view.name = String::new();
    assert_eq!(view.monogram(), "?", "空名字也要有字，不能画白块");
}
