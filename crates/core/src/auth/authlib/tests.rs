//! authlib 轨的纯函数/模型测试（HTTP 部分在 `tests/auth/authlib_api.rs`）。

use super::{normalize_api_root, AuthlibEndpoints, AuthlibProfile, AuthlibSession, USER_TYPE_MSA};
use crate::auth::secret::Secret;

#[test]
fn api_root_normalization() {
    assert_eq!(
        normalize_api_root("littleskin.cn/api/yggdrasil").expect("补协议"),
        "https://littleskin.cn/api/yggdrasil/"
    );
    assert_eq!(
        normalize_api_root("  http://127.0.0.1:8080/api/yggdrasil/  ").expect("保留协议"),
        "http://127.0.0.1:8080/api/yggdrasil/"
    );
    assert_eq!(
        normalize_api_root("https://littleskin.cn").expect("根域名"),
        "https://littleskin.cn/"
    );
    assert!(normalize_api_root("   ").is_err());
    assert!(normalize_api_root("https://").is_err());
}

#[test]
fn endpoints_follow_directory_layout() {
    let endpoints =
        AuthlibEndpoints::from_api_root("https://example.invalid/api/yggdrasil").expect("端点");
    assert_eq!(
        endpoints.authenticate,
        "https://example.invalid/api/yggdrasil/authserver/authenticate"
    );
    assert_eq!(
        endpoints.refresh,
        "https://example.invalid/api/yggdrasil/authserver/refresh"
    );
    assert_eq!(
        endpoints.validate,
        "https://example.invalid/api/yggdrasil/authserver/validate"
    );
    assert_eq!(
        endpoints.invalidate,
        "https://example.invalid/api/yggdrasil/authserver/invalidate"
    );
}

#[test]
fn placeholders_require_a_profile() {
    let without = AuthlibSession {
        access_token: Secret::new("token"),
        client_token: Secret::new("client"),
        selected_profile: None,
    };
    assert!(!without.has_profile());
    assert!(without.to_placeholders().is_err());

    let with = AuthlibSession {
        selected_profile: Some(AuthlibProfile {
            id: "0123456789abcdef0123456789abcdef".to_string(),
            name: "Steve".to_string(),
        }),
        ..without
    };
    assert!(with.has_profile());
    let placeholders = with.to_placeholders().expect("占位符");
    assert_eq!(placeholders.player_name, "Steve");
    assert_eq!(placeholders.uuid, "0123456789abcdef0123456789abcdef");
    assert_eq!(placeholders.user_type, USER_TYPE_MSA);
    assert_eq!(placeholders.access_token, "token");
}

#[test]
fn session_debug_never_leaks_tokens() {
    let session = AuthlibSession {
        access_token: Secret::new("access-token-value"),
        client_token: Secret::new("client-token-value"),
        selected_profile: None,
    };
    let debug = format!("{session:?}");
    assert!(!debug.contains("access-token-value"));
    assert!(!debug.contains("client-token-value"));
    assert_eq!(session.secrets().len(), 2);
}

#[test]
fn session_survives_json_roundtrip() {
    let session = AuthlibSession {
        access_token: Secret::new("access-token-value"),
        client_token: Secret::new("client-token-value"),
        selected_profile: Some(AuthlibProfile {
            id: "0123456789abcdef0123456789abcdef".to_string(),
            name: "Steve".to_string(),
        }),
    };
    let text = serde_json::to_string(&session).expect("序列化");
    let back: AuthlibSession = serde_json::from_str(&text).expect("反序列化");
    assert_eq!(back, session);
}
