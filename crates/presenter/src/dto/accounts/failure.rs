//! 失败原因码与账户页横幅（M114 r2 / 复审 P1-1、P2-2）。
//!
//! ## 为什么这一层必须有
//!
//! core/presenter 的诊断串是中文人话。把它们直接画到英文界面上，就是复审 r1 那条 p1
//! （「全英文界面里整句中文」）。根因不是 catalog 缺条目，而是**后端诊断串没有结构**：
//! 页面除了原样画没有别的选择。
//!
//! 这里补的就是那个缺环——把「失败属于哪一类」「这次操作的结果是什么」变成**枚举**。
//! 页面据 `code()` 选 i18n key 出标题 + 正文 + 一行 `Code: …`；原始诊断留在 `message` /
//! `detail` 里，**只进日志与详细信息，不上屏**。
//!
//! ## 唯一一个「说清原因」的例外（M132）
//!
//! [`LoginFailure::MsaNotConfigured`]。它上屏的**不是后端诊断串**，而是 catalog 里一条固定
//! 文案（`Key::AccountFailureMsaNotConfigured` / `Key::AccountLoginNotConfigured`，不含任何
//! 凭据、token 或内部细节）——「原文不上屏」这条纪律原样保留，页面仍拿不到、也画不出
//! `message`/`detail`。允许它说清原因的理由只有一条：**原因本身就是用户能执行的下一步**
//! （缺 `ULL_MSA_CLIENT_ID`，设上或构建时注入即可）。判据：没读过代码的用户碰上它，必须能
//! 从界面自己看出「是缺一个配置」并知道怎么补，而不是去翻日志。其余一切（网络 / 拒绝 /
//! 落盘…）仍旧只给码与分类文案，原文只进日志。
//!
//! 约束：`code()` 一律 ascii 且**不随语言变**（日志、测试、用户报障都靠它对齐）。

use serde::{Deserialize, Serialize};

use super::redact;

/// 登录失败的**结构化原因码**（M114 r2 / 复审 P1-1）。
///
/// 为什么要有它：core/presenter 的诊断串是中文人话，**直接上屏**会让英文界面里出现整句中文
/// （复审 r1 的 p1）。这里把「失败属于哪一类」变成**可判据的枚举**，页面据码选 i18n key 出
/// 标题 + 正文 + 一个稳定 code；原始诊断只留在 `message` 里进日志/详细信息，**不上屏**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoginFailure {
    /// 服务器/微软明确拒绝（403、OAuth `error`、XErr）
    Denied,
    /// 应用注册还没获批（AADSTS7000218 一类「白名单未批」）
    NotWhitelisted,
    /// 网络不可达 / 超时
    Unreachable,
    /// 目标地址不是 authlib-injector 服务器（响应不是服务发现 JSON）
    NotAuthlibServer,
    /// 服务器拒绝了这个账号（密码错 / 账号不存在）
    BadCredentials,
    /// 服务器没有返回角色（账号还没创建角色）
    NoProfile,
    /// 落盘失败（账户库读写）
    StoreFailed,
    /// 微软轨的 client id 没配（`ULL_MSA_CLIENT_ID` 运行期/构建期两处都没有）——**配置缺失**，
    /// 不是登录被拒、也不是「没实现」（M132）。
    ///
    /// 单独成码的理由见模块文档「唯一一个『说清原因』的例外」：这一类的原因就是用户能执行的
    /// 下一步，所以它的正文必须说清「要设哪个环境变量」，而不是「完整原因在日志里」。
    MsaNotConfigured,
    /// 其它（未知原因；原文只在 `message`）
    Other,
}

impl LoginFailure {
    /// 稳定的 ascii code（日志 / 页面「Code: …」/ 测试断言都用它，**不随语言变**）。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Denied => "denied",
            Self::NotWhitelisted => "whitelist_pending",
            Self::Unreachable => "unreachable",
            Self::NotAuthlibServer => "not_authlib_server",
            Self::BadCredentials => "bad_credentials",
            Self::NoProfile => "no_profile",
            Self::StoreFailed => "store_failed",
            Self::MsaNotConfigured => "msa_not_configured",
            Self::Other => "other",
        }
    }
}

/// 账户页的**一次性横幅**（添加/删除/切换的结果；页面据 `code` 选 i18n）。
///
/// 为什么不是 toast：app-iced 不渲染 toast（复审 r1 P2-2 实锤），发 toast 等于用户什么都看不到。
/// 横幅是页面上的真实控件，用户点了「Create profile」失败后一定看得见。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountNotice {
    /// 结果码（决定标题/正文/颜色）
    pub code: AccountNoticeCode,
    /// 涉及的账户名或服务器（拼进文案；可为空）
    pub subject: String,
    /// 原始诊断（已脱敏；**只进日志/详细信息，不上屏**）
    pub detail: String,
}

impl AccountNotice {
    /// 构造。
    pub fn new(
        code: AccountNoticeCode,
        subject: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code,
            subject: subject.into(),
            detail: redact(&detail.into()),
        }
    }
}

/// 横幅的结果码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountNoticeCode {
    /// 已添加
    Added,
    /// 已删除
    Removed,
    /// 已切换
    Switched,
    /// 名字不合法（长度/字符集）
    OfflineNameInvalid,
    /// 同名账户已存在
    AccountExists,
    /// 落盘失败
    StoreFailed,
    /// 找不到这个账户
    NotFound,
    /// 其它（原文在 `detail`）
    Other,
}

impl AccountNoticeCode {
    /// 是否是一个「失败」结果（决定横幅配色）。
    #[must_use]
    pub const fn is_failure(self) -> bool {
        !matches!(self, Self::Added | Self::Removed | Self::Switched)
    }

    /// 稳定 ascii code（日志/测试用）。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Switched => "switched",
            Self::OfflineNameInvalid => "offline_name_invalid",
            Self::AccountExists => "account_exists",
            Self::StoreFailed => "store_failed",
            Self::NotFound => "not_found",
            Self::Other => "other",
        }
    }
}
