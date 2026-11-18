//! 账户：三轨统一列表（只读公开字段）+ 离线新增 + 删除/切换 + 微软登录（两条流）。
//!
//! 凭据红线（D9）：`Credentials` / `MicrosoftSession` 只在本模块与 `launch.rs` 里出现，
//! 出口一律是 [`AccountView`] / [`DeviceLoginView`]。
//!
//! 两条登录流（M178 改了默认）：**设备码是默认**（[`start_login`]：不依赖任何应用注册项），
//! 授权码（[`start_browser_login`]，浏览器 + 本地回调）降级成「用户显式要求才试」的加速路，
//! 且带回环注册预检 + 失败自动回退设备码。除了「怎么拿到 MS access_token」之外完全共用：
//! 同一个取消槽位、同一段账户入库、同一个刷新链。
//!
//! 取消语义：轮询/等回调都跑在 `core` 里、没有取消钩子，所以取消只把面板收回；线程收尾时
//! 若发现「已取消」就不入库（避免用户点了取消却凭空多出一个账户）。
//!
//! M132：缺 client id 只在**登录入口**失败（`auth_failure`），所以说明接在账户页的登录面板，
//! 不接启动流程——没配 id 的发布版根本建不出微软账户，启动那条路用户撞不到。
//!
//! M236 拆分：两条流的编排搬到 `accounts/login.rs`（纯搬移，路径靠下面的 `pub use` 保住），
//! 本文件只留列表/增删/句柄/测试缝——原文件正好 400 行零余量，测试缝换身份前必须先拆。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use unlucky_core::auth::account::Credentials;
use unlucky_core::auth::microsoft::{now_ms, BrowserFallbackReason};
use unlucky_core::auth::offline::offline_meta;
use unlucky_core::auth::{AccountStore, AuthError};

use crate::dto::CoreEvent;
use crate::dto::{
    valid_offline_name, AccountKind, AccountNotice, AccountNoticeCode, AccountView,
    BrowserLoginNote, LoginFailure,
};

use super::env::HostEnv;

/// 第三方轨（服务发现 / 密码登录 / 皮肤抓取）。
pub mod authlib;

mod login;

mod refresh;

mod failure;

pub use failure::AccountFailure;

pub use login::{start_browser_login, start_login};

pub use authlib::{fetch_skin, spawn_probe, start_login as start_authlib_login};

use failure::classify;

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
#[derive(Debug, Clone)]
pub struct LoginJob {
    /// 用户点了取消（线程收尾时据此丢弃结果）
    pub cancelled: Arc<AtomicBool>,
    /// 测试缝的**稳定身份**（见 `test_seam`）：进程内单调递增、永不复用。
    ///
    /// 只在测试构建里存在——发布产物不需要它。它会存在，是因为「这条记录属于哪个 job」
    /// 不能用堆地址回答：地址是会被复用的资源，不是身份（M236 的 ABA 假红）。
    #[cfg(test)]
    id: JobId,
}

impl Default for LoginJob {
    /// 与 [`LoginJob::new`] 同一个东西：`id` 不许走 `Default` 的零值，否则两个 job 撞身份。
    fn default() -> Self {
        Self::new()
    }
}

impl LoginJob {
    /// 新建一个未取消的句柄。
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            id: JobId::next(),
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

/// [`LoginJob`] 的稳定身份：进程内单调递增，**永不复用**。
///
/// 为什么不用 `Arc::as_ptr(&job.cancelled)`：job 一 drop，它的控制块就可以被下一个
/// `LoginJob` 复用，于是「新 job」会读到「旧 job」记下的模式（ABA）——并行跑的用例因此
/// 偶发假红（实测形态：设备码入口读到别人的 `browser_first=true`）。
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct JobId(u64);

#[cfg(test)]
impl JobId {
    /// 取下一个身份。`Relaxed` 够用：只要求唯一与单调，不要求跨线程的可见性顺序。
    fn next() -> Self {
        use std::sync::atomic::AtomicU64;

        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

/// 回退原因码 → 页面说明码（两边各留一个枚举：presenter 的 DTO 不依赖 core 的中文诊断）。
fn browser_note(reason: BrowserFallbackReason) -> BrowserLoginNote {
    match reason {
        BrowserFallbackReason::RedirectNotRegistered => BrowserLoginNote::RedirectNotRegistered,
        BrowserFallbackReason::PreflightUnavailable => BrowserLoginNote::PreflightUnavailable,
        BrowserFallbackReason::AttemptFailed => BrowserLoginNote::AttemptFailed,
    }
}

/// 微软登录的两个入口（决定 `MicrosoftAuth::browser_first`，M178）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MsaEntry {
    /// 默认路：设备码。**一个监听口都不开**，也永远不做回环预检。
    DeviceCode,
    /// 用户显式点了「用浏览器登录」：浏览器优先（预检 → 授权码 → 失败自动回退设备码）。
    Browser,
}

impl MsaEntry {
    /// 这个入口要不要把 `browser_first` 置位。
    const fn browser_first(self) -> bool {
        matches!(self, Self::Browser)
    }
}

/// 测试缝：记下生产入口**实际请求**的模式，让守卫能钉住「按钮 → browser_first=true」这条线。
///
/// 为什么需要它：两条入口在没有 client id 的测试构建里都会在网络之前早退，拿不到
/// `MicrosoftAuth` 实例去问 `browser_first()`；而 P1-1 的 bug 恰恰在**调用点**。这里在被调用
/// 的一刻就把模式记下来，把调用点变成可断言的事实。
///
/// 为什么按 job 分桶：同一进程里别的用例也在调这两个入口，一个全局「最后一次」记录会串台
/// （实测过：并行跑时读到别人的 Browser ⇒ 假红）。
///
/// **身份是 [`JobId`]，不是堆地址**（M236）：`Arc::as_ptr` 那种做法会被地址复用骗到——job A
/// drop 之后控制块立刻分给 job B，B 于是读到 A 的记录（ABA 假红）。
#[cfg(test)]
mod test_seam {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex, Weak};

    use super::{JobId, LoginJob, MsaEntry};

    /// 表里的一条记录。
    ///
    /// `alive` 只用来回收「job 已经 drop」的条目，**不参与身份判定**（身份是 `job`）。它同时把
    /// 控制块钉住：`Weak` 还在时控制块不会释放，`strong_count() == 0` 因此读得准。
    pub(super) struct Entry {
        job: JobId,
        alive: Weak<AtomicBool>,
        browser_first: bool,
    }

    impl Entry {
        pub(super) fn new(job: JobId, cancelled: &Arc<AtomicBool>, browser_first: bool) -> Self {
            Self {
                job,
                alive: Arc::downgrade(cancelled),
                browser_first,
            }
        }

        /// job 还活着吗（`false` ⇒ 这条只是还没被收走的死条目）。
        pub(super) fn is_alive(&self) -> bool {
            self.alive.strong_count() > 0
        }

        pub(super) fn job(&self) -> JobId {
            self.job
        }

        pub(super) fn browser_first(&self) -> bool {
            self.browser_first
        }
    }

    static ENTRIES: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

    /// 写一条记录：**同一个 job 只留最后一次**，顺手收走已 drop 的 job。
    ///
    /// 「多条时取第一条」的歧义在这里被消掉：表里每个 job 恒为一条，所以 [`mode_for`] 取到的
    /// 那条就是它最后一次请求的模式。收死条目的 `retain` 也在这里——表只随活 job 增长。
    pub(super) fn remember(entries: &mut Vec<Entry>, entry: Entry) {
        entries.retain(Entry::is_alive);
        match entries.iter_mut().find(|known| known.job == entry.job) {
            Some(slot) => slot.browser_first = entry.browser_first,
            None => entries.push(entry),
        }
    }

    /// 生产入口被调用时记一笔。
    pub(super) fn record(job: &LoginJob, entry: MsaEntry) {
        let mut entries = ENTRIES.lock().unwrap_or_else(|error| error.into_inner());
        remember(
            &mut entries,
            Entry::new(job.id, &job.cancelled, entry.browser_first()),
        );
    }

    /// 这个 job 的生产构造请求的模式（`None` = 还没调用过）。
    pub(super) fn mode_for(job: &LoginJob) -> Option<bool> {
        let entries = ENTRIES.lock().unwrap_or_else(|error| error.into_inner());
        entries
            .iter()
            .find(|known| known.job == job.id)
            .map(Entry::browser_first)
    }

    /// 造一个「共用同一个取消槽、但确定是另一个 job」的句柄（ABA 用例的构造件）。
    ///
    /// 两个不同 job 指向同一个控制块：这正是 `Arc::as_ptr` 做 key 时无法区分的情形。
    pub(super) fn job_around(cancelled: &Arc<AtomicBool>) -> LoginJob {
        LoginJob {
            cancelled: Arc::clone(cancelled),
            id: JobId::next(),
        }
    }
}

/// `msa_auth` 失败 → 原因码。
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

#[cfg(test)]
mod tests;
