//! 升级尝试标记 + 「新版起不来就退回上一版」（M309）。
//!
//! # 为什么要有本模块
//!
//! [`super::install`] 只在**安装期**失败时回滚：改名/占用/跨卷撞墙 ⇒ 旧版本原封不动。
//! 但装在盘上的新版本**起不来**（启动即崩、或卡在启动阶段）这件事，安装期一律看不见——
//! 那一刻新 exe 还没被跑过一次。于是自动更新闭环上留着一个缺口：装成功 = 永远留住，
//! 哪怕它是个一启动就死的构建。
//!
//! 本模块把「装上去了」与「跑得起来」分成两件事：
//!
//! ```text
//! apply 成功（拿到 InstallReceipt）
//!      └─ arm(...)                          落「升级尝试」标记（含旧版备份路径）
//! 新版本启动
//!      ├─ reconcile(...) 超窗口 ⇒ rollback  旧版备份搬回 dest、清标记、写 guard + 一次性凭证
//!      ├─ reconcile(...) 窗口内 ⇒ 只登记    「正常首次启动途中」，绝不回滚
//!      └─ confirm(...)   进了主界面        清标记 + 删旧备份（成功时不弹任何提示）
//! ```
//!
//! # 标记放哪
//!
//! `<数据根>/cache/update/upgrade-attempt.json`（[`attempt_path`]，与暂存目录同一个 `update` 域）。
//! **不放安装目录**：那里可能只读、可能与数据根不同卷，而标记每次启动都要读。碰盘动作在
//! `super::rollback_io`。
//!
//! # 阈值从哪算起（这条最容易做错）
//!
//! [`TRIAL_DEADLINE_MS`] 从**新版第一次启动**（`trial.started_ms`）算起，不是从写标记那一刻
//! 算起。按「标记写下的时间」判定会立刻产生一个假回滚：用户装完更新就把启动器关了，一周后
//! 再打开——那一刻新版本还没被跑过一次，却已经「超时」了。所以第一次启动只做登记
//! （[`Action::TrialRecorded`]），窗口内的后续启动也只累加次数；窗口**过了**还没进主界面，
//! 才判定起不来。
//!
//! # 判定表（状态 × 时间 × `starts` ⇒ 动作）
//!
//! | 标记 | `to` vs 正在跑的版本 | 距 `trial.started_ms` | guard 命中 | 动作 |
//! |---|---|---|---|---|
//! | 没有 | — | — | — | [`Action::Nothing`]（只是把上一次的一次性凭证说掉） |
//! | 有 | 不是这一版 | — | — | [`Action::Nothing`]，并丢掉这份过期标记 |
//! | 有 | 是 | 窗口内（含刚登记） | — | [`Action::TrialRecorded`]，`starts += 1` |
//! | 有 | 是 | 超窗口 | 否 | [`Action::RolledBack`]（原因：`starts >= 2` ⇒ 崩；否则 ⇒ 超时） |
//! | 有 | 是 | 超窗口 | 是 | [`Action::GaveUp`]（不再搬文件，只给具名结论） |
//! | 有 | 是 | 超窗口 | 否，搬不动 | [`Action::RollbackFailed`]（旧版仍在备份名上，需人工恢复） |
//!
//! # 回滚后为什么不会无限循环（本条的命门）
//!
//! 三道闸，缺一条就会变成「起不来 → 回滚 → 又判一次 → 又回滚」：
//!
//! 1. **回滚分支清标记**：没有标记就没有下一次判定，这是最硬的一道；
//! 2. **`rollback-guard.json`**：记下「这个 `to` 已被回退过一次」。同一 `to` 的标记再次出现
//!    （用户重装同一份坏构建、或崩溃现场留下的残迹）且又过了窗口时只给 [`Action::GaveUp`]，
//!    **绝不**再搬文件；更高的版本不受影响。停的是「搬文件」不是「观察」——重装仍照给一个窗口；
//! 3. **`rollback-notice.json` 是一次性凭证**：回滚当场说一次 + 落一份，下一次启动读一次就删。
//!    于是「回滚那个进程自己也崩了」不会让用户一次都看不到，也不会变成每次必弹的常驻提示。
//!
//! # 本模块**不**负责的事
//!
//! * 不判断「新版本是不是好版本」——只认「起了没起」（版本对不对由 [`super::checker`]）；
//! * 不改 [`super::install`] 的任何语义：本模块叠在它**外面**，用它的 [`InstallReceipt`] 当输入；
//! * 不写 UI 文案（壳层按 i18n 现求）：[`Notice`] 只有结构，没有一个自然语言串。
//!
//! # 单元测试覆盖到哪
//!
//! 四条情形（首次成功 / 窗口内不回滚 / 超窗口回滚一次 / 同一版本不再回滚）加 guard、一次性
//! 凭证与「标记写不下来」见 `tests/rollback_cases.rs`；回滚那两步搬不动的两条（注入器）见
//! `tests/rollback_io_cases.rs`。**都没跑真进程**：起不来的新版本是用「标记里 `trial.started_ms`
//! 落在窗口之外」造出来的，不是真的让它崩。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::{UpdateError, UpdateErrorKind};
use super::install::{InstallReceipt, ReplacePolicy};
use super::replace_io::{RealFs, ReplaceFs};
use super::rollback_io::{
    clear_attempt, read_attempt, read_guard, roll_back_with, take_notice, write_attempt,
    write_guard, write_notice,
};

pub use super::rollback_io::attempt_path;

/// 新版**第一次启动之后**允许它「还没进主界面」的窗口（毫秒）。
///
/// 2 分钟：真实启动到主界面是秒级（后台扫描都发生在主界面之后），这个窗口留够慢机器 + 杀软
/// 首次扫描；再大就会让「崩了又立刻重开」的用户长时间留在坏版本上。
pub const TRIAL_DEADLINE_MS: u64 = 120_000;

/// 标记文件的格式版本（读不懂就整条机制不启动，绝不按猜的字段去搬用户的 exe）。
pub(crate) const SCHEMA: u32 = 1;

/// 一次「新版启动尝试」的记账。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trial {
    /// 新版**第一次**被启动的时刻（毫秒 since UNIX_EPOCH；窗口从这里算）。
    pub started_ms: u64,
    /// 新版被启动过几次（含当前这次）。
    pub starts: u32,
}

/// 「升级尝试」标记：装上新版的那一刻写，进了主界面才清。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    /// 格式版本（`SCHEMA`）。
    pub schema: u32,
    /// 旧版本号（回滚回去的就是它）。
    pub from: String,
    /// 新版本号（可能是起不来的那个）。
    pub to: String,
    /// 被替换的目标（启动器本体路径）。
    pub dest: PathBuf,
    /// 旧版本备份（[`super::install`] 留在 dest 同目录的 `.ullold-*`）。
    pub backup: PathBuf,
    /// 标记写下的时刻（只用于诊断；判定看 [`Trial::started_ms`]）。
    pub written_ms: u64,
    /// 启动记账（`None` = 新版还一次都没被启动过）。
    pub trial: Option<Trial>,
}

/// 为什么会回滚（用户看到的那一格原因）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RollbackCause {
    /// 注册过不止一次启动，但都没进主界面 ⇒ 启动即崩。
    Crash,
    /// 只启动过一次、却过了窗口还没进主界面 ⇒ 启动阶段挂住 / 被杀。
    Timeout,
}

impl RollbackCause {
    /// 稳定 id（日志 / 诊断用；**不要改名**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Crash => "crash",
            Self::Timeout => "timeout",
        }
    }
}

/// 一条要告诉用户的结论属于哪一类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoticeKind {
    /// 已经退回上一版。
    RolledBack,
    /// 这一版之前已经回退过，这次不再自动回退。
    GaveUp,
    /// 该回滚但没搬动（旧版本还在备份名上）。
    RollbackFailed,
}

/// 一条给用户看的结论（壳层按 `kind` + `cause` 现求文案，这里不带任何自然语言）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    /// 哪一类结论。
    pub kind: NoticeKind,
    /// 回退到的旧版本号。
    pub from: String,
    /// 出问题的新版本号。
    pub to: String,
    /// 原因（`GaveUp` 沿用当初那次的原因）。
    pub cause: RollbackCause,
    /// 补充细节（只有 [`NoticeKind::RollbackFailed`] 会填；其余是空串）。
    pub detail: String,
}

/// 这次启动对「升级尝试」做了什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// 没有标记、标记不属于正在跑的这一版、或读不懂：什么都没做。
    Nothing,
    /// 登记（或累加）了一次新版启动，**没有**回滚（还在窗口内）。
    TrialRecorded,
    /// 已经回滚（旧版本回到了 `dest`）。
    RolledBack,
    /// 这个版本之前已经回退过：不再自动回滚，只给具名结论。
    GaveUp,
    /// 该回滚但搬运失败（旧版本还在备份名上，需要人工处理）。
    RollbackFailed,
}

/// [`reconcile`] 的返回值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// 做了什么。
    pub action: Action,
    /// 要**现在**告诉用户的结论（`None` = 一切正常，别弹任何东西）。
    ///
    /// 两个来源：上一次进程留下的一次性凭证（回滚进程当场死掉的那条路），或本次刚写下的。
    pub notice: Option<Notice>,
}

/// [`confirm`] 的返回值（成功路径**不产生任何用户可见结论**，这两格只用于日志与单测）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Confirmed {
    /// 有没有清掉标记（`false` = 本来就没有这一版待确认的标记）。
    pub marker_cleared: bool,
    /// 有没有删掉旧版本备份（`false` = 没有备份、或删不掉）。
    pub backup_removed: bool,
}

/// 装上新版之后的第一件事：**落标记**；落不下去就把新版当场退回去。
///
/// 落不下来（数据根的 cache 不可写：只读盘、权限、盘满）意味着这条更新**没有安全网**——
/// 装上去的新版将来起不来时，没有任何东西会把它退回去。所以处置是「宁可不装」：当场
/// [`InstallReceipt::rollback`] 回旧版本，并把原因如实报出去（UI 上就是一次具名的更新失败，
/// 不是静默跳过）。这条路径在**只读数据根的机器**上是真的会走到的。
///
/// `receipt.backup` 为 `None`（安装前目标本来就不存在）时**什么都不写**：没有可回退的东西，
/// 标记写了也只能在超时后做一次空搬动。
///
/// # Errors
///
/// 标记落不下去时返回 [`UpdateError`]（分类 `io`）；返回前已经尽力回滚，回滚也失败时错误
/// 文案会明说「目标路径上可能仍是新版本」——不许把两种结局混成一句话。
pub fn arm(
    mut receipt: InstallReceipt,
    cache: &Path,
    from: &str,
    to: &str,
    now_ms: u64,
) -> Result<InstallReceipt, UpdateError> {
    let Some(backup) = receipt.backup.clone() else {
        return Ok(receipt);
    };
    let attempt = Attempt {
        schema: SCHEMA,
        from: from.to_string(),
        to: to.to_string(),
        dest: receipt.dest.clone(),
        backup,
        written_ms: now_ms,
        trial: None,
    };
    match write_attempt(cache, &attempt) {
        Ok(()) => Ok(receipt),
        Err(error) => {
            let path = attempt_path(cache);
            let detail = UpdateError::io_at(&path, &error);
            let rolled_back = receipt.rollback(ReplacePolicy::default()).is_ok();
            let tail = if rolled_back {
                "已退回旧版本，这次更新没有装上"
            } else {
                "且退回旧版本失败：目标路径上可能仍是新版本，请手动重新安装"
            };
            Err(UpdateError::new(
                UpdateErrorKind::Io,
                format!("{detail}（{tail}）"),
            ))
        }
    }
}

/// 一次启动的对账：登记、回滚、或（同一版本已经退过一次）停手。
///
/// 从不返回错误：这是一条**启动路径**，读盘/写盘失败只能是「这次不判」，绝不能把启动器拦下来。
/// 写不进去的后果是「下次仍判为窗口内」——即少回滚一次，比拦下启动轻得多。
#[must_use]
pub fn reconcile(cache: &Path, current: &str, now_ms: u64) -> Outcome {
    reconcile_with(&RealFs, ReplacePolicy::default(), cache, current, now_ms)
}

/// [`reconcile`] 的注入版：回滚那两步改名走调用方给的 [`ReplaceFs`]（与 `install::install_with`
/// 同一条形状——注入器只能改「这一次改名成不成功」，改不了判据与顺序）。
///
/// 判据与生产路径完全相同；`rollback_io` 里「第二步失败就把停车位挪回去」那条恢复路径只有靠它
/// 才可能被单测打到（真盘上没法确定性复现）。
#[must_use]
pub(crate) fn reconcile_with(
    fs: &dyn ReplaceFs,
    policy: ReplacePolicy,
    cache: &Path,
    current: &str,
    now_ms: u64,
) -> Outcome {
    // 一次性凭证先取走：不管下面走哪一支，它都只能被说一次。
    let carried = take_notice(cache);
    let Some(mut attempt) = read_attempt(cache) else {
        return done(Action::Nothing, carried, None);
    };

    // 标记不是给「我现在跑的这版」的（已经回滚过、用户手工换回旧版、或换了目标）：
    // 它已经没有任何判定价值，丢掉，一个文件都不搬。
    if attempt.to != current {
        clear_attempt(cache);
        return done(Action::Nothing, carried, None);
    }

    let Some(trial) = attempt.trial else {
        // 新版第一次启动：把窗口起点记下来，**绝不**在这一刻回滚（正在正常启动途中）。
        attempt.trial = Some(Trial {
            started_ms: now_ms,
            starts: 1,
        });
        write_attempt(cache, &attempt).ok();
        return done(Action::TrialRecorded, carried, None);
    };

    if now_ms.saturating_sub(trial.started_ms) < TRIAL_DEADLINE_MS {
        // 窗口内：又启动了一次，但仍然可能是「正常首次启动途中」——不回滚，只记账。
        attempt.trial = Some(Trial {
            starts: trial.starts.saturating_add(1),
            ..trial
        });
        write_attempt(cache, &attempt).ok();
        return done(Action::TrialRecorded, carried, None);
    }

    // 窗口过了还没进主界面。**停手判定放在这里而不是更早**：guard 要说的是「不再**动手回滚**」，
    // 不是「不再观察」——同一份构建重装一次仍然值得给它一个诚实的启动窗口。
    let cause = cause_of(&attempt);
    if read_guard(cache).as_deref() == Some(attempt.to.as_str()) {
        clear_attempt(cache);
        let notice = Notice {
            kind: NoticeKind::GaveUp,
            from: attempt.from.clone(),
            to: attempt.to.clone(),
            cause,
            detail: String::new(),
        };
        write_notice(cache, &notice);
        return done(Action::GaveUp, carried, Some(notice));
    }

    // 窗口过了还没进主界面：这一次启动就把它退回去。
    match roll_back_with(fs, &attempt, policy) {
        Ok(()) => {
            // 命门第一道：回滚之后**必须**清标记，否则下一次启动会拿同一份标记再判一次。
            clear_attempt(cache);
            // 命门第二道：把「这个 to 已经退过一次」钉住，同一版本再来就不再自动回滚。
            write_guard(cache, &attempt.to);
            let notice = Notice {
                kind: NoticeKind::RolledBack,
                from: attempt.from.clone(),
                to: attempt.to.clone(),
                cause,
                detail: String::new(),
            };
            // 命门第三道：留一份一次性凭证（回滚进程若当场死掉，下一次启动仍说得出来）。
            write_notice(cache, &notice);
            done(Action::RolledBack, carried, Some(notice))
        }
        Err(error) => {
            // 搬不动也要清标记：留着它只会让每次启动都重演一次失败。
            clear_attempt(cache);
            let notice = Notice {
                kind: NoticeKind::RollbackFailed,
                from: attempt.from.clone(),
                to: attempt.to.clone(),
                cause,
                detail: error.to_string(),
            };
            write_notice(cache, &notice);
            done(Action::RollbackFailed, carried, Some(notice))
        }
    }
}

/// 新版**进了主界面**：清标记 + 删旧版本备份。成功路径不产生任何用户可见结论。
///
/// 这也是 [`InstallReceipt::remove_backup`] 的第一个生产调用方：早先它只有单测在调，
/// 于是每更新一次就在安装目录里留下一个几十 MB 的旧 exe。
#[must_use]
pub fn confirm(cache: &Path, current: &str) -> Confirmed {
    let Some(attempt) = read_attempt(cache) else {
        return Confirmed {
            marker_cleared: false,
            backup_removed: false,
        };
    };
    if attempt.to != current {
        // 不是这一版的记账：不动它（别的版本的尝试由它自己的启动去收）。
        return Confirmed {
            marker_cleared: false,
            backup_removed: false,
        };
    }
    clear_attempt(cache);
    Confirmed {
        marker_cleared: true,
        backup_removed: fs::remove_file(&attempt.backup).is_ok(),
    }
}

/// 结论拼装：本次写的优先，其次才是上一次留下的一次性凭证（两者不会同时有）。
fn done(action: Action, carried: Option<Notice>, written: Option<Notice>) -> Outcome {
    Outcome {
        action,
        notice: written.or(carried),
    }
}

/// 回滚原因：注册过不止一次启动 ⇒ 崩；只启动过一次 ⇒ 挂住/被杀。
const fn cause_of(attempt: &Attempt) -> RollbackCause {
    match attempt.trial {
        Some(trial) if trial.starts >= 2 => RollbackCause::Crash,
        _ => RollbackCause::Timeout,
    }
}
