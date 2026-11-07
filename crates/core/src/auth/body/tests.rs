//! [`super`] 的单测：分类、服务端文案提取、摘要脱敏与 403 分流文案。

use super::{
    advice, classify, digest, one_line, response, server_message, truncate, BodyKind, DIGEST_CHARS,
};
use crate::auth::error::AuthError;
use crate::auth::http::Reply;
use crate::auth::secret::{redactor_for, Secret};
use crate::launch::redact::Redactor;
use serde_json::json;

const JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlLXZhbHVlLXRoYXQtaXMtbG9uZy1lbm91Z2g";

fn json_reply(status: u16, body: serde_json::Value) -> Reply {
    Reply {
        status,
        body: Some(body),
        content_type: Some("application/json".to_string()),
        raw: None,
    }
}

fn text_reply(status: u16, content_type: Option<&str>, raw: &str) -> Reply {
    Reply {
        status,
        body: None,
        content_type: content_type.map(str::to_string),
        raw: Some(raw.to_string()),
    }
}

#[test]
fn classification_splits_json_html_text_and_empty() {
    assert_eq!(
        classify(&json_reply(403, json!({ "error": "x" }))),
        BodyKind::Json
    );
    assert_eq!(
        classify(&text_reply(403, Some("text/html"), "<!DOCTYPE html>")),
        BodyKind::Html
    );
    // 没给 Content-Type 时靠正文形状兜底。
    assert_eq!(
        classify(&text_reply(403, None, "<html><body>blocked")),
        BodyKind::Html
    );
    assert_eq!(
        classify(&text_reply(502, Some("text/plain"), "Bad Gateway")),
        BodyKind::Text
    );
    assert_eq!(classify(&text_reply(403, None, "   ")), BodyKind::Empty);
}

#[test]
fn server_message_prefers_error_fields_and_dedups() {
    let redactor = Redactor::new();
    let reply = json_reply(
        403,
        json!({ "error": "UNAUTHORIZED", "errorMessage": "no license", "path": "/x" }),
    );
    let message = server_message(&reply, &redactor).expect("应有服务端文案");
    assert!(message.contains("no license"), "{message}");
    assert!(message.contains("UNAUTHORIZED"), "{message}");

    let same = json_reply(403, json!({ "error": "DENIED", "errorMessage": "DENIED" }));
    assert_eq!(server_message(&same, &redactor).as_deref(), Some("DENIED"));
    assert!(server_message(&json_reply(403, json!({ "path": "/x" })), &redactor).is_none());
    assert!(server_message(&text_reply(500, None, "boom"), &redactor).is_none());
}

#[test]
fn digest_is_redacted_and_capped() {
    let token = Secret::new("mc-access-token-value");
    let redactor = redactor_for([&token]);
    let reply = json_reply(
        403,
        json!({ "error": "DENIED", "echo": format!("Bearer {}", token.expose()) }),
    );
    let masked_digest = digest(&reply, &redactor);
    assert!(
        !masked_digest.contains("mc-access-token-value"),
        "{masked_digest}"
    );

    let long = "blocked ".repeat(60);
    let capped = digest(&text_reply(500, Some("text/plain"), &long), &redactor);
    assert!(capped.chars().count() <= DIGEST_CHARS);
    assert!(capped.ends_with('…'));
}

#[test]
fn opaque_runs_are_masked_even_without_known_secret() {
    let masked = digest(&text_reply(401, Some("text/plain"), JWT), &Redactor::new());
    assert!(!masked.contains("eyJhbGci"), "{masked}");
    assert!(masked.contains("redacted"), "{masked}");
}

#[test]
fn dotted_tokens_are_masked_by_the_shared_redactor() {
    // 规则本体在 `launch::redact`（M40 上提），这里只验「auth 摘要走的是同一份实现」。
    let redactor = Redactor::new();
    assert!(!redactor.scrub(JWT).contains("eyJhbGci"), "JWT 应被打码");
    // 普通文案里的点不会触发（分段太短）。
    assert_eq!(
        redactor.scrub("Service.Unavailable.today"),
        "Service.Unavailable.today"
    );
    let masked = redactor.scrub(&format!("token={JWT}&end")).into_owned();
    assert!(!masked.contains("eyJhbGci"), "{masked}");
    assert!(masked.ends_with("&end"), "{masked}");
}

#[test]
fn one_line_collapses_whitespace() {
    assert_eq!(one_line("  a\n\n b\tc  "), "a b c");
    assert_eq!(truncate("abcdef", 4), "abc…");
    assert_eq!(truncate("abc", 4), "abc");
}

#[test]
fn detail_codes_and_advice_split_network_from_license() {
    let redactor = Redactor::new();
    let blocked = response(
        "login_with_xbox",
        &text_reply(403, Some("text/html"), "<html>blocked</html>"),
        "Minecraft 登录被拒",
        &redactor,
    );
    let AuthError::Response(detail) = &blocked else {
        panic!("应是 Response，实际 {blocked:?}");
    };
    assert_eq!(detail.detail_code(), "login_with_xbox_non_json");
    assert!(
        detail.message().contains("网络拦截"),
        "{}",
        detail.message()
    );
    assert!(detail.message().contains("代理"), "{}", detail.message());

    let denied = response(
        "login_with_xbox",
        &json_reply(
            403,
            json!({ "error": "UNAUTHORIZED", "errorMessage": "nope" }),
        ),
        "Minecraft 登录被拒",
        &redactor,
    );
    let AuthError::Response(detail) = &denied else {
        panic!("应是 Response，实际 {denied:?}");
    };
    assert_eq!(detail.detail_code(), "login_with_xbox_403");
    assert!(detail.message().contains("许可"), "{}", detail.message());
    assert!(!detail.message().contains("代理"), "{}", detail.message());

    let empty = response(
        "profile",
        &text_reply(500, None, ""),
        "拉取档案被拒",
        &redactor,
    );
    let AuthError::Response(detail) = &empty else {
        panic!("应是 Response，实际 {empty:?}");
    };
    assert_eq!(detail.detail_code(), "profile_empty_body");
    assert!(!advice("xbl", BodyKind::Html).is_empty());
    assert!(!advice("xbl", BodyKind::Json).is_empty());
}
