//! 账户端口：只读 `AccountStore::accounts()`（公开字段）。**凭据红线在这里落闸**。
//!
//! `AccountStore::credentials()` 存在，但本文件**不调用**它——agent 侧也没有能承接
//! `Credentials` 的类型（[`unlucky_agent::port::AccountSummary`]）。这条界线不是靠自觉：
//! 只要有人想从工具结果里带出 token，就必须先改 `AccountSummary`，而 `port::game` 的
//! 单测会先炸（序列化结果里不许出现 token/password/secret）。

use unlucky_agent::port::game::{AccountPort, AccountSummary};
use unlucky_agent::port::PortResult;
use unlucky_core::auth::AccountStore;

/// M31 账户库的只读端口。
#[derive(Debug, Clone)]
pub struct CoreAccounts {
    root: std::path::PathBuf,
}

impl CoreAccounts {
    /// `root` = `<数据根>/.ULL/data/auth`。
    #[must_use]
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl AccountPort for CoreAccounts {
    fn list_accounts(&self) -> PortResult<Vec<AccountSummary>> {
        let store = AccountStore::open(self.root.clone());
        if !store.warnings().is_empty() {
            // 账户文件有坏条目：不阻断（用户还能看到其余账户），但要留痕到 stderr。
            eprintln!(
                "[unlucky-agent] 账户库有 {} 条提示（只读本工具不受影响）",
                store.warnings().len()
            );
        }
        Ok(store
            .accounts()
            .iter()
            .map(|meta| AccountSummary {
                id: meta.id.as_str().to_string(),
                kind: meta.kind.as_str().to_string(),
                profile_id: meta.profile_id.clone(),
                profile_name: meta.profile_name.clone(),
                login_name: meta.login_name.clone(),
                created_at_ms: meta.created_at_ms,
            })
            .collect())
    }
}
