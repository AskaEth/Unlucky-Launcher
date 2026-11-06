//! 认证错误：把 HMCL 的异常树压成「调用方能分流」的一层（`analyse/04` §2.6 / §4.5）。
//!
//! 三条设计：
//! 1. **`CredentialExpired` 是唯一「要人再登录」的信号**（承 HMCL `CredentialExpiredException`），
//!    其余错误都不该弹交互；
//! 2. **取消 / 超时 / 被拒 三分**（mission 明示的错误分流）：用户关窗口 = `Canceled`，
//!    设备码 15 分钟硬超时 = `Timeout`，`access_denied` = `Denied`；
//! 3. **文案里永不含凭据**：错误只描述发生了什么，token 一律不进 message。

use std::fmt;

use crate::auth::body::ResponseDetail;

/// Xbox Live 错误码：账号被封（`MicrosoftService.BANNED`）。
pub const XERR_BANNED: u32 = 2_148_916_227;
/// 没有 Xbox 账号（需先建档案）。
pub const XERR_MISSING_XBOX_ACCOUNT: u32 = 2_148_916_233;
/// 地区不可用。
pub const XERR_COUNTRY_UNAVAILABLE: u32 = 2_148_916_235;
/// 儿童账号未加入家庭组。
pub const XERR_ADD_FAMILY: u32 = 2_148_916_238;

/// 认证失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// 网络层失败（DNS/连接/TLS/超时以外的 IO）。
    Network {
        /// 目标 URL（不含 query 里的凭据）。
        url: String,
        /// 底层描述。
        detail: String,
    },
    /// 超时：连接超时、整体超时，或设备码流程等满 `expires_in`/15 分钟。
    Timeout {
        /// 在等什么。
        what: String,
    },
    /// 用户主动取消（关窗口、Ctrl+C、`access_denied` 也归这里）。
    Canceled,
    /// 凭据过期/被撤销，必须重新交互式登录（`AADSTS70000` 族）。
    CredentialExpired,
    /// 服务端明确拒绝（OAuth `error` + `error_description`）。
    Denied {
        /// OAuth 错误码（`invalid_grant` / `authorization_declined` …）。
        code: String,
        /// 服务端描述。
        description: String,
    },
    /// 链上某一步拿到非 2xx，且带回了正文诊断（M38）。
    ///
    /// 与 [`Self::Denied`] 同一分流层（`code()` 仍是 `denied`，向后兼容 M31 的调用方），
    /// 细分码看 [`AuthError::detail_code`]：JSON 拒绝沿用 `<step>_<status>`，
    /// 拦截页（HTML/纯文本）是新增的 `<step>_non_json`，空体是 `<step>_empty_body`。
    ///
    /// 载荷装箱：`ResponseDetail` 有 7 个字段（含 4 个字符串），不装箱会把整个枚举撑到
    /// 128 字节以上，`Result<T, AuthError>` 的每个 Ok 也跟着变胖（clippy `result_large_err`）。
    Response(Box<ResponseDetail>),
    /// Xbox Live / XSTS 业务错误码（`XErr`）。
    Xbox {
        /// `XErr` 数值。
        code: u32,
    },
    /// 响应结构不合法（JSON 缺字段、类型不对、profile 被换掉）。
    Malformed(String),
    /// 凭据/元数据落盘或读取失败。
    Store {
        /// 涉及的路径。
        path: String,
        /// 底层描述。
        detail: String,
    },
    /// 找不到指定账户。
    NotFound {
        /// 用户给的查找键。
        account: String,
    },
    /// 账户重复（同名同轨已存在）。
    Conflict {
        /// 冲突的账户名。
        account: String,
    },
    /// 该轨/该能力当前不可用：**未配置**（如缺 Azure client id）或本期未实现
    /// （如 CLI 上的 authlib 密码登录）。
    ///
    /// 载荷必须**自带完整的人读句子**（「未配置 + 怎么配」/「未实现 + 为什么」），
    /// 本层不再统一拼「本期未支持：」这类前缀——同一层里两种情况完全不同的事，
    /// 硬拼一个前缀会让「没配置」被读成「没实现」（M117 用户实跑踩到）。
    /// 机读码仍是 `unsupported`（[`Self::code`]），调用方分流不受影响。
    Unsupported(String),
}

impl AuthError {
    /// 稳定机读码（CLI `--json` 与日志都用它，不随文案变）。
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Network { .. } => "network",
            Self::Timeout { .. } => "timeout",
            Self::Canceled => "canceled",
            Self::CredentialExpired => "credential_expired",
            Self::Denied { .. } => "denied",
            // 与 `Denied` 同一粗分流层：M31 时代的调用方（CLI 提示、launch 身份解析）无需改。
            Self::Response(_) => "denied",
            Self::Xbox { .. } => "xbox",
            Self::Malformed(_) => "malformed",
            Self::Store { .. } => "store",
            Self::NotFound { .. } => "not_found",
            Self::Conflict { .. } => "conflict",
            Self::Unsupported(_) => "unsupported",
        }
    }

    /// 是否是「必须重新登录」的信号（唯一该弹交互的一类）。
    #[must_use]
    pub const fn is_credential_expired(&self) -> bool {
        matches!(self, Self::CredentialExpired)
    }

    /// 是否属于「网络/服务端抽风，可重试或跳过」的一类。
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        match self {
            Self::Network { .. } | Self::Timeout { .. } | Self::Xbox { .. } => true,
            // 拦截页/空体多半是网络侧问题；JSON 拒绝是服务端明确表态，重试无意义。
            Self::Response(detail) => detail.kind.is_interception(),
            _ => false,
        }
    }

    /// 细分机读码（M38 新增）：OAuth 拒绝返回 `<step>` 里带的错误码，链上非 2xx 返回
    /// `<step>_<status>` / `<step>_non_json` / `<step>_empty_body`，其余回落到 [`Self::code`]。
    ///
    /// 与 [`Self::code`] 的分工：`code()` 是**分流层**（决定弹不弹交互、能不能重试），
    /// 这个方法给日志与 `--json` 的**排障层**用。既有码一律不改，只增。
    #[must_use]
    pub fn detail_code(&self) -> String {
        match self {
            Self::Response(detail) => detail.detail_code(),
            Self::Denied { code, .. } => code.clone(),
            other => other.code().to_string(),
        }
    }

    /// 网络错误的构造（`url` 只放 host+path，别把 query 里的 code 带进来）。
    #[must_use]
    pub fn network(url: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::Network {
            url: url.into(),
            detail: detail.into(),
        }
    }

    /// 落盘错误的构造。
    #[must_use]
    pub fn store(path: impl fmt::Display, detail: impl Into<String>) -> Self {
        Self::Store {
            path: path.to_string(),
            detail: detail.into(),
        }
    }

    /// 人读一句话（**不含凭据**）。
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Network { url, detail } => format!("网络请求失败（{url}）：{detail}"),
            Self::Timeout { what } => format!("等待超时：{what}"),
            Self::Canceled => "登录已取消".to_string(),
            Self::CredentialExpired => "凭据已过期，需要重新登录".to_string(),
            Self::Denied { code, description } => {
                format!("服务端拒绝（{code}）：{description}")
            }
            Self::Response(detail) => detail.message(),
            Self::Xbox { code } => format!("Xbox 授权失败（XErr {code}）：{}", xbox_hint(*code)),
            Self::Malformed(detail) => format!("响应不符合预期：{detail}"),
            Self::Store { path, detail } => format!("账户数据读写失败（{path}）：{detail}"),
            Self::NotFound { account } => format!("找不到账户 `{account}`"),
            Self::Conflict { account } => format!("账户 `{account}` 已存在"),
            Self::Unsupported(detail) => detail.clone(),
        }
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for AuthError {}

/// `XErr` 码 → 中文说明（承 `analyse/04` §5.2 的四码表）。
#[must_use]
pub fn xbox_hint(code: u32) -> &'static str {
    match code {
        XERR_BANNED => "该微软账号已被 Xbox Live 封禁",
        XERR_MISSING_XBOX_ACCOUNT => "该微软账号还没有 Xbox 档案，请先去 xbox.com 创建",
        XERR_COUNTRY_UNAVAILABLE => "该账号所在地区不被 Xbox Live 支持",
        XERR_ADD_FAMILY => "儿童账号需要先加入 Microsoft 家庭组",
        _ => "Xbox Live 拒绝了本次授权（可能无 Xbox 档案或账号受限）",
    }
}

/// 结果别名。
pub type AuthResult<T> = Result<T, AuthError>;

#[cfg(test)]
mod tests {
    use super::{xbox_hint, AuthError, XERR_ADD_FAMILY};

    #[test]
    fn codes_are_stable() {
        assert_eq!(AuthError::Canceled.code(), "canceled");
        assert_eq!(AuthError::CredentialExpired.code(), "credential_expired");
        assert!(AuthError::Xbox {
            code: XERR_ADD_FAMILY
        }
        .is_transient());
        assert!(!AuthError::Canceled.is_transient());
    }

    #[test]
    fn messages_carry_no_secret_shape() {
        let error = AuthError::Denied {
            code: "invalid_grant".to_string(),
            description: "AADSTS70000: bad redirect".to_string(),
        };
        assert!(error.message().contains("AADSTS70000"));
        assert!(AuthError::Xbox { code: 2148916238 }
            .message()
            .contains("家庭组"));
        assert!(!xbox_hint(0).is_empty());
    }

    #[test]
    fn response_shares_denied_coarse_code_but_has_its_own_detail_code() {
        let error = AuthError::Response(Box::new(super::ResponseDetail {
            step: "login_with_xbox".to_string(),
            what: "Minecraft 登录被拒".to_string(),
            status: 403,
            content_type: Some("text/html".to_string()),
            kind: crate::auth::body::BodyKind::Html,
            server_message: None,
            digest: String::new(),
        }));
        // 粗分流层向后兼容：仍是 `denied`；排障层才是细分码。
        assert_eq!(error.code(), "denied");
        assert_eq!(error.detail_code(), "login_with_xbox_non_json");
        assert!(error.is_transient(), "拦截页可以重试");
        assert_eq!(
            AuthError::Canceled.detail_code(),
            "canceled",
            "没有细分的错误回落到 code()"
        );
    }
}
