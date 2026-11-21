//! Provider 失败的**结构化分类**（M174 r1 复审 P2-5）：一次「测试连接」失败到底属于哪一类。
//!
//! ## 为什么不是字符串匹配
//!
//! r1 的版本对 `ProviderError` 的 `Display` 做子串匹配（找 `"401"` / `"connection refused"` /
//! `"timed out"`），有三个真问题：①URL / 请求 id / 模型名里出现 `401` 这类数字就误判（假阳）；
//! ②「连接被拒」的实际文本是 `Http client error: error sending request for url (…)`，不含
//! `connection refused`，只能靠补一个更宽的 `error sending request` 兜住——**这是在猜文本**；
//! ③文本随 rig / reqwest 版本漂，升级一次就静默错档（复审实测：`closed_port_is_unreachable`
//! 一度偶发红）。
//!
//! 现在分类由 [`super::rig`] 的适配层从 rig 0.45 的**结构化报告**（`ErrorReport`：`kind` /
//! `http_status` / `code`）翻译出来，本文件只放枚举与「给界面看的稳定名」。本模块不认识 rig。
//!
//! ## 六档口径
//!
//! | 档 | 判据（`super::rig::map_error`） | 界面含义 |
//! |---|---|---|
//! | [`ProviderFailure::NotConfigured`] | 配置不完整（`validate()` 拒绝）——**没有发请求** | 先把配置填完 |
//! | [`ProviderFailure::Auth`] | 有 HTTP 状态且是 401 / 403，或 rig 明确报「凭据被拒」 | 密钥/权限不对 |
//! | [`ProviderFailure::ModelNotFound`] | 有 HTTP 状态 404，或响应体的机器码是 `model_not_found` | 端点通了，模型不存在 |
//! | [`ProviderFailure::Unreachable`] | 传输层失败：连接被拒 / DNS / TLS / 响应被截断（**没有拿到任何回应**） | 端点不可达 |
//! | [`ProviderFailure::Timeout`] | 调用方的时限（探测的 8 秒）或 rig 报 `ErrorKind::Timeout` / 传输层 `io::ErrorKind::TimedOut` | 太慢或端点不响应 |
//! | [`ProviderFailure::Other`] | 其余（JSON / URL / 请求构造 / 5xx 之类） | 请求失败（详情见下） |

/// 一次 provider 调用的失败分类（见模块文档的六档表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderFailure {
    /// 配置不完整：**没有发出任何请求**。
    NotConfigured,
    /// 鉴权失败（401 / 403 / 凭据被拒）。
    Auth,
    /// 模型不存在（404 / `model_not_found`）。
    ModelNotFound,
    /// 端点不可达（传输层失败，没拿到任何回应）。
    Unreachable,
    /// 超时（调用方时限或传输层超时）。
    Timeout,
    /// 其它（协议 / 解析 / 5xx / 未知）。
    Other,
}

impl ProviderFailure {
    /// 稳定名（日志 / CLI / 测试断言用；界面文案走 i18n）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::Auth => "auth",
            Self::ModelNotFound => "model_not_found",
            Self::Unreachable => "unreachable",
            Self::Timeout => "timeout",
            Self::Other => "other",
        }
    }

    /// 「配置不完整」以外都意味着**请求真的发出去了**（守卫②/③的判据）。
    #[must_use]
    pub const fn sent_a_request(self) -> bool {
        !matches!(self, Self::NotConfigured)
    }
}
