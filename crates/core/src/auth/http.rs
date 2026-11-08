//! 认证用的 HTTP 薄封装（三条轨共用）：**复用既有 download 引擎的客户端基座**
//! （mission Context：网络操作走既有 HTTP 基座），只补三件认证需要的东西：
//! 表单/JSON 体、Bearer 头、单请求整体超时。
//!
//! 三条纪律：
//! 1. **响应体不原样进错误文案**——OAuth 的 token 响应体本身就是凭据，把它塞进
//!    `Malformed` 的 message 就等于把 token 写进日志（`analyse/04` §11 观察项 9 的坑）；
//!    因此解析失败只说「不是合法 JSON」，贴原文的唯一通道是
//!    [`crate::auth::body`] 的脱敏摘要（M38：仅非 2xx、≤200 字符、过 Redactor）。
//! 2. 状态码不做归类，**原样交给调用方**：XSTS 的 401 带 `XErr`、`/minecraft/profile` 的
//!    404 要复判 license，都是「非 200 但有语义」的分支，提前归一化会丢掉信息。
//! 3. **诊断留证，但不留凭据**（M38）：`Reply` 额外携带 `Content-Type` 与非 JSON 正文的快照
//!    （[`RAW_PREVIEW_BYTES`] 字节封顶），诊断由 [`crate::auth::body`] 分类/脱敏后再进错误消息。
//!    原文快照**只在 JSON 解析失败时**保留，所以 OAuth 成功响应体不会被复制进第二个字段。

use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde_json::Value;

use crate::auth::error::{AuthError, AuthResult};
use crate::auth::secret::Secret;

/// 单请求整体超时（设备码轮询的每个请求都远快于此）。
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// 非 JSON 正文保留的快照上限（字节）：够判断「是不是 HTML」并抓前几句文案。
pub(crate) const RAW_PREVIEW_BYTES: usize = 512;

/// 一次应答：状态码 + 可解析的 JSON 体 + 诊断用的头/正文快照。
#[derive(Debug, Clone)]
pub(crate) struct Reply {
    /// HTTP 状态码。
    pub status: u16,
    /// 解析成功时的 JSON 体（空体/非 JSON 为 `None`）。
    pub body: Option<Value>,
    /// `Content-Type` 原样（诊断文案与分流判断用；缺失为 `None`）。
    pub content_type: Option<String>,
    /// 非 JSON 正文的前 [`RAW_PREVIEW_BYTES`] 字节（有损 UTF-8；JSON 解析成功时为 `None`）。
    pub raw: Option<String>,
}

impl Reply {
    /// 是否 2xx。
    pub(crate) fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// 取一个字符串字段。
    pub(crate) fn text(&self, path: &str) -> Option<&str> {
        self.body.as_ref()?.get(path)?.as_str()
    }

    /// 取一个 u64 字段（数字或数字字符串都接受）。
    pub(crate) fn number(&self, path: &str) -> Option<u64> {
        let value = self.body.as_ref()?.get(path)?;
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|raw| raw.trim().parse().ok()))
    }
}

/// 表单 `POST`（OAuth 的设备码/token 端点）。
pub(crate) async fn post_form(
    client: &Client,
    url: &str,
    form: Vec<(&str, String)>,
) -> AuthResult<Reply> {
    let request = client
        .post(url)
        .header(ACCEPT, "application/json")
        .timeout(REQUEST_TIMEOUT)
        .form(&form);
    send(request, url).await
}

/// JSON `POST`（XBL / XSTS / login_with_xbox）。
pub(crate) async fn post_json(
    client: &Client,
    url: &str,
    body: &Value,
    bearer: Option<&Secret>,
) -> AuthResult<Reply> {
    let text = serde_json::to_string(body)
        .map_err(|error| AuthError::Malformed(format!("请求体序列化失败：{error}")))?;
    let mut request = client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .timeout(REQUEST_TIMEOUT)
        .body(text);
    if let Some(token) = bearer {
        request = request.header(AUTHORIZATION, format!("Bearer {}", token.expose()));
    }
    send(request, url).await
}

/// 带 Bearer 的 `GET`（entitlements / profile）。
pub(crate) async fn get_json(client: &Client, url: &str, bearer: &Secret) -> AuthResult<Reply> {
    let request = client
        .get(url)
        .header(ACCEPT, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", bearer.expose()))
        .timeout(REQUEST_TIMEOUT);
    send(request, url).await
}

/// 发请求并解析 JSON（无凭据泄漏的错误映射）。
async fn send(request: reqwest::RequestBuilder, url: &str) -> AuthResult<Reply> {
    let response = request
        .send()
        .await
        .map_err(|error| http_error(url, &error))?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = response
        .bytes()
        .await
        .map_err(|error| http_error(url, &error))?;
    let body = if bytes.is_empty() {
        None
    } else {
        serde_json::from_slice::<Value>(&bytes).ok()
    };
    // 只有解析失败（HTML 拦截页 / 纯文本 / 坏 JSON）才留原文快照：
    // 解析成功的响应体可能含 token，不留第二份副本。
    let raw = match (&body, bytes.is_empty()) {
        (None, false) => Some(preview(&bytes)),
        _ => None,
    };
    Ok(Reply {
        status,
        body,
        content_type,
        raw,
    })
}

/// 取正文前 [`RAW_PREVIEW_BYTES`] 字节（有损解码，截断时补 `…`）。
fn preview(bytes: &[u8]) -> String {
    let truncated = bytes.len() > RAW_PREVIEW_BYTES;
    let head = &bytes[..bytes.len().min(RAW_PREVIEW_BYTES)];
    let mut text = String::from_utf8_lossy(head).into_owned();
    if truncated {
        text.push('…');
    }
    text
}

/// `reqwest::Error` → [`AuthError`]（超时与非超时分开，文案只带 host+path）。
fn http_error(url: &str, error: &reqwest::Error) -> AuthError {
    let target = url.split('?').next().unwrap_or(url).to_string();
    if error.is_timeout() {
        AuthError::Timeout {
            what: format!("请求 {target}"),
        }
    } else {
        AuthError::network(target, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{preview, Reply, RAW_PREVIEW_BYTES};
    use serde_json::json;

    fn reply(status: u16, body: Option<serde_json::Value>) -> Reply {
        Reply {
            status,
            body,
            content_type: Some("application/json".to_string()),
            raw: None,
        }
    }

    #[test]
    fn reply_reads_text_and_numbers() {
        let sample = reply(
            200,
            Some(json!({ "user_code": "ABC", "expires_in": 900, "interval": "5" })),
        );
        assert!(sample.is_success());
        assert_eq!(sample.text("user_code"), Some("ABC"));
        assert_eq!(sample.number("expires_in"), Some(900));
        assert_eq!(sample.number("interval"), Some(5));
        assert_eq!(sample.number("missing"), None);
        assert_eq!(sample.text("missing"), None);
    }

    #[test]
    fn non_success_is_not_success() {
        assert!(!reply(401, None).is_success());
    }

    #[test]
    fn preview_caps_and_marks_truncation() {
        let short = preview(b"Bad Gateway");
        assert_eq!(short, "Bad Gateway");
        let long = preview(&vec![b'x'; RAW_PREVIEW_BYTES + 64]);
        assert!(long.ends_with('…'));
        assert_eq!(long.trim_end_matches('…').len(), RAW_PREVIEW_BYTES);
    }
}
