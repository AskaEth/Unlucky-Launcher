//! 账户列表的刷新与皮肤抓取（`impl CoreHost` 的这一段，从 `host/mod.rs` 拆出来守 400 行）。
//!
//! 为什么在这里：`host/mod.rs` 在 M109/M113 之后已经贴着 400 行硬线，而这三段是**账户专属**
//! 的编排（读账户库 → 盖皮肤 → 起抓取线程）。放回 `host/accounts.rs` 也不行——那个文件同样
//! 380 行。归属仍是 accounts（`mod accounts { mod refresh; }`），只是按功能再分一层。

use crate::dto::{AccountKind, CoreEvent};

use super::super::{accounts, job, CoreHost, HostTask};

impl CoreHost {
    pub(in crate::host) fn refresh_accounts(&mut self) {
        self.accounts = accounts::accounts(&self.env, self.active_account.as_deref());
        if self.active_account.is_none() {
            // 默认选中第一个账户（没有账户时保持 None = 离线占位）
            self.active_account = self.accounts.first().map(|account| account.id.clone());
            self.accounts = accounts::accounts(&self.env, self.active_account.as_deref());
        }
        self.attach_skins();
        self.request_skins();
        self.send(CoreEvent::AccountsChanged(self.accounts.clone()));
    }

    /// 把已抓到的皮肤盖到账户视图上。
    ///
    /// 只盖第三方轨：离线档案没有皮肤，微软轨不做真头像（M114 口径）——
    /// 「没有皮肤就画首字母块」，绝不给假皮肤/白块。
    pub(in crate::host) fn attach_skins(&mut self) {
        for account in &mut self.accounts {
            if account.kind != AccountKind::Authlib {
                account.skin = None;
                continue;
            }
            if let Some(skin) = self.skins.get(&account.id) {
                account.skin = skin.clone();
            }
        }
    }

    /// 给还没抓过皮肤的第三方轨账户起后台抓取（每个账户只抓一次）。
    ///
    /// 抓不到就记 `None`：错误在面板上不显示（皮肤是装饰，不是业务失败），但也不会重试刷屏。
    fn request_skins(&mut self) {
        let mut pending = Vec::new();
        for account in &self.accounts {
            if account.kind != AccountKind::Authlib || account.server_url.trim().is_empty() {
                continue;
            }
            if self.skin_requested.insert(account.id.clone()) {
                pending.push((
                    account.id.clone(),
                    account.server_url.clone(),
                    account.profile_id.clone(),
                ));
            }
        }
        for (id, server, profile) in pending {
            let tx = self.tx.clone();
            let env = self.env.clone();
            job::spawn("account-skin", move || {
                let skin = accounts::fetch_skin(&env, &server, &profile).ok();
                let _ = tx.send(HostTask::Avatar { id, skin });
            });
        }
    }
}
