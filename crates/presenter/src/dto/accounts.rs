//! 账户页 DTO：三条轨道的统一抽象 + 微软登录（设备码 / 授权码浏览器）的可见状态。
//!
//! **凭据红线**：本模块（以及整个 `crate::dto`）只有 [`AccountView`] 这种公开字段视图。
//! `core::auth` 的 `Credentials` / `MicrosoftSession` / token 永远不进 app 层——
//! host 侧在拿到账户快照的那一刻就丢掉凭据，只留「能显示什么」。
//!
//! | 文件 | 职责 |
//! |---|---|
//! | 本文件 | 账户/登录**可见状态**（全部是可上屏字段） |
//! | [`redact`] | 形状脱敏：JWT / 不透明串 / `token=...` 一律抹掉（第二道脱敏器） |
//! | `tests` | 契约 round-trip + 凭据红线的入口断言 |

mod failure;
mod kind;
mod name;
mod redact;
#[cfg(test)]
#[path = "accounts/tests.rs"]
mod tests;

pub use failure::{AccountNotice, AccountNoticeCode, LoginFailure};
pub use kind::AccountKind;
pub use name::valid_offline_name;
pub use redact::{looks_like_jwt, redact, REDACTED};

use serde::{Deserialize, Serialize};

/// 账户列表行（**只含可公开字段**）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountView {
    /// 稳定 id（`account:...`）
    pub id: String,
    /// 展示名
    pub name: String,
    /// 轨道
    pub kind: AccountKind,
    /// 档案 UUID（公开）
    pub profile_id: String,
    /// 登录名（微软轨才有；可空）
    pub login_name: String,
    /// 创建时间展示串（`2026-10-31 20:12`）
    ///
    /// 口径：core 的 `AccountMeta` 只有 `created_at_ms`，**没有 last-used**，所以列表上贴的是
    /// 「添加于 {created}」，不伪造「最近使用」（缺口登记在 M114 报告）。
    pub created: String,
    /// 是否是当前启动使用的账户
    pub active: bool,
    /// 第三方轨的认证服务器 API 根（其余轨为空串）
    #[serde(default)]
    pub server_url: String,
    /// 皮肤原图（第三方轨才可能有；`None` = 没有/没取到，UI 退回首字母块）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<SkinImage>,
}

impl AccountView {
    /// 首字母块上的字：展示名第一个非空字符（大写化），空名字回落 `?`。
    ///
    /// 放在 DTO 而不是页面里，是为了「没有皮肤时画什么」这件事只有一处判据，两个 UI 版一致。
    #[must_use]
    pub fn monogram(&self) -> String {
        self.name
            .chars()
            .find(|c| !c.is_whitespace())
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string())
    }

    /// 首字母块的取色索引（0..6）：由 `profile_id` 稳定推导，同一个人每次都是同一个色块。
    #[must_use]
    pub fn monogram_tone(&self) -> u8 {
        let mut hash: u32 = 2_166_136_261;
        for byte in self.profile_id.as_bytes() {
            hash ^= u32::from(*byte);
            hash = hash.wrapping_mul(16_777_619);
        }
        u8::try_from(hash % 6).unwrap_or(0)
    }
}

/// 一张皮肤原图。
///
/// **停在「原图」这一层是刻意的**：presenter 不引入图片解码依赖（`presenter-no-ui` +
/// 依赖极简），裁剪/缩放/取首字母块都由 UI 层自己做。离线轨永远没有它——没有皮肤就不给假皮肤。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkinImage {
    /// PNG 字节（Minecraft 皮肤：64×64 或旧版 64×32）
    pub png: Vec<u8>,
    /// 来源 URL（**只进取证日志，不上屏**）
    pub source: String,
}

impl SkinImage {
    /// 是否有可用字节（0 字节等同没有皮肤）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.png.is_empty()
    }
}

/// 微软登录阶段（UI 据此切换面板；两条流共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceLoginPhase {
    /// 没有登录在进行
    Idle,
    /// 正在向微软申请设备码（短暂）
    Requesting,
    /// 已拿到用户码，等待用户在浏览器完成
    Waiting,
    /// 成功（账户已入库）
    Succeeded,
    /// 失败（原因在 `message`，已脱敏）
    Failed,
}

impl DeviceLoginPhase {
    /// 面板是否要显示（非 Idle 都显示）。
    pub fn visible(self) -> bool {
        !matches!(self, Self::Idle)
    }

    /// 是否还在跑（Requesting / Waiting）。
    pub fn busy(self) -> bool {
        matches!(self, Self::Requesting | Self::Waiting)
    }
}

/// 设备码登录的可见状态。
///
/// `user_code` / `verification_uri` 来自 `core::auth::microsoft::DeviceCodePrompt`——
/// 它们本来就是给用户看的，不是凭据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceLoginView {
    /// 阶段
    pub phase: DeviceLoginPhase,
    /// 用户码（`ABCD-EFGH`）
    pub user_code: String,
    /// 验证地址（`https://www.microsoft.com/link`）
    pub verification_uri: String,
    /// 二维码内容（= 带码的链接；GUI 用文本展示，终端可直接打开）
    pub qr_link: String,
    /// 有效期（秒；0 = 未知）
    pub expires_in: u64,
    /// 状态/错误文案（**失败时是原始诊断，只进日志，不上屏**）
    pub message: String,
    /// 失败原因码（`None` = 不是失败态 / 未分类）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<LoginFailure>,
    /// 成功后入库的账户展示名
    pub account_name: String,
}

impl Default for DeviceLoginView {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceLoginView {
    /// 空闲态。
    pub fn new() -> Self {
        Self {
            phase: DeviceLoginPhase::Idle,
            user_code: String::new(),
            verification_uri: String::new(),
            qr_link: String::new(),
            expires_in: 0,
            message: String::new(),
            failure: None,
            account_name: String::new(),
        }
    }

    /// 已拿到用户码（`present` 回调时刻）。
    pub fn waiting(
        user_code: &str,
        verification_uri: &str,
        expires_in: u64,
        qr_hint: &str,
    ) -> Self {
        let qr_link = if qr_hint.is_empty() {
            format!("{verification_uri}?code={user_code}")
        } else {
            qr_hint.to_string()
        };
        Self {
            phase: DeviceLoginPhase::Waiting,
            user_code: user_code.to_string(),
            verification_uri: verification_uri.to_string(),
            qr_link,
            expires_in,
            message: format!(
                "在浏览器打开 {verification_uri} 并输入用户码 {user_code}（{} 秒内有效）",
                expires_in
            ),
            failure: None,
            account_name: String::new(),
        }
    }

    /// 授权码流：本地回调已就绪、浏览器已拉起（`present` 回调时刻）。
    ///
    /// 面板不展示用户码，改展示**可复制的授权链接**（用户可能需要在另一台设备/
    /// 另一个浏览器里手动打开）；`warning` 是「浏览器没拉起来」这类非致命提示。
    pub fn browser_waiting(
        authorization_url: &str,
        expires_in: u64,
        warning: Option<&str>,
    ) -> Self {
        let mut message = format!(
            "已在系统浏览器打开微软登录页，请完成授权（{} 秒内有效）；没弹出就把下面的链接粘到浏览器里",
            expires_in.max(1)
        );
        if let Some(warning) = warning {
            message.push_str(&format!("（{}）", redact(warning)));
        }
        Self {
            phase: DeviceLoginPhase::Waiting,
            user_code: String::new(),
            verification_uri: authorization_url.to_string(),
            qr_link: authorization_url.to_string(),
            expires_in,
            message,
            failure: None,
            account_name: String::new(),
        }
    }

    /// 失败态（文案在入口就脱敏：错误原文里可能带着服务端回显的 token）。
    pub fn failed(message: impl Into<String>) -> Self {
        Self::failed_as(LoginFailure::Other, message)
    }

    /// 带**原因码**的失败态（页面据码出 i18n；原文只进日志）。
    pub fn failed_as(failure: LoginFailure, message: impl Into<String>) -> Self {
        Self {
            phase: DeviceLoginPhase::Failed,
            message: redact(&message.into()),
            failure: Some(failure),
            ..Self::new()
        }
    }

    /// 成功态。
    pub fn succeeded(account_name: impl Into<String>) -> Self {
        let account_name = account_name.into();
        Self {
            phase: DeviceLoginPhase::Succeeded,
            message: format!("已登录：{account_name}"),
            failure: None,
            account_name,
            ..Self::new()
        }
    }
}

/// 第三方轨（authlib-injector）登录请求。
///
/// **凭据红线**：`password` 只是「用户这一次输入的请求值」，从页面流到 host 的
/// `AuthlibAuth::authenticate` 调用栈里，host 取走即弃——不入账户库、不进日志、
/// 不进任何回程 DTO。所以本类型只有一个方向（UI → host），回程只有 [`AuthlibLoginView`]。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthlibRequest {
    /// 认证服务器地址（用户原样输入；host 侧 `normalize_api_root` 规范化）
    pub server: String,
    /// 登录名（邮箱或用户名）
    pub login: String,
    /// 密码（只在本调用栈里出现）
    pub password: String,
}

/// **手写 `Debug`**：任何 `{:?}`（日志/panic 消息/测试失败输出）都不得带出明文密码。
impl std::fmt::Debug for AuthlibRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthlibRequest")
            .field("server", &redact(&self.server))
            .field("login", &redact(&self.login))
            .field("password", &REDACTED)
            .finish()
    }
}

impl AuthlibRequest {
    /// 构造。
    pub fn new(
        server: impl Into<String>,
        login: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            server: server.into(),
            login: login.into(),
            password: password.into(),
        }
    }

    /// 可打印形态（日志/诊断/错误路径用）：**密码恒被抹掉**。
    #[must_use]
    pub fn redacted(&self) -> String {
        format!(
            "server={} login={} password={REDACTED}",
            redact(&self.server),
            redact(&self.login)
        )
    }
}

/// 第三方轨登录的可见状态（阶段复用 [`DeviceLoginPhase`]：都是「申请 → 等待 → 成/败」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthlibLoginView {
    /// 阶段
    pub phase: DeviceLoginPhase,
    /// 认证服务器地址（规范化后的 API 根）
    pub server: String,
    /// 服务发现里的服务器名（校验通过后才有；空 = 没校验/没名字）
    pub server_name: String,
    /// 状态/错误文案（**失败时是原始诊断，只进日志，不上屏**）
    pub message: String,
    /// 失败原因码（`None` = 不是失败态 / 未分类）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<LoginFailure>,
    /// 成功后入库的账户展示名
    pub account_name: String,
}

impl Default for AuthlibLoginView {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthlibLoginView {
    /// 空闲态。
    pub fn new() -> Self {
        Self {
            phase: DeviceLoginPhase::Idle,
            server: String::new(),
            server_name: String::new(),
            message: String::new(),
            failure: None,
            account_name: String::new(),
        }
    }

    /// 进行中（校验服务器地址 / 换 token）。
    pub fn running(server: &str, message: &str) -> Self {
        Self {
            phase: DeviceLoginPhase::Requesting,
            server: server.to_string(),
            server_name: String::new(),
            message: redact(message),
            failure: None,
            account_name: String::new(),
        }
    }

    /// 地址校验通过（服务发现拿到了服务器名）。
    pub fn reachable(server: &str, server_name: &str) -> Self {
        Self {
            phase: DeviceLoginPhase::Waiting,
            server: server.to_string(),
            server_name: redact(server_name),
            message: format!("服务器可用：{}", server_name.trim()),
            failure: None,
            account_name: String::new(),
        }
    }

    /// 失败态（文案在入口就脱敏）。
    pub fn failed(message: impl Into<String>) -> Self {
        Self::failed_as(LoginFailure::Other, message)
    }

    /// 带**原因码**的失败态（页面据码出 i18n；原文只进日志）。
    pub fn failed_as(failure: LoginFailure, message: impl Into<String>) -> Self {
        Self {
            phase: DeviceLoginPhase::Failed,
            message: redact(&message.into()),
            failure: Some(failure),
            ..Self::new()
        }
    }

    /// 成功态。
    pub fn succeeded(server_name: &str, account_name: impl Into<String>) -> Self {
        let account_name = account_name.into();
        Self {
            phase: DeviceLoginPhase::Succeeded,
            server_name: server_name.to_string(),
            message: format!("已登录：{account_name}"),
            account_name,
            ..Self::new()
        }
    }
}
