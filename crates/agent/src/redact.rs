//! 脱敏：审计日志的参数、以及日志工具返回文本的**第二道**防线。
//!
//! 第一道在 launch 落盘时（`launch-接口约定.md` §4：`<redacted:token>` / `<home>` /
//! `<redacted:opaque>`），所以这里不重复实现那套；本模块只兜两件事：
//! 1. 审计日志里的参数可能含 API key（`set_config` 能改 AI 端点）→ 按键名掩码；
//! 2. 日志正文万一含用户主目录绝对路径（旧会话、第三方 mod 输出）→ 掩码用户名段。
//!
//! 幂等：对已脱敏文本再跑一遍结果不变（`<home>` / `***` 原样保留）。

use serde_json::{Map, Value};

/// 掩码占位。
pub const MASK: &str = "***";

/// 键名是否敏感（不区分大小写，含子串即算）。
///
/// 覆盖：`api_key` / `apikey` / `accessToken` / `session_token` / `password` /
/// `secret` / `authorization` / `credential`。
#[must_use]
pub fn is_sensitive_key(key: &str) -> bool {
    const NEEDLES: [&str; 8] = [
        "api_key",
        "apikey",
        "token",
        "password",
        "passwd",
        "secret",
        "authorization",
        "credential",
    ];
    let lower = key.to_ascii_lowercase();
    NEEDLES.iter().any(|n| lower.contains(n))
}

/// 审计用：递归把敏感键的值换成 [`MASK`]（保留结构，读审计的人仍能看出改的是哪个键）。
///
/// 除了「键名本身敏感」之外，还认 `set_config {path, value}` 这种**值随路径敏感**的形状：
/// 当同级的 `path` 是敏感键名（`ai.api_key`）时，`value`/`old`/`new` 一律掩码——
/// 否则审计日志会把 API key 原文写进磁盘。
#[must_use]
pub fn redact_args(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sensitive_path = map
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(is_sensitive_key);
            let mut out = Map::with_capacity(map.len());
            for (k, v) in map {
                let mask = is_sensitive_key(k)
                    || (sensitive_path && matches!(k.as_str(), "value" | "old" | "new"));
                let masked = if mask {
                    Value::String(MASK.to_string())
                } else {
                    redact_args(v)
                };
                out.insert(k.clone(), masked);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_args).collect()),
        other => other.clone(),
    }
}

/// 文本脱敏：用户主目录段 + `Bearer` 令牌。已脱敏文本原样返回（幂等）。
#[must_use]
pub fn redact_text(text: &str) -> String {
    let masked = mask_user_dirs(text);
    mask_bearer(&masked)
}

/// 任意 JSON 的每个字符串都过 [`redact_text`]（工具结果出口的第二道防线）。
///
/// 与 [`redact_args`] 的分工：那个按键名掩码（审计参数），这个按**内容**掩码
/// （日志正文、账户展示名之类可能夹带主目录/Bearer 的字符串）。
#[must_use]
pub fn redact_texts(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(redact_text(text)),
        Value::Array(items) => Value::Array(items.iter().map(redact_texts).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, child)| (key.clone(), redact_texts(child)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `C:\Users\<名>` / `/home/<名>` / `/Users/<名>` → 用户名段替换为 [`MASK`]。
fn mask_user_dirs(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    while cursor < text.len() {
        let Some((start, marker_len)) = find_marker(&lower, cursor) else {
            out.push_str(&text[cursor..]);
            break;
        };
        out.push_str(&text[cursor..start + marker_len]);
        let name_start = start + marker_len;
        let name_end = segment_end(text, name_start);
        let segment = &text[name_start..name_end];
        if segment.is_empty() || segment == MASK || segment.eq_ignore_ascii_case("<home>") {
            out.push_str(segment);
        } else {
            out.push_str(MASK);
        }
        cursor = name_end;
    }
    out
}

/// 找最靠前的用户目录标记，返回 `(字节位置, 标记长度)`。标记须与 `lower` 同源。
fn find_marker(lower: &str, from: usize) -> Option<(usize, usize)> {
    const MARKERS: [&str; 3] = ["c:\\users\\", "/home/", "/users/"];
    MARKERS
        .iter()
        .filter_map(|m| lower[from..].find(m).map(|p| (from + p, m.len())))
        .min_by_key(|(start, _)| *start)
}

/// 从 `from` 起取到下一个路径分隔符/空白/引号为止。
fn segment_end(text: &str, from: usize) -> usize {
    const STOPS: [char; 9] = ['\\', '/', '"', '\'', ' ', '\t', '\n', '\r', ','];
    text[from..]
        .find(STOPS)
        .map_or(text.len(), |offset| from + offset)
}

/// `Bearer <token>` → `Bearer ***`。
fn mask_bearer(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    while let Some(pos) = text[cursor..].find("Bearer ") {
        let token_start = cursor + pos + "Bearer ".len();
        let token_end = segment_end(text, token_start);
        out.push_str(&text[cursor..token_start]);
        if token_end > token_start {
            out.push_str(MASK);
        }
        cursor = token_end;
    }
    out.push_str(&text[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn masks_sensitive_arg_keys_only() {
        let args = json!({
            "path": "launcher.theme",
            "value": "light",
            "nested": { "accessToken": "abc", "keep": 1 },
            "list": [{ "password": "p" }]
        });
        let red = redact_args(&args);
        // 非敏感路径：值原样保留（审计要能看出改成了什么）
        assert_eq!(red["value"], json!("light"));
        assert_eq!(red["path"], json!("launcher.theme"));
        // 键名敏感：值掩码
        assert_eq!(red["nested"]["accessToken"], json!(MASK));
        assert_eq!(red["nested"]["keep"], json!(1));
        assert_eq!(red["list"][0]["password"], json!(MASK));
    }

    #[test]
    fn masks_value_when_path_itself_is_sensitive() {
        // `set_config {path: "ai.api_key", value: "sk-…"}` 是最容易漏的泄漏点
        let args = json!({ "path": "ai.api_key", "value": "sk-super-secret" });
        let red = redact_args(&args);
        assert_eq!(red["value"], json!(MASK));
        let rendered = red.to_string();
        assert!(!rendered.contains("sk-super-secret"), "{rendered}");
    }

    #[test]
    fn masks_windows_and_posix_home() {
        assert_eq!(
            redact_text(r"C:\Users\aska\.minecraft\logs\latest.log"),
            r"C:\Users\***\.minecraft\logs\latest.log"
        );
        assert_eq!(
            redact_text("/home/aska/.minecraft/logs"),
            "/home/***/.minecraft/logs"
        );
    }

    #[test]
    fn keeps_already_redacted_and_is_idempotent() {
        let once = redact_text(r"C:\Users\aska\game\log.txt");
        assert_eq!(once, r"C:\Users\***\game\log.txt");
        assert_eq!(redact_text(&once), once);
        assert_eq!(redact_text("<home>/.minecraft"), "<home>/.minecraft");
    }

    #[test]
    fn masks_bearer_token() {
        assert_eq!(
            redact_text("Authorization: Bearer eyJhbGciOi.payload.sig"),
            "Authorization: Bearer ***"
        );
    }

    #[test]
    fn detects_sensitive_key_names() {
        assert!(is_sensitive_key("ai.api_key"));
        assert!(is_sensitive_key("AccessToken"));
        assert!(is_sensitive_key("proxy.password"));
        assert!(!is_sensitive_key("download.concurrency"));
    }
}
