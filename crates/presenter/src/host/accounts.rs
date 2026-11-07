//! 账户：三轨统一列表（只读公开字段）+ 离线新增 + 删除/切换 + 微软登录（两条流）。
//!
//! 凭据红线（D9）：`Credentials` / `MicrosoftSession` 只在本模块与 `launch.rs` 里出现，
//! 出口一律是 [`AccountView`] / [`DeviceLoginView`]。
//!
//! 两条登录流：授权码（[`start_browser_login`]，GUI 默认：拉起系统浏览器 + 本地回调，
//! `research/08` §3 方案 A）与设备码（[`start_login`]，无浏览器/CLI 场景）。除了「怎么拿到
//! MS access_token」之外完全共用：同一个取消槽位、同一段账户入库、同一个刷新链。
//!
//! 取消语义：轮询/等回调都跑在 `core` 里、没有取消钩子，所以取消只把面板收回；线程收尾时
//! 若发现「已取消」就不入库（避免用户点了取消却凭空多出一个账户）。
//!
//! M132：缺 client id 只在**登录入口**失败（`auth_failure`），所以说明接在账户页的登录面板，
//! 不接启动流程——没配 id 的发布版根本建不出微软账户，启动那条路用户撞不到。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use unlucky_core::auth::account::Credentials;
use unlucky_core::auth::microsoft::{now_ms, MicrosoftAuth, MicrosoftSession};
use unlucky_core::auth::offline::offline_meta;
use unlucky_core::auth::{AccountStore, AuthError, AuthResult};
use unlucky_core::download::blocking::block_on;

use crate::dto::CoreEvent;
use crate::dto::{
    valid_offline_name, AccountKind, AccountNotice, AccountNoticeCode, AccountView,
    DeviceLoginView, LoginFailure,
};

use super::env::HostEnv;
use super::job::{spawn, HostTask, JobSender};
use super::opener;

/// 第三方轨（服务发现 / 密码登录 / 皮肤抓取）。
pub mod authlib;

mod refresh;

mod failure;

pub use failure::AccountFailure;

use failure::classify;

pub use authlib::{fetch_skin, spawn_probe, start_login as start_authlib_login};

impl super::CoreHost {
    /// 推一条账户页横幅（一次性操作结果；页面渲染后由用户关掉）。
    pub(super) fn notice(&mut self, code: AccountNoticeCode, subject: &str, detail: &str) {
        self.send(CoreEvent::AccountNoticeChanged(Some(AccountNotice::new(
            code, subject, detail,
        ))));
    }

    /// 账户写操作失败：**原文进日志**，页面只拿结构化码 + 主题名。
    pub(super) fn notice_failure(&mut self, failure: AccountFailure, subject: &str) {
        eprintln!(
            "[accounts] 操作失败（{}，subject={subject}）：{}",
            failure.code.code(),
            failure.detail
        );
        self.notice(failure.code, subject, &failure.detail);
    }
}

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
            server_url: meta.server_url.clone().unwrap_or_default(),
            // 皮肤是网络产物：列表先给 `None`，host 抓到之后按账户 id 盖回去（见 `CoreHost`）
            skin: None,
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
/// 名字不合法、同名冲突、落盘失败时返回 [`AccountFailure`]（带结构化码）。
pub fn add_offline(env: &HostEnv, name: &str) -> Result<String, AccountFailure> {
    let name = name.trim();
    if !valid_offline_name(name) {
        return Err(AccountFailure {
            code: AccountNoticeCode::OfflineNameInvalid,
            detail: format!("离线名字必须是 3–16 位字母/数字/下划线，收到 {name:?}"),
        });
    }
    let meta = offline_meta(name, now_ms())
        .map_err(|error| AccountFailure::classify(error, AccountNoticeCode::OfflineNameInvalid))?;
    let display = meta.display_name().to_string();
    let mut store = AccountStore::open(env.layout.auth_data());
    store
        .insert(meta, Credentials::Offline)
        .map_err(|error| AccountFailure::classify(error, AccountNoticeCode::AccountExists))?;
    store
        .save()
        .map_err(|error| AccountFailure::classify(error, AccountNoticeCode::StoreFailed))?;
    Ok(display)
}

/// 删除账户（按 id 或名字）。
///
/// # Errors
///
/// 找不到、落盘失败时返回 [`AccountFailure`]。
pub fn remove(env: &HostEnv, key: &str) -> Result<String, AccountFailure> {
    let mut store = AccountStore::open(env.layout.auth_data());
    let meta = store
        .remove(key)
        .map_err(|error| AccountFailure::classify(error, AccountNoticeCode::NotFound))?;
    store
        .save()
        .map_err(|error| AccountFailure::classify(error, AccountNoticeCode::StoreFailed))?;
    Ok(meta.display_name().to_string())
}

/// 微软登录任务句柄（取消标志 + 账户名回执；两条流共用）。
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
        let auth = match microsoft_auth(&env) {
            Ok(auth) => auth,
            Err(error) => return fail(&tx, auth_failure(&error), error.to_string()),
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
        finish(&env, &tx, &job, result);
    });
}

/// 起一次授权码登录（后台线程：绑本地端口 → 拉起系统默认浏览器 → 等回调 → 换链）。
///
/// 与设备码流共用同一个 [`LoginJob`] 取消槽位、同一段账户入库逻辑，所以两条流的
/// 凭据/刷新语义完全一致（`core` 侧也共用 [`unlucky_core::auth::MicrosoftSession`]）。
pub fn start_browser_login(env: HostEnv, tx: JobSender, job: LoginJob) {
    spawn("msa-authcode", move || {
        let auth = match microsoft_auth(&env) {
            Ok(auth) => auth,
            Err(error) => return fail(&tx, auth_failure(&error), error.to_string()),
        };
        let tx_prompt = tx.clone();
        let result = block_on(auth.authcode_login(|prompt| {
            // 浏览器拉不起来不是致命错误：面板里还有可复制的授权链接。
            let opened = opener::open_url(&prompt.authorization_url);
            let _ = tx_prompt.send(HostTask::Login(Box::new(DeviceLoginView::browser_waiting(
                &prompt.authorization_url,
                prompt.timeout.as_secs(),
                opened.err().as_deref(),
            ))));
        }));
        finish(&env, &tx, &job, result);
    });
}

/// 生产入口的 `MicrosoftAuth`：client id 取环境/构建期，失败日志落 `<数据根>/logs/`。
fn microsoft_auth(env: &HostEnv) -> AuthResult<MicrosoftAuth> {
    MicrosoftAuth::from_env().map(|auth| {
        auth.with_journal(Some(unlucky_core::auth::AuthJournal::in_logs_dir(
            env.layout.logs(),
        )))
    })
}

/// `microsoft_auth` 失败 → 原因码。
///
/// **只有「两处都没配 client id」单独成码**（M132）：`from_env` 的 `Unsupported` 是它唯一的
/// 产出点，且这条路径零网络。上屏的正文是 catalog 里的固定文案（原文照旧只进日志），理由是
/// 这一类的原因就是用户的下一步（设 `ULL_MSA_CLIENT_ID`）；其余失败仍走 [`classify`]。
fn auth_failure(error: &AuthError) -> LoginFailure {
    match error {
        AuthError::Unsupported(_) => LoginFailure::MsaNotConfigured,
        other => classify(other),
    }
}

/// 失败收尾（带**原因码**）：原文进日志，页面据码出 i18n（复审 r1 P1-1）。
fn fail(tx: &JobSender, failure: LoginFailure, detail: impl Into<String>) {
    let detail = detail.into();
    eprintln!("[auth] 微软登录失败（{}）：{detail}", failure.code());
    let _ = tx.send(HostTask::Login(Box::new(DeviceLoginView::failed_as(
        failure, detail,
    ))));
}

/// 收尾：取消就吞掉结果；成功入库；失败带 core 的诊断文案（M38 的 403 也走这条）。
fn finish(env: &HostEnv, tx: &JobSender, job: &LoginJob, result: AuthResult<MicrosoftSession>) {
    if job.is_cancelled() {
        let _ = tx.send(HostTask::Login(Box::default()));
        return;
    }
    match result {
        Ok(session) => match store_session(env, session) {
            Ok(name) => {
                let _ = tx.send(HostTask::Login(Box::new(DeviceLoginView::succeeded(&name))));
                let _ = tx.send(HostTask::Accounts(accounts(env, None)));
            }
            Err(message) => fail(tx, LoginFailure::StoreFailed, message),
        },
        Err(error) => {
            let failure = classify(&error);
            fail(tx, failure, error.to_string());
        }
    }
}

/// 微软会话入库（元数据 + 凭据分文件，见 `core::auth::AccountStore`）。
fn store_session(env: &HostEnv, session: MicrosoftSession) -> Result<String, String> {
    let meta = unlucky_core::auth::account::AccountMeta::new(
        unlucky_core::auth::account::AccountKind::Microsoft,
        session.profile_id.clone(),
        session.profile_name.clone(),
        now_ms(),
    );
    let name = meta.display_name().to_string();
    let mut store = AccountStore::open(env.layout.auth_data());
    store
        .insert(meta, Credentials::Microsoft(session))
        .and_then(|()| store.save())
        .map_err(|error| format!("账户入库失败：{error}"))?;
    Ok(name)
}

#[cfg(test)]
mod tests;
