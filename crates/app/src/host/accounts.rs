//! 账户：三轨统一列表（只读公开字段）+ 离线新增 + 删除/切换 + 微软设备码登录。
//!
//! 凭据红线（D9）：`Credentials` / `MicrosoftSession` 只在本模块与 `launch.rs` 里出现，
//! 出口一律是 [`AccountView`] / [`DeviceLoginView`]。设备码本身不是凭据，可以直接展示。
//!
//! 取消语义：设备码轮询跑在 `core` 里、没有取消钩子，所以取消只把面板收回；线程收尾时
//! 若发现「已取消」就不入库（避免用户点了取消却凭空多出一个账户）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use unlucky_core::auth::account::Credentials;
use unlucky_core::auth::microsoft::{now_ms, MicrosoftAuth};
use unlucky_core::auth::offline::offline_meta;
use unlucky_core::auth::AccountStore;
use unlucky_core::download::blocking::block_on;

use crate::dto::{AccountKind, AccountView, DeviceLoginView};

use super::env::HostEnv;
use super::job::{spawn, HostTask, JobSender};

/// 账户列表（公开字段）。
pub fn accounts(env: &HostEnv, active: Option<&str>) -> Vec<AccountView> {
    let store = AccountStore::open(env.layout.auth_data());
    store
        .accounts()
        .iter()
        .map(|meta| AccountView {
            id: meta.id.as_str().to_string(),
            name: meta.display_name().to_string(),
            kind: AccountKind::parse(meta.kind.as_str()),
            profile_id: meta.profile_id.clone(),
            login_name: meta.login_name.clone().unwrap_or_default(),
            created: display_ms(meta.created_at_ms),
            active: active.is_some_and(|id| id == meta.id.as_str()),
        })
        .collect()
}

/// 毫秒时间戳 → `2026-10-31 20:12`（UTC；账户列表只用来排序/辨认）。
pub fn display_ms(ms: u64) -> String {
    if ms == 0 {
        return String::new();
    }
    let total_secs = ms / 1000;
    let days = total_secs / 86_400;
    let rem = total_secs % 86_400;
    let (year, month, day) = civil_from_days(i64::try_from(days).unwrap_or(0));
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rem / 3_600,
        (rem % 3_600) / 60
    )
}

/// Howard Hinnant 的 `civil_from_days`（1970-01-01 为 0）。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// 加一个离线账户。
///
/// # Errors
///
/// 名字不合法、同名冲突、落盘失败时返回用户可读文案。
pub fn add_offline(env: &HostEnv, name: &str) -> Result<String, String> {
    let name = name.trim();
    let meta = offline_meta(name, now_ms()).map_err(|error| error.to_string())?;
    let display = meta.display_name().to_string();
    let mut store = AccountStore::open(env.layout.auth_data());
    store
        .insert(meta, Credentials::Offline)
        .map_err(|error| format!("添加账户失败：{error}"))?;
    store
        .save()
        .map_err(|error| format!("保存账户失败：{error}"))?;
    Ok(display)
}

/// 删除账户（按 id 或名字）。
///
/// # Errors
///
/// 找不到、落盘失败时返回用户可读文案。
pub fn remove(env: &HostEnv, key: &str) -> Result<String, String> {
    let mut store = AccountStore::open(env.layout.auth_data());
    let meta = store
        .remove(key)
        .map_err(|error| format!("删除账户失败：{error}"))?;
    store
        .save()
        .map_err(|error| format!("保存账户失败：{error}"))?;
    Ok(meta.display_name().to_string())
}

/// 设备码登录任务句柄（取消标志 + 账户名回执）。
#[derive(Debug, Clone, Default)]
pub struct LoginJob {
    /// 用户点了取消（线程收尾时据此丢弃结果）
    pub cancelled: Arc<AtomicBool>,
}

impl LoginJob {
    /// 新建一个未取消的句柄。
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 取消。
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// 是否已取消。
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// 起一次设备码登录（后台线程；`present` 时刻发 Waiting 事件）。
pub fn start_login(env: HostEnv, tx: JobSender, job: LoginJob) {
    spawn("msa-login", move || {
        let auth = match MicrosoftAuth::from_env() {
            Ok(auth) => auth.with_journal(Some(unlucky_core::auth::AuthJournal::in_logs_dir(
                env.layout.logs(),
            ))),
            Err(error) => {
                let _ = tx.send(HostTask::Login(Box::new(DeviceLoginView::failed(
                    error.to_string(),
                ))));
                return;
            }
        };
        let tx_prompt = tx.clone();
        let result = block_on(auth.device_code_login(|prompt| {
            let _ = tx_prompt.send(HostTask::Login(Box::new(DeviceLoginView::waiting(
                &prompt.user_code,
                &prompt.verification_uri,
                prompt.expires_in.as_secs(),
                &format!("{}#{}", prompt.verification_uri, prompt.user_code),
            ))));
        }));
        if job.is_cancelled() {
            let _ = tx.send(HostTask::Login(Box::default()));
            return;
        }
        match result {
            Ok(session) => {
                let meta = unlucky_core::auth::account::AccountMeta::new(
                    unlucky_core::auth::account::AccountKind::Microsoft,
                    session.profile_id.clone(),
                    session.profile_name.clone(),
                    now_ms(),
                );
                let name = meta.display_name().to_string();
                let mut store = AccountStore::open(env.layout.auth_data());
                let saved = store
                    .insert(meta, Credentials::Microsoft(session))
                    .and_then(|()| store.save());
                match saved {
                    Ok(()) => {
                        let _ =
                            tx.send(HostTask::Login(Box::new(DeviceLoginView::succeeded(&name))));
                        let _ = tx.send(HostTask::Accounts(accounts(&env, None)));
                    }
                    Err(error) => {
                        let _ = tx.send(HostTask::Login(Box::new(DeviceLoginView::failed(
                            format!("账户入库失败：{error}"),
                        ))));
                    }
                }
            }
            Err(error) => {
                let _ = tx.send(HostTask::Login(Box::new(DeviceLoginView::failed(
                    error.to_string(),
                ))));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_display_matches_known_timestamps() {
        assert_eq!(display_ms(0), "");
        // 1_769_000_000_000 ms = 2026-01-21T12:53:20Z（取整到分钟）
        assert_eq!(display_ms(1_769_000_000_000), "2026-01-21 12:53");
    }

    #[test]
    fn offline_add_list_remove_round_trips() {
        let dir = std::env::temp_dir().join(format!("ull-host-accounts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let env = HostEnv::at_root(&dir);
        std::fs::create_dir_all(env.layout.auth_data()).expect("auth 目录");

        let name = add_offline(&env, "Steve").expect("加账户");
        assert_eq!(name, "Steve");
        let list = accounts(&env, None);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, AccountKind::Offline);
        assert!(!list[0].active);

        let active = accounts(&env, Some(list[0].id.as_str()));
        assert!(active[0].active);

        let removed = remove(&env, "Steve").expect("删账户");
        assert_eq!(removed, "Steve");
        assert!(accounts(&env, None).is_empty());
        assert!(add_offline(&env, "  ").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn login_job_cancel_flag_is_shared() {
        let job = LoginJob::new();
        let clone = job.clone();
        assert!(!clone.is_cancelled());
        job.cancel();
        assert!(clone.is_cancelled());
    }
}
