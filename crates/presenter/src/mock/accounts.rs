//! 截图/演示用的账户矩阵（**只在 mock 后端**里被调用）。
//!
//! D30 要求账户页出五态 × 浅深：空态 / 多账号列表 / 设备码进行中 / 登录失败 / 第三方站表单。
//! 截图通道强制 mock 后端（`AppOptions::mock`），所以这五态的**数据**必须由 mock 造：
//! 走 `ULL_ACCOUNT_STATE` 选场景，与 `ULL_START_ROUTE` / `ULL_APPEARANCE_*` 同一套取证开关。
//!
//! ## 造的是什么、不造什么
//!
//! - 列表行造的是**公开字段**（名字 / 轨道 / 创建时间 / 服务器），与真 host 的
//!   `AccountView` 同构；
//! - **不造皮肤字节**：第三方轨的真图来自服务器，mock 里没有（伪造一张「假皮肤」正好违反
//!   M114 的口径）。所以截图里皮肤区显示的是首字母块 + 「没有可用皮肤」。

use crate::dto::{
    AccountKind, AccountNotice, AccountNoticeCode, AccountView, AuthlibLoginView, DeviceLoginView,
    LoginFailure,
};

impl super::MockBackend {
    /// 取消登录：两条轨都发 Idle（真 host 的 `CancelDeviceLogin` 就是这么做的）。
    ///
    /// 为什么放在 mock 里也要认真做：M114 的 `Close` 回归闸与「关掉之后」的取证图都走这条
    /// 通道——mock 用 `_ => {}` 兜底的话，Close 会看起来「点了没反应」，断言会假红/假绿。
    pub(super) fn cancel_login(&mut self) {
        self.emit(crate::dto::AppEvent::Core(
            crate::dto::CoreEvent::DeviceLoginChanged(DeviceLoginView::new()),
        ));
        self.emit(crate::dto::AppEvent::Core(
            crate::dto::CoreEvent::AuthlibLoginChanged(AuthlibLoginView::new()),
        ));
    }
}

/// 场景名（`ULL_ACCOUNT_STATE`；未知值按 `list` 处理）。
pub(super) fn scenario() -> String {
    std::env::var("ULL_ACCOUNT_STATE")
        .unwrap_or_else(|_| "list".to_string())
        .trim()
        .to_lowercase()
}

/// 场景对应的账户列表（`empty` 场景是空表 ⇒ 空态）。
pub(super) fn accounts(scenario: &str) -> Vec<AccountView> {
    if scenario == "empty" {
        return Vec::new();
    }
    vec![
        account(
            "account:mock-msa",
            "Steve",
            AccountKind::Microsoft,
            "8f2f1d3c4b5a69788796a5b4c3d2e1f0",
            "steve@outlook.com",
            "2026-10-28 20:12",
            true,
            "",
        ),
        account(
            "account:mock-authlib",
            "Alex",
            AccountKind::Authlib,
            "0a1b2c3d4e5f60718293a4b5c6d7e8f9",
            "alex@example.com",
            "2026-10-30 09:40",
            false,
            "https://littleskin.cn/api/yggdrasil/",
        ),
        account(
            "account:mock-offline",
            "Herobrine",
            AccountKind::Offline,
            "3c4d5e6f708192a3b4c5d6e7f8091a2b",
            "",
            "2026-10-31 21:05",
            false,
            "",
        ),
    ]
}

/// 场景对应的登录面板状态（`None` = 没有面板）。
pub(super) fn device_login(scenario: &str) -> Option<DeviceLoginView> {
    match scenario {
        "device" | "expired" => Some(DeviceLoginView::waiting(
            "ABCD-EFGH",
            "https://www.microsoft.com/link",
            900,
            "",
        )),
        // 带**原因码**的失败：原文（中文诊断）留在 detail 里只进日志，页面画 i18n + Code
        // `closed` 用同一份种子，再由 `force_account_state` 按一次「关闭」，出「关掉之后」的图。
        "failed" | "closed" => Some(DeviceLoginView::failed_as(
            LoginFailure::NotWhitelisted,
            "微软拒绝了这次登录：AADSTS7000218（HTTP 403）—— 应用白名单还没批准，或网络被拦截",
        )),
        _ => None,
    }
}

/// 场景对应的**第三方站**登录状态（`authlib-failed` = 地址不是 authlib 服务器）。
///
/// 原文（中文诊断）只进日志；页面据 `failure` 码出 i18n（复审 r1 P1-1 的第三方轨那一半）。
pub(super) fn authlib_login(scenario: &str) -> Option<AuthlibLoginView> {
    match scenario {
        "authlib-failed" => Some(AuthlibLoginView {
            phase: crate::dto::DeviceLoginPhase::Failed,
            server: "https://example.invalid/api/yggdrasil/".to_string(),
            server_name: String::new(),
            message: "服务器返回 HTTP 404（不像可用的 authlib 认证服务器）".to_string(),
            failure: Some(LoginFailure::NotAuthlibServer),
            account_name: String::new(),
        }),
        _ => None,
    }
}

/// 场景对应的横幅（`notice` = 离线名字不合法那条；`added` = 添加成功的正向样例）。
pub(super) fn notice(scenario: &str) -> Option<AccountNotice> {
    match scenario {
        "notice" => Some(AccountNotice::new(
            AccountNoticeCode::OfflineNameInvalid,
            "ab",
            "离线名字必须是 3–16 位字母/数字/下划线",
        )),
        "added" => Some(AccountNotice::new(AccountNoticeCode::Added, "Alex", "")),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn account(
    id: &str,
    name: &str,
    kind: AccountKind,
    profile_id: &str,
    login_name: &str,
    created: &str,
    active: bool,
    server_url: &str,
) -> AccountView {
    AccountView {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        profile_id: profile_id.to_string(),
        login_name: login_name.to_string(),
        created: created.to_string(),
        active,
        server_url: server_url.to_string(),
        skin: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_empty_scenario_has_no_rows_and_the_list_scenario_covers_all_three_tracks() {
        assert!(accounts("empty").is_empty());
        let rows = accounts("list");
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().any(|row| row.kind == AccountKind::Microsoft));
        assert!(rows.iter().any(|row| row.kind == AccountKind::Authlib));
        assert!(rows.iter().any(|row| row.kind == AccountKind::Offline));
        assert_eq!(rows.iter().filter(|row| row.active).count(), 1);
        assert!(rows.iter().all(|row| row.skin.is_none()), "mock 不伪造皮肤");
    }

    #[test]
    fn the_device_states_are_renderable() {
        assert!(device_login("device").is_some_and(|view| !view.user_code.is_empty()));
        assert!(device_login("failed")
            .is_some_and(|view| view.phase == crate::dto::DeviceLoginPhase::Failed));
        assert!(device_login("list").is_none());
    }
}
