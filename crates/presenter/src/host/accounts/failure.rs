//! 账户写操作的失败分类 + 离线名字契约（M114 r2）。
//!
//! 分类**只看 `AuthError` 的变体与 ascii 码**，不看中文文案——文案随时会改，
//! 而页面据 `AccountNoticeCode` 选 i18n，两边必须能独立演进。

use unlucky_core::auth::AuthError;

use unlucky_core::auth::BodyKind;

use crate::dto::{AccountNoticeCode, LoginFailure};

/// 账户写操作的失败：**结构化码 + 原文**（原文只进日志/详细信息，不上屏）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountFailure {
    /// 结果码（页面据此选 i18n）
    pub code: AccountNoticeCode,
    /// 原始诊断（已脱敏）
    pub detail: String,
}

impl AccountFailure {
    /// 按 core 的错误类型分类（不看中文文案，只看变体）。
    pub(super) fn classify(error: AuthError, fallback: AccountNoticeCode) -> Self {
        let code = match &error {
            unlucky_core::auth::AuthError::Conflict { .. } => AccountNoticeCode::AccountExists,
            unlucky_core::auth::AuthError::NotFound { .. } => AccountNoticeCode::NotFound,
            unlucky_core::auth::AuthError::Malformed(_) => AccountNoticeCode::OfflineNameInvalid,
            unlucky_core::auth::AuthError::Store { .. } => AccountNoticeCode::StoreFailed,
            _ => fallback,
        };
        Self {
            code,
            detail: error.to_string(),
        }
    }
}

/// `AuthError` → 原因码（**只看变体与 ascii 码，不看中文文案**）。
pub(super) fn classify(error: &AuthError) -> LoginFailure {
    match error {
        AuthError::Network { .. } | AuthError::Timeout { .. } => LoginFailure::Unreachable,
        AuthError::Denied { code, description } => {
            if mentions_whitelist(code) || mentions_whitelist(description) {
                LoginFailure::NotWhitelisted
            } else {
                LoginFailure::Denied
            }
        }
        // 只看 `ResponseDetail` 的**结构化字段**（status / kind / server_message），
        // 不把整条错误 Debug 成字符串再去扫——那会把中文 `what`/`digest` 也算进判据里。
        AuthError::Response(detail) => {
            let server = detail.server_message.as_deref().unwrap_or_default();
            let digest = detail.digest.as_str();
            if mentions_whitelist(server) || mentions_whitelist(digest) {
                LoginFailure::NotWhitelisted
            } else if detail.status >= 500 || matches!(detail.kind, BodyKind::Html | BodyKind::Text)
            {
                // 5xx / 拦截页：服务器或链路出了问题，不是「它拒绝了你」。
                LoginFailure::Unreachable
            } else {
                LoginFailure::Denied
            }
        }
        AuthError::Xbox { .. } | AuthError::CredentialExpired => LoginFailure::Denied,
        AuthError::Store { .. } => LoginFailure::StoreFailed,
        _ => LoginFailure::Other,
    }
}

/// 是否属于「应用注册未获批」这一类（AADSTS7000218 / consent required / whitelist）。
pub(super) fn mentions_whitelist(text: &str) -> bool {
    let lowered = text.to_lowercase();
    lowered.contains("aadsts7000218")
        || lowered.contains("aadsts65001")
        || lowered.contains("consent")
        || lowered.contains("whitelist")
        || lowered.contains("not approved")
}

#[cfg(test)]
mod tests {
    use super::*;
    use unlucky_core::auth::body::ResponseDetail;

    fn response(status: u16, kind: BodyKind, server_message: Option<&str>) -> AuthError {
        AuthError::Response(Box::new(ResponseDetail {
            step: "xsts".to_string(),
            what: "XSTS 授权".to_string(),
            status,
            content_type: None,
            kind,
            server_message: server_message.map(str::to_string),
            digest: "digest".to_string(),
        }))
    }

    /// 403 带 AADSTS 码 → 白名单未批；403 不带 → 一般性拒绝。
    #[test]
    fn aadsts_markers_win_over_the_generic_denial() {
        assert_eq!(
            classify(&response(
                403,
                BodyKind::Json,
                Some("AADSTS7000218: app not approved")
            )),
            LoginFailure::NotWhitelisted
        );
        assert_eq!(
            classify(&response(403, BodyKind::Json, Some("invalid_grant"))),
            LoginFailure::Denied
        );
    }

    /// 5xx 与「拦截页」（HTML/纯文本）归「连不上」，不是「服务器拒绝了你」。
    #[test]
    fn upstream_failures_and_intercept_pages_are_unreachable() {
        assert_eq!(
            classify(&response(503, BodyKind::Json, None)),
            LoginFailure::Unreachable
        );
        assert_eq!(
            classify(&response(200, BodyKind::Html, None)),
            LoginFailure::Unreachable
        );
    }
}
