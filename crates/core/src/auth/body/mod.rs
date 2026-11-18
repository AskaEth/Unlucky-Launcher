//! 非 2xx 应答的**正文诊断**（M38）：把 M31「响应体一律丢掉」这个盲区补上。
//!
//! 起因是 `findings/20261030-tower-improve-login-with-xbox-403.md`：MSA 真机验收两轮都是
//! OAuth→XBL→XSTS 全通、`login_with_xbox` 稳定 403，而旧 `rejected()` 只报状态码，
//! 于是「区域/网络拦截页」和「服务端拒绝（许可/区域）」在文案上分不开。
//!
//! 这里只做三件事：
//! 1. **分类**：JSON（真实服务应答）/ HTML 或纯文本（多为网关、风控或代理拦截页）/ 空体；
//! 2. **提取**：JSON 里的 `error` / `errorMessage` / `reason` / `Message` 等服务端文案直出；
//! 3. **摘要**：正文压成 ≤[`DIGEST_CHARS`] 字符，**先过 [`Redactor`] 再进错误消息**。
//!
//! 凭据红线（M31 的 `Secret`/脱敏体系不破）：
//! - 摘要只从**非 2xx** 响应构造——OAuth 成功响应体（含 token）永远不会走到这里；
//! - `Reply` 的原文快照只在 JSON 解析失败时保留（见 [`crate::auth::http`]），
//!   解析成功的应答不留原文，避免把 token 文本复制进第二个字段；
//! - 摘要与提取出的服务端文案都过同一套脱敏：[`Redactor::scrub`]（已登记 token 整串替换、
//!   ≥64 不透明串打码、JWT 形状的带点 token 打码——第三条 M40 已上提到 `launch::redact`，
//!   本模块不再自己补规则）。

use serde_json::Value;

use crate::auth::error::AuthError;
use crate::auth::http::Reply;
use crate::launch::redact::Redactor;

#[cfg(test)]
mod tests;

/// 错误文案 / 事件里正文摘要的上限（字符）。
pub const DIGEST_CHARS: usize = 200;

/// 服务端文案的取值字段（按优先级，命中即收；`Message` 是 XSTS 的写法）。
const SERVER_MESSAGE_KEYS: [&str; 6] = [
    "errorMessage",
    "error_message",
    "error",
    "errorType",
    "reason",
    "Message",
];

/// 一个非 2xx 应答的正文形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// 合法 JSON（真实服务应答）。
    Json,
    /// HTML 页面（网关/风控/代理拦截页的典型形状）。
    Html,
    /// 其它非 JSON 文本。
    Text,
    /// 空体。
    Empty,
}

impl BodyKind {
    /// 稳定名（错误文案与 JSONL 事件共用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Html => "html",
            Self::Text => "text",
            Self::Empty => "empty",
        }
    }

    /// 是否是「拦截页」形状（HTML / 纯文本）：这一类先怀疑网络，而不是账号。
    #[must_use]
    pub const fn is_interception(self) -> bool {
        matches!(self, Self::Html | Self::Text)
    }
}

/// 一次非 2xx 的诊断结果（[`AuthError::Response`] 的载荷，也是 JSONL 事件的字段来源）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseDetail {
    /// 链上的步骤（`xbl` / `xsts` / `login_with_xbox` / `entitlements` / `profile`）。
    pub step: String,
    /// 这一步在做什么（人话前缀，如「Minecraft 登录被拒」）。
    pub what: String,
    /// HTTP 状态码。
    pub status: u16,
    /// `Content-Type`（服务端没给时为 `None`）。
    pub content_type: Option<String>,
    /// 正文形状。
    pub kind: BodyKind,
    /// 服务端文案（已脱敏、已截断）。
    pub server_message: Option<String>,
    /// 正文摘要（已脱敏，≤[`DIGEST_CHARS`] 字符）。
    pub digest: String,
}

impl ResponseDetail {
    /// 细分机读码（**只增不改**）：JSON 拒绝沿用 M31 的 `<step>_<status>`；
    /// 非 JSON 用新增后缀 `<step>_non_json`；空体用 `<step>_empty_body`。
    #[must_use]
    pub fn detail_code(&self) -> String {
        let suffix = match self.kind {
            BodyKind::Json => self.status.to_string(),
            BodyKind::Html | BodyKind::Text => "non_json".to_string(),
            BodyKind::Empty => "empty_body".to_string(),
        };
        format!("{}_{suffix}", self.step)
    }

    /// 是否是「疑似网络拦截页」（HTML / 纯文本）。
    #[must_use]
    pub const fn is_interception(&self) -> bool {
        self.kind.is_interception()
    }

    /// 「；服务端文案…；响应摘要…」片段（都没有时为空串，方便复用文案拼装）。
    #[must_use]
    pub fn note(&self) -> String {
        let mut note = String::new();
        if let Some(message) = &self.server_message {
            note.push_str(&format!("；服务端文案「{message}」"));
        }
        if !self.digest.is_empty() {
            note.push_str(&format!("；响应摘要：{}", self.digest));
        }
        note
    }

    /// 人读一句话（**不含凭据**），403 的两种分流在这里分开。
    #[must_use]
    pub fn message(&self) -> String {
        let mut text = format!("{}（HTTP {}", self.what, self.status);
        if let Some(content_type) = &self.content_type {
            text.push_str(&format!("，Content-Type: {content_type}"));
        }
        text.push('）');
        match self.kind {
            BodyKind::Json => text.push_str("：服务端返回了 JSON 拒绝"),
            BodyKind::Html | BodyKind::Text => {
                text.push_str("：响应非 JSON（可能网络拦截/网关页）");
            }
            BodyKind::Empty => text.push_str("：响应体为空（可能网络拦截/网关页）"),
        }
        text.push('；');
        text.push_str(advice(&self.step, self.kind));
        text.push_str(&self.note());
        text
    }
}

/// 下一步该往哪查（403 分流的核心：拦截页查网络/代理，JSON 拒绝查账号许可）。
#[must_use]
pub fn advice(step: &str, kind: BodyKind) -> &'static str {
    if kind.is_interception() {
        return "请检查网络或代理（关掉/换掉代理后再试，必要时用镜像方案）";
    }
    match step {
        "login_with_xbox" => "若网络正常，请检查该账号的 Minecraft 许可与所在区域",
        "entitlements" => "若网络正常，请检查该账号是否拥有 Minecraft Java 版",
        "profile" => "若网络正常，请去 minecraft.net 检查档案状态",
        _ => "请按服务端文案处理；如反复失败，检查网络代理是否生效",
    }
}

/// 分类一个非 2xx 应答。
pub(crate) fn classify(reply: &Reply) -> BodyKind {
    if reply.body.is_some() {
        return BodyKind::Json;
    }
    let Some(raw) = reply
        .raw
        .as_deref()
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
    else {
        return BodyKind::Empty;
    };
    let content_type = reply
        .content_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if content_type.contains("html") || looks_like_html(raw) {
        BodyKind::Html
    } else {
        BodyKind::Text
    }
}

/// 正文是不是 HTML 文档（没给 `Content-Type` 时的兜底判断）。
fn looks_like_html(raw: &str) -> bool {
    raw.trim_start().starts_with('<')
}

/// 组装一条诊断（分类 + 服务端文案 + 摘要）。
pub(crate) fn rejection(
    step: &str,
    reply: &Reply,
    what: &str,
    redactor: &Redactor,
) -> ResponseDetail {
    ResponseDetail {
        step: step.to_string(),
        what: what.to_string(),
        status: reply.status,
        content_type: reply.content_type.clone(),
        kind: classify(reply),
        server_message: server_message(reply, redactor),
        digest: digest(reply, redactor),
    }
}

/// 从 JSON 体里挑出服务端文案（去重后以 ` / ` 连接）。
fn server_message(reply: &Reply, redactor: &Redactor) -> Option<String> {
    let body = reply.body.as_ref()?;
    let mut parts: Vec<String> = Vec::new();
    for key in SERVER_MESSAGE_KEYS {
        let Some(text) = body.get(key).and_then(Value::as_str) else {
            continue;
        };
        let text = one_line(text);
        if text.is_empty() || parts.iter().any(|part| part == &text) {
            continue;
        }
        parts.push(text);
    }
    if parts.is_empty() {
        return None;
    }
    Some(truncate(&scrub(&parts.join(" / "), redactor), DIGEST_CHARS))
}

/// 正文摘要：JSON 用紧凑文本，非 JSON 用原文快照；先脱敏再截断。
fn digest(reply: &Reply, redactor: &Redactor) -> String {
    let raw = match (&reply.body, &reply.raw) {
        (Some(body), _) => body.to_string(),
        (None, Some(raw)) => raw.to_string(),
        (None, None) => return String::new(),
    };
    truncate(&scrub(&one_line(&raw), redactor), DIGEST_CHARS)
}

/// 走完整套脱敏：[`Redactor::scrub`] 的三条规则（已登记 token + ≥64 不透明串 + 带点 token）。
///
/// M40 之前这里自己补了一条「带点 token」规则：`Redactor` 只按「连续不透明串 ≥64」判形状，
/// 而 JWT（`header.payload.signature`）里有点号，把连续段切短，恰好躲过那条规则——服务端
/// 把 token 回显在错误体里时就会原样进摘要。现在规则已上提到 `launch::redact`（全局底座），
/// 这里只调共用实现，两处规则不会再漂。
fn scrub(text: &str, redactor: &Redactor) -> String {
    redactor.scrub(text).into_owned()
}

/// 压成一行（HTML 的分行会让错误消息难以阅读）。
fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if std::mem::take(&mut space) {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// 按字符截断（超长补 `…`）。
fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out: String = text.chars().take(limit.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// 把诊断包成 [`AuthError::Response`]（链上各步骤的统一出口）。
pub(crate) fn response(step: &str, reply: &Reply, what: &str, redactor: &Redactor) -> AuthError {
    AuthError::Response(Box::new(rejection(step, reply, what, redactor)))
}
