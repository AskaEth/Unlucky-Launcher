//! 账户页 DTO：三条轨道的统一抽象 + 微软登录（设备码 / 授权码浏览器）的可见状态。
//!
//! **凭据红线**：本模块（以及整个 `crate::dto`）只有 [`AccountView`] 这种公开字段视图。
//! `core::auth` 的 `Credentials` / `MicrosoftSession` / token 永远不进 app 层——
//! host 侧在拿到账户快照的那一刻就丢掉凭据，只留「能显示什么」。

use serde::{Deserialize, Serialize};

/// 账户轨道（三轨统一抽象，`09 §G6` / `D9`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountKind {
    /// 离线账户（HMCL 兼容 UUID）
    Offline,
    /// 微软正版
    Microsoft,
    /// authlib-injector（第三方皮肤站）
    Authlib,
}

impl AccountKind {
    /// 徽标文字。
    pub fn label(self) -> &'static str {
        match self {
            Self::Offline => "离线",
            Self::Microsoft => "微软",
            Self::Authlib => "第三方",
        }
    }

    /// 稳定 id（事件/持久化用；与 core 的 `kind.as_str()` 同词表）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Microsoft => "microsoft",
            Self::Authlib => "authlib_injector",
        }
    }

    /// 从 core 的稳定 id 解析（未知轨道按离线处理，不让一个坏条目毁掉整个列表）。
    pub fn parse(raw: &str) -> Self {
        match raw {
            "microsoft" => Self::Microsoft,
            "authlib_injector" => Self::Authlib,
            _ => Self::Offline,
        }
    }
}

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
    pub created: String,
    /// 是否是当前启动使用的账户
    pub active: bool,
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
    /// 状态/错误文案
    pub message: String,
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
            message.push_str(&format!("（{warning}）"));
        }
        Self {
            phase: DeviceLoginPhase::Waiting,
            user_code: String::new(),
            verification_uri: authorization_url.to_string(),
            qr_link: authorization_url.to_string(),
            expires_in,
            message,
            account_name: String::new(),
        }
    }

    /// 失败态。
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            phase: DeviceLoginPhase::Failed,
            message: message.into(),
            ..Self::new()
        }
    }

    /// 成功态。
    pub fn succeeded(account_name: impl Into<String>) -> Self {
        let account_name = account_name.into();
        Self {
            phase: DeviceLoginPhase::Succeeded,
            message: format!("已登录：{account_name}"),
            account_name,
            ..Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_round_trips_through_stable_ids() {
        for kind in [
            AccountKind::Offline,
            AccountKind::Microsoft,
            AccountKind::Authlib,
        ] {
            assert_eq!(AccountKind::parse(kind.as_str()), kind);
            assert!(!kind.label().is_empty());
        }
    }

    #[test]
    fn device_login_view_never_carries_tokens() {
        let view = DeviceLoginView::waiting("ABCD-EFGH", "https://www.microsoft.com/link", 900, "");
        assert_eq!(view.phase, DeviceLoginPhase::Waiting);
        assert!(view.phase.busy());
        assert!(view.qr_link.contains("ABCD-EFGH"));
        let json = serde_json::to_string(&view).expect("序列化");
        for forbidden in ["token", "secret", "password", "refresh"] {
            assert!(
                !json.to_lowercase().contains(forbidden),
                "视图里不该出现 {forbidden}"
            );
        }
    }

    #[test]
    fn browser_waiting_shows_a_copyable_link_instead_of_a_user_code() {
        let view = DeviceLoginView::browser_waiting(
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?client_id=x",
            300,
            Some("拉起浏览器失败"),
        );
        assert_eq!(view.phase, DeviceLoginPhase::Waiting);
        assert!(view.phase.busy());
        assert!(view.user_code.is_empty(), "授权码流没有用户码");
        assert_eq!(view.qr_link, view.verification_uri);
        assert!(view.message.contains("浏览器"));
        assert!(view.message.contains("拉起浏览器失败"), "{}", view.message);
        let json = serde_json::to_string(&view).expect("序列化");
        for forbidden in ["refresh", "mc-access"] {
            assert!(!json.contains(forbidden), "视图里不该出现 {forbidden}");
        }
    }

    #[test]
    fn succeeded_and_failed_clear_the_code() {
        let ok = DeviceLoginView::succeeded("Steve");
        assert_eq!(ok.phase, DeviceLoginPhase::Succeeded);
        assert!(ok.user_code.is_empty());
        let bad = DeviceLoginView::failed("网络超时");
        assert_eq!(bad.phase, DeviceLoginPhase::Failed);
        assert_eq!(bad.message, "网络超时");
    }
}
