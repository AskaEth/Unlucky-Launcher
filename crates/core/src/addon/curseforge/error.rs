//! CurseForge 的失败分类（**分级判据只在这里**）。
//!
//! # 为什么这些档必须分开
//!
//! 每一档对应一个**不同的用户动作**，把它们折成一个字符串就是让界面靠 `contains` 猜文案：
//!
//! | 大类 | 站点/客户端怎么说 | 用户该做什么 |
//! |------|-------------------|--------------|
//! | [`CfErrorKind::NotConfigured`] | 本地没有 Key（**一次请求都没发**） | 去设置里填自己的 Key |
//! | [`CfErrorKind::InvalidKey`] | 401 / 403 | Key 不对 / 被吊销，换一把 |
//! | [`CfErrorKind::RateLimited`] | 429（带 `Retry-After`） | 等一会儿再试（**不是**改筛选条件） |
//! | [`CfErrorKind::Network`] | 连不上 / 超时 | 检查网络 |
//! | [`CfErrorKind::NotFound`] | 404 / 410 | 换个关键词 |
//! | [`CfErrorKind::Server`] | 5xx | 站点那边的事，稍后重试 |
//! | [`CfErrorKind::Malformed`] | 响应不是期望形状 | 无（我们自己的 bug 或站点改方言） |
//! | [`CfErrorKind::Unsupported`] | 这条路走不通（请求本身不对 / 没有可用下载地址） | 无 |
//!
//! 「缺 Key」与「Key 无效」都是**不可用**（[`CfErrorKind::is_unavailable`]），但它们是两种修法：
//! 前者去填，后者去换。合并成一档，界面就只能说「CurseForge 不可用」——用户不知道该干嘛。
//!
//! # 密钥纪律
//!
//! 本文件的每一条错误只会带上**状态码**与站点给的 `Retry-After`。Key 从不进来：客户端
//! 不把 header 值传给 `CfError` 的任何构造点（见 `auth::ApiKey` 没有公开的取值方法）。

/// 「拿不到」的成因。
///
/// `RateLimited` 带载荷（`Retry-After` 的秒数）而不是把秒数挂在 [`CfError`] 上：限流这一档的
/// 「该等多久」是站点给的、且只在限流时有意义，放在 kind 里让「这一档必有它」成为类型事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CfErrorKind {
    /// 用户没配置 Key（官方 API 要求 `x-api-key`，本项目不内置）。
    NotConfigured,
    /// Key 无效 / 无权（站点 401 / 403）。
    InvalidKey,
    /// 站点限流（429）；`retry_after` 是站点 `Retry-After` 头里的秒数（没给为 `None`）。
    RateLimited {
        /// 站点要求的等待秒数。
        retry_after: Option<u64>,
    },
    /// 连不上 / 超时 / 传输层失败。
    Network,
    /// 站点说没有这个东西（404 / 410）。
    NotFound,
    /// 站点出错（5xx）。
    Server,
    /// 响应不是期望形状（JSON 坏 / 顶层没有 `data`）。
    Malformed,
    /// 这条路走不通（请求本身不对 / 没有可用下载地址）。
    Unsupported,
}

impl CfErrorKind {
    /// 稳定字符串 id（日志 / DTO / 用例断言用，**不要改名**）。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NotConfigured => "cf_not_configured",
            Self::InvalidKey => "cf_invalid_key",
            Self::RateLimited { .. } => "cf_rate_limited",
            Self::Network => "cf_network",
            Self::NotFound => "cf_not_found",
            Self::Server => "cf_server",
            Self::Malformed => "cf_malformed",
            Self::Unsupported => "cf_unsupported",
        }
    }

    /// 这个源此刻**不可用**（缺 Key 或 Key 无效）。两种都是「先修配置再谈检索」。
    #[must_use]
    pub const fn is_unavailable(&self) -> bool {
        matches!(self, Self::NotConfigured | Self::InvalidKey)
    }

    /// 是不是「用户还没配 Key」（**一次请求都没发**）。
    #[must_use]
    pub const fn is_not_configured(&self) -> bool {
        matches!(self, Self::NotConfigured)
    }

    /// 重试有没有意义（限流 / 站点错 / 网络；401/403/404 重试只会更糟）。
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Server | Self::Network
        )
    }
}

/// 一次取数失败（大类 + 一句人读原因）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfError {
    kind: CfErrorKind,
    message: String,
}

impl CfError {
    /// 构造。
    #[must_use]
    pub fn new(kind: CfErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// 用户没配 Key（具名的不可用状态；调用方据此画「去填 Key」）。
    #[must_use]
    pub fn not_configured() -> Self {
        Self::new(
            CfErrorKind::NotConfigured,
            "CurseForge 需要用户自带的 API Key（x-api-key），当前没有配置",
        )
    }

    /// 传输层失败（连不上 / 超时）。
    #[must_use]
    pub fn network(message: impl Into<String>) -> Self {
        Self::new(CfErrorKind::Network, message)
    }

    /// 响应不是期望形状。
    #[must_use]
    pub fn malformed(message: impl Into<String>) -> Self {
        Self::new(CfErrorKind::Malformed, message)
    }

    /// 这条路走不通。
    #[must_use]
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(CfErrorKind::Unsupported, message)
    }

    /// HTTP 状态码 → 一个大类（**唯一**的状态判定处）。
    ///
    /// 3xx 与没登记的 4xx（400 / 422 …）落到 [`CfErrorKind::Unsupported`]：那是**我们的请求**有问题
    /// （参数拼错 / 端点拼错），重试与换关键词都救不了，报成「站点出错」会让人一直等。
    #[must_use]
    pub fn from_status(status: u16, retry_after: Option<u64>) -> Self {
        match status {
            401 | 403 => Self::new(
                CfErrorKind::InvalidKey,
                format!("CurseForge 拒绝了这把 Key（HTTP {status}）"),
            ),
            404 | 410 => Self::new(
                CfErrorKind::NotFound,
                format!("CurseForge 没有这个条目（HTTP {status}）"),
            ),
            429 => Self::new(
                CfErrorKind::RateLimited { retry_after },
                match retry_after {
                    Some(seconds) => format!("CurseForge 限流（HTTP 429，Retry-After={seconds}s）"),
                    None => "CurseForge 限流（HTTP 429，没给 Retry-After）".to_string(),
                },
            ),
            500..=599 => Self::new(
                CfErrorKind::Server,
                format!("CurseForge 服务端出错（HTTP {status}）"),
            ),
            _ => Self::new(
                CfErrorKind::Unsupported,
                format!("CurseForge 拒绝了这条请求（HTTP {status}）"),
            ),
        }
    }

    /// 大类。
    #[must_use]
    pub const fn kind(&self) -> &CfErrorKind {
        &self.kind
    }

    /// 人读说明（**不含** Key）。
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 站点要求的等待秒数（只有限流那一档有）。
    #[must_use]
    pub const fn retry_after(&self) -> Option<u64> {
        match self.kind {
            CfErrorKind::RateLimited { retry_after } => retry_after,
            _ => None,
        }
    }

    /// 是不是「用户还没配 Key」（上层据此画独立的「去填 Key」态）。
    #[must_use]
    pub const fn is_not_configured(&self) -> bool {
        self.kind.is_not_configured()
    }

    /// 这个源此刻不可用（缺 Key / Key 无效）。
    #[must_use]
    pub const fn is_unavailable(&self) -> bool {
        self.kind.is_unavailable()
    }

    /// 重试有没有意义。
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        self.kind.is_retryable()
    }
}

impl std::fmt::Display for CfError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}：{}", self.kind.as_str(), self.message)
    }
}

impl std::error::Error for CfError {}
