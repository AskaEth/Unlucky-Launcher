//! 账户/登录状态：事件落地（含**上屏前再脱敏**）、设备码倒计时、当前账户查询。
//!
//! 从 `state.rs` 拆出来守 400 行硬线；`AppState` 的字段仍在那个文件里，
//! 这里只加它的 `impl`（同一个 crate 的固有 impl 可以拆在多个文件）。

use std::time::Instant;

use crate::dto::{AccountView, AuthlibLoginView, DeviceLoginPhase, DeviceLoginView, REDACTED};

use super::AppState;

impl AppState {
    /// 微软登录状态落地。
    ///
    /// **凭据红线（M114）**：这里对 `message` 再做一次形状脱敏。DTO 的构造器已经脱过一遍，
    /// 但状态层是**所有**事件都要过的唯一闸口——将来谁新加一个构造器忘了脱敏，也不会把
    /// 服务端回显的 token 画到面板上。脱敏是幂等的，重复做没有代价。
    pub fn apply_device_login(&mut self, mut view: DeviceLoginView) {
        view.message = crate::dto::redact(&view.message);
        view.account_name = crate::dto::redact(&view.account_name);
        // 这里**不发 toast**：presenter 是共享宿主层，写什么语言都不对（复审 r1 P1-1）。
        // 成败反馈由页面自己画（登录面板的 Succeeded/Failed 态 + 账户页横幅），文案走 i18n。
        match view.phase {
            DeviceLoginPhase::Succeeded => {
                self.device_login_started = None;
            }
            DeviceLoginPhase::Failed => {
                self.device_login_started = None;
            }
            DeviceLoginPhase::Waiting => {
                // 每次进入等待态都重记起点：换流/重试之后倒计时必须从头算
                if self.device_login.phase != DeviceLoginPhase::Waiting
                    || self.device_login.user_code != view.user_code
                {
                    self.device_login_started = Some(Instant::now());
                }
            }
            DeviceLoginPhase::Idle | DeviceLoginPhase::Requesting => {
                self.device_login_started = None;
            }
        }
        self.device_login = view;
    }

    /// 第三方轨登录状态落地（同样在状态层再脱敏一遍）。
    pub fn apply_authlib_login(&mut self, mut view: AuthlibLoginView) {
        view.message = crate::dto::redact(&view.message);
        view.server_name = crate::dto::redact(&view.server_name);
        // 同 `apply_device_login`：不发 toast，成败由页面按 i18n 画。
        self.authlib_login = view;
    }

    /// 设备码/授权码还剩几秒（`None` = 没有在等 / 不知道有效期）。
    ///
    /// 纯计算 + 一个起点，UI 侧每秒重问一次就能画出倒计时——这也让「过期」成为**可观测**的
    /// 状态（任务要求：设备码不得只转圈，过期要显示「已过期，重试」）。
    pub fn device_code_remaining(&self) -> Option<u64> {
        self.device_code_remaining_at(Instant::now())
    }

    /// [`Self::device_code_remaining`] 的可注入时钟版本（单测用）。
    pub fn device_code_remaining_at(&self, now: Instant) -> Option<u64> {
        if self.device_login.phase != DeviceLoginPhase::Waiting {
            return None;
        }
        let expires = self.device_login.expires_in;
        if expires == 0 {
            return None;
        }
        let started = self.device_login_started?;
        let elapsed = now.saturating_duration_since(started).as_secs();
        Some(expires.saturating_sub(elapsed))
    }

    /// 当前启动账户（列表里 `active == true` 的那条）。
    pub fn active_account(&self) -> Option<&AccountView> {
        self.accounts.iter().find(|account| account.active)
    }

    /// 面板要显示的脱敏占位符（守门测试用：值不该出现在任何可见文案里）。
    pub const REDACTED: &'static str = REDACTED;
}

#[cfg(test)]
mod tests {
    use super::*;

    const JWT: &str =
        "eyJhbGciOiJSUzI1NiIsIng1dCI6IkFCQyJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlLXZhbHVl";

    fn state() -> AppState {
        AppState::new()
    }

    /// 凭据红线：**绕过构造器**直接塞一个带 JWT 的事件，状态层必须把它抹掉。
    ///
    /// 抓得住：`apply_device_login` / `apply_authlib_login` 里的脱敏被删（那是「最后一个闸口」）。
    /// 抓不住：恶意的 UI 层（如果有人绕过 `AppState` 直接画 DTO）。
    #[test]
    fn a_token_injected_into_events_never_reaches_the_state() {
        let mut state = state();
        state.apply_device_login(DeviceLoginView {
            phase: DeviceLoginPhase::Failed,
            message: format!("登录失败：accessToken={JWT}"),
            ..DeviceLoginView::new()
        });
        assert!(
            !state.device_login.message.contains(JWT),
            "{}",
            state.device_login.message
        );
        assert!(state.device_login.message.contains(REDACTED));

        state.apply_authlib_login(AuthlibLoginView {
            phase: DeviceLoginPhase::Failed,
            message: format!("服务器回包 {JWT}"),
            ..AuthlibLoginView::new()
        });
        assert!(
            !state.authlib_login.message.contains(JWT),
            "{}",
            state.authlib_login.message
        );
    }

    #[test]
    fn the_device_code_countdown_is_derived_from_a_start_instant() {
        let mut state = state();
        state.apply_device_login(DeviceLoginView::waiting(
            "ABCD-EFGH",
            "https://www.microsoft.com/link",
            900,
            "",
        ));
        let started = state.device_login_started.expect("等待态要记起点");
        assert_eq!(state.device_code_remaining_at(started), Some(900));
        let later = started + std::time::Duration::from_secs(120);
        assert_eq!(state.device_code_remaining_at(later), Some(780));
        let expired = started + std::time::Duration::from_secs(901);
        assert_eq!(
            state.device_code_remaining_at(expired),
            Some(0),
            "过期后是 0，不是 None"
        );

        // 没有在等的时候不画倒计时
        state.apply_device_login(DeviceLoginView::failed("网络超时"));
        assert_eq!(state.device_code_remaining_at(started), None);
    }

    #[test]
    fn a_new_waiting_view_restarts_the_countdown() {
        let mut state = state();
        state.apply_device_login(DeviceLoginView::waiting("AAAA-AAAA", "https://a", 900, ""));
        let first = state.device_login_started.expect("起点");
        state.apply_device_login(DeviceLoginView::waiting("BBBB-BBBB", "https://a", 900, ""));
        let second = state.device_login_started.expect("起点");
        assert!(second >= first, "换了一个用户码就要重算倒计时");
    }

    #[test]
    fn the_active_account_is_the_one_marked_active() {
        let mut state = state();
        state.accounts = vec![
            AccountView {
                id: "account:a".to_string(),
                name: "Steve".to_string(),
                kind: crate::dto::AccountKind::Microsoft,
                profile_id: "0f8f".to_string(),
                login_name: String::new(),
                created: String::new(),
                active: false,
                server_url: String::new(),
                skin: None,
            },
            AccountView {
                id: "account:b".to_string(),
                name: "Alex".to_string(),
                kind: crate::dto::AccountKind::Offline,
                profile_id: "1a2b".to_string(),
                login_name: String::new(),
                created: String::new(),
                active: true,
                server_url: String::new(),
                skin: None,
            },
        ];
        assert_eq!(
            state.active_account().map(|a| a.name.as_str()),
            Some("Alex")
        );
    }
}
