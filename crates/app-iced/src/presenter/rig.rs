//! 账户轨的 UI 侧 rig（**只在测试里存在**）：把 `AppCommand` 翻成 host 形状的 `CoreEvent`。
//!
//! ## 它替的是谁、替到什么程度
//!
//! 真 host 的账户分支在 `presenter/host/dispatch.rs`：`AddOfflineAccount` → `accounts::add_offline`
//! （`core::auth::AccountStore` 落盘）→ `AccountsChanged`；`SwitchAccount` → 只改内存里的
//! `active_account` → `AccountsChanged`；失败一律走 `AccountNoticeChanged(结构化码)`。
//!
//! 这里**照同一条形状**回事件，但不碰磁盘、不起线程：
//!
//! | 真 host | 本 rig | 差在哪 |
//! |---|---|---|
//! | `AccountStore` 落盘 + 读回 | 内存 `Vec<AccountView>` | 落盘/读回由 presenter 自己的
//!   `host::accounts::offline_add_list_remove_round_trips` 覆盖，本轮**没有**跨 crate 复跑 |
//! | `notice_failure` 带原因码 | 同名三个码（`OfflineNameInvalid` / `NotFound` / `Added`…） | 一致 |
//! | 设备码：起后台线程等用户 | 直接给 Waiting 态 | 「进行中可见 + 可取消」验得到，**真实网络不验** |
//!
//! 为什么不能直接用 `CoreHost`：`host::accounts` 是**私有**模块（outside 本 mission 的 scope），
//! 而 `CoreHost::new` 会 `refresh_all`（版本清单网络线程 + Java 探测）。测试要的是确定性。

use std::collections::VecDeque;

use unlucky_presenter::dto::{
    valid_offline_name, AccountKind, AccountNotice, AccountNoticeCode, AccountView, AppCommand,
    AppEvent, Backend, CoreEvent, DeviceLoginView,
};

/// 账户轨的假后端（形状与真 host 一致，数据在内存里）。
#[derive(Debug)]
pub(crate) struct AccountRig {
    events: VecDeque<AppEvent>,
    accounts: Vec<AccountView>,
    next_id: u32,
    active: Option<String>,
}

impl Default for AccountRig {
    fn default() -> Self {
        Self::new()
    }
}

impl AccountRig {
    /// 空账户表。
    pub(crate) fn new() -> Self {
        Self {
            events: VecDeque::new(),
            accounts: Vec::new(),
            next_id: 0,
            active: None,
        }
    }

    fn emit(&mut self, event: CoreEvent) {
        self.events.push_back(AppEvent::Core(event));
    }

    /// 当前列表（`active` 标记按内存里的当前账户算）。
    fn snapshot(&mut self) {
        self.emit(CoreEvent::AccountsChanged(self.accounts.clone()));
    }

    fn notice(&mut self, code: AccountNoticeCode, subject: &str, detail: &str) {
        self.emit(CoreEvent::AccountNoticeChanged(Some(AccountNotice::new(
            code, subject, detail,
        ))));
    }

    fn add_offline(&mut self, name: &str) {
        let name = name.trim();
        if !valid_offline_name(name) {
            // 与 host 同码：名字不合法是**结构化**失败，不是静默
            self.notice(
                AccountNoticeCode::OfflineNameInvalid,
                name,
                "长度/字符集不符",
            );
            return;
        }
        if self.accounts.iter().any(|account| account.name == name) {
            self.notice(AccountNoticeCode::AccountExists, name, "同名账户已存在");
            return;
        }
        self.next_id += 1;
        let id = format!("account:rig-{}", self.next_id);
        self.accounts.push(AccountView {
            id,
            name: name.to_string(),
            kind: AccountKind::Offline,
            profile_id: format!("{:08x}", self.next_id),
            login_name: String::new(),
            created: "2026-11-05 00:00".to_string(),
            active: false,
            server_url: String::new(),
            skin: None,
        });
        self.notice(AccountNoticeCode::Added, name, "");
        self.snapshot();
    }

    fn remove(&mut self, key: &str) {
        let Some(index) = self
            .accounts
            .iter()
            .position(|account| account.id == key || account.name == key)
        else {
            self.notice(AccountNoticeCode::NotFound, key, "找不到这个账户");
            return;
        };
        let removed = self.accounts.remove(index);
        if self.active.as_deref() == Some(removed.id.as_str()) {
            self.active = None;
        }
        self.notice(AccountNoticeCode::Removed, &removed.name, "");
        self.snapshot();
    }

    fn switch(&mut self, id: &str) {
        if !self.accounts.iter().any(|account| account.id == id) {
            self.notice(AccountNoticeCode::NotFound, id, "找不到这个账户");
            return;
        }
        let name = self
            .accounts
            .iter()
            .find(|account| account.id == id)
            .map(|account| account.name.clone())
            .unwrap_or_default();
        self.active = Some(id.to_string());
        for account in &mut self.accounts {
            account.active = account.id == id;
        }
        self.notice(AccountNoticeCode::Switched, &name, "");
        self.snapshot();
    }
}

impl Backend for AccountRig {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        self.events.drain(..).collect()
    }

    fn dispatch(&mut self, command: AppCommand) {
        match command {
            AppCommand::RefreshAll => self.snapshot(),
            AppCommand::AddOfflineAccount(name) => self.add_offline(&name),
            AppCommand::RemoveAccount(key) => self.remove(&key),
            AppCommand::SwitchAccount(id) => self.switch(&id),
            AppCommand::DismissAccountNotice => self.emit(CoreEvent::AccountNoticeChanged(None)),
            // 两条登录流：都给「等用户」态（真 host 在这里起后台线程 + 浏览器）
            AppCommand::StartDeviceLogin | AppCommand::StartBrowserLogin => {
                self.emit(CoreEvent::DeviceLoginChanged(DeviceLoginView::waiting(
                    "ABCD-EFGH",
                    "https://www.microsoft.com/link",
                    900,
                    "",
                )));
            }
            // 取消：两条轨都收回 Idle（真 host 的 `CancelDeviceLogin` 就是这么做的）
            AppCommand::CancelDeviceLogin => {
                self.emit(CoreEvent::DeviceLoginChanged(DeviceLoginView::new()));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rig_answers_add_switch_remove_with_host_shaped_events() {
        let mut rig = AccountRig::new();
        rig.dispatch(AppCommand::AddOfflineAccount("Steve".to_string()));
        let events = rig.drain_events();
        assert!(events.iter().any(|event| matches!(
            event,
            AppEvent::Core(CoreEvent::AccountsChanged(rows)) if rows.len() == 1
        )));

        let id = match &events[1] {
            AppEvent::Core(CoreEvent::AccountsChanged(rows)) => rows[0].id.clone(),
            _ => unreachable!(),
        };
        rig.dispatch(AppCommand::SwitchAccount(id.clone()));
        assert!(matches!(
            rig.drain_events().first(),
            Some(AppEvent::Core(CoreEvent::AccountNoticeChanged(Some(notice))))
                if notice.code == AccountNoticeCode::Switched
        ));

        rig.dispatch(AppCommand::RemoveAccount(id));
        let events = rig.drain_events();
        assert!(events.iter().any(|event| matches!(
            event,
            AppEvent::Core(CoreEvent::AccountsChanged(rows)) if rows.is_empty()
        )));
    }

    #[test]
    fn an_invalid_offline_name_is_a_structured_failure_not_silence() {
        let mut rig = AccountRig::new();
        rig.dispatch(AppCommand::AddOfflineAccount("ab".to_string()));
        assert!(matches!(
            rig.drain_events().first(),
            Some(AppEvent::Core(CoreEvent::AccountNoticeChanged(Some(notice))))
                if notice.code == AccountNoticeCode::OfflineNameInvalid
        ));
    }
}
