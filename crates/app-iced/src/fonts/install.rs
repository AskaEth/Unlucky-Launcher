//! 中文字体下载的**执行与状态**（M168）：谁触发、什么时候触发、结果怎么被看见。
//!
//! ## 一句话
//!
//! ```text
//! i18n::font::decide（判据）          →  Strategy
//! fonts::install::after_probe（触发）  →  此刻要不要开始下载（一份进程级状态）
//! fonts::cdn::CdnFetcher（下载）      →  bytes 或三类错误
//! fonts::install::register（注册）    →  iced 的字体系统里多一个族
//! ```
//!
//! ## 两个触发点（都在 `fonts.rs`，因为那里是判据的两个调用点）
//!
//! | 入口 | 时机 | 谁来 |
//! |---|---|---|
//! | [`super::resolve_with`] | 启动（按系统 locale / `ULL_LANG`） | `app/boot.rs` |
//! | [`super::cjk_for`] | 设置页切语言（`ui/lang.rs::log_cjk`） | 用户手选中文 |
//!
//! 两条路都只把**判据结论**交给 [`after_probe`]：只有 `NeedsDownload` 才会真的开始下载
//! （英文环境 / 系统已有字体 / 探测不出，三路一次网络都不发；守卫见 `tests.rs`）。
//!
//! ## 为什么下载在后台线程而不是启动那一拍
//!
//! 一份 16.9 MiB 的字体在弱网下要几十秒。启动编排（`app/boot.rs`）是在**开窗之前**同步跑的，
//! 在那里下载等于「点了图标，窗口几十秒不出来」。所以：启动路径只**发起**，窗口照常起来，
//! 状态行（设置页 General 组）与 stderr 日志承担「正在获取 / 完成 / 失败」这三句话。
//!
//! ## 失败**不是**致命错误
//!
//! 任何一类失败（网络 / 校验 / 磁盘）都只让状态变成 [`CjkInstall::Failed`]：
//! 界面继续用系统字体（中文用户）+ 系统字体（英文用户），程序照常可用。这正是 D28 的要求
//! （「下载失败不崩」）。
//!
//! ## 状态是一份进程级快照
//!
//! `App` 每秒（心跳）比对一次快照，变了就重建视图模型（设置页那一行因此自己会刷新）。
//! 为什么不用 iced 的 `Task::stream` 推事件：本 crate 的执行器是 `thread-pool`（`Cargo.toml`
//! 的 feature 决定），推送通道要多一条生命周期不明的流；而「每秒看一眼快照」的代价是一把
//! 短锁 + 一次比较，且天然不会在窗口关掉之后还往一个死掉的 channel 里写。
//!
//! ## 截图通道
//!
//! `ULL_FONT_STATUS=` 可把状态钉成任意一档（`system` / `downloading` / `ready` / `failed:network`
//! / `not-needed` / `unprobeable`），只为了出图——与 `ULL_LANG` / `ULL_APPEARANCE_*` 同一族
//! 取证开关（D30 可复现：同一份代码出同一张图）。产品路径不设这个变量。

use std::sync::{Mutex, OnceLock};

use unlucky_i18n::font::{self, FetchError, Fetched, Fetcher, Strategy};

/// 失败类别（与 `unlucky_i18n::font::FetchError` 的三类一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchClass {
    /// 网络层。
    Network,
    /// 校验层（哈希 / 魔数 / 大小上限）。
    Integrity,
    /// 磁盘层。
    Disk,
}

impl FetchClass {
    /// 稳定类别名（日志用；UI 文案按它选 i18n key）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Integrity => "integrity",
            Self::Disk => "disk",
        }
    }
}

impl From<&FetchError> for FetchClass {
    fn from(error: &FetchError) -> Self {
        match error {
            FetchError::Network(_) => Self::Network,
            FetchError::Integrity(_) => Self::Integrity,
            FetchError::Disk(_) => Self::Disk,
        }
    }
}

/// 中文字体这一件的当前状态（设置页那一行与日志共用一份）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CjkInstall {
    /// 还没判定（启动前 / 判据没跑过）。
    Unknown,
    /// 英文环境：不需要中文字体（D28 的第一条）。
    NotNeeded,
    /// 系统里已有中文字体（含上一次下载后仍在字体库里的那份）。
    System {
        /// 命中的族名。
        family: String,
    },
    /// 探测不出（拿不到系统字体库）——**不下载**。
    Unprobeable,
    /// 正在下载。
    Downloading {
        /// 字节数（pin 里的期望值；用来写「16.9 MB」这种可核对的量）。
        bytes: u64,
    },
    /// 已就绪（下载完成并注册进字体系统）。
    Ready {
        /// 注册后的族名（真从字体库里读回来的，不是常量抄的）。
        family: String,
        /// 是否零网络命中缓存。
        from_cache: bool,
    },
    /// 失败（三类之一）——**不阻塞**，界面回落到系统字体照常可用。
    Failed {
        /// 类别。
        class: FetchClass,
        /// 人话原因（日志/诊断用；UI 只按类别选词）。
        message: String,
    },
}

impl CjkInstall {
    /// 是否正在下载（设置页要显示「进行中」，心跳也靠它决定要不要更勤地看图）。
    #[must_use]
    pub const fn is_downloading(&self) -> bool {
        matches!(self, Self::Downloading { .. })
    }

    /// 一行日志片段（`cjk-install=…`）。
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Unknown => "cjk-install=unknown".to_owned(),
            Self::NotNeeded => "cjk-install=not-needed".to_owned(),
            Self::System { family } => format!("cjk-install=system(\"{family}\")"),
            Self::Unprobeable => "cjk-install=unprobeable".to_owned(),
            Self::Downloading { bytes } => format!("cjk-install=downloading({bytes} bytes)"),
            Self::Ready { family, from_cache } => {
                format!("cjk-install=ready(\"{family}\",from_cache={from_cache})")
            }
            Self::Failed { class, message } => {
                format!("cjk-install=failed({},{message})", class.as_str())
            }
        }
    }
}

/// 进程级状态（一份）。
pub(super) fn slot() -> &'static Mutex<CjkInstall> {
    static STATE: OnceLock<Mutex<CjkInstall>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(CjkInstall::Unknown))
}

/// 当前状态（设置页那一行、心跳、日志都读它）。
#[must_use]
pub fn status() -> CjkInstall {
    if let Some(forced) = capture_override() {
        return forced;
    }
    slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// 截图通道的状态钉子（`ULL_FONT_STATUS=…`）。
fn capture_override() -> Option<CjkInstall> {
    static FORCED: OnceLock<Option<CjkInstall>> = OnceLock::new();
    FORCED
        .get_or_init(|| parse_capture(&std::env::var(CAPTURE_ENV).ok()?))
        .clone()
}

/// 取证开关名（与 `ULL_LANG` / `ULL_APPEARANCE_*` 同族）。
pub const CAPTURE_ENV: &str = "ULL_FONT_STATUS";

/// `ULL_FONT_STATUS` 的取值 → 状态（纯函数，截图夹具直接打这张表）。
#[must_use]
pub fn parse_capture(raw: &str) -> Option<CjkInstall> {
    let value = raw.trim();
    Some(match value {
        "not-needed" => CjkInstall::NotNeeded,
        "unprobeable" => CjkInstall::Unprobeable,
        "system" => CjkInstall::System {
            family: "Noto Sans SC".to_owned(),
        },
        "downloading" => CjkInstall::Downloading { bytes: 17_772_300 },
        "ready" => CjkInstall::Ready {
            family: "Noto Sans SC".to_owned(),
            from_cache: true,
        },
        "failed" | "failed:network" => CjkInstall::Failed {
            class: FetchClass::Network,
            message: "capture fixture".to_owned(),
        },
        "failed:integrity" => CjkInstall::Failed {
            class: FetchClass::Integrity,
            message: "capture fixture".to_owned(),
        },
        "failed:disk" => CjkInstall::Failed {
            class: FetchClass::Disk,
            message: "capture fixture".to_owned(),
        },
        _ => return None,
    })
}

/// 纯状态机：判据结论 + 下载器 + 注册器 → 结局（**不碰全局状态、不起线程**）。
///
/// 单测直接打这张表（守「不需要时一次都不联网」与「下载失败不崩」）。
pub fn ensure_for(
    strategy: Strategy,
    fetcher: &dyn Fetcher,
    register: &dyn Fn(Vec<u8>) -> Result<String, String>,
) -> CjkInstall {
    match font::ensure(strategy, fetcher) {
        Ok(Fetched::NothingToDo) => CjkInstall::NotNeeded,
        Ok(Fetched::AlreadySystem { family }) => CjkInstall::System {
            family: family.to_owned(),
        },
        Ok(Fetched::Unknown) => CjkInstall::Unprobeable,
        Ok(Fetched::Downloaded(bytes)) => match register(bytes) {
            Ok(family) => CjkInstall::Ready {
                family,
                from_cache: false,
            },
            Err(message) => CjkInstall::Failed {
                class: FetchClass::Disk,
                message,
            },
        },
        Err(error) => CjkInstall::Failed {
            class: FetchClass::from(&error),
            message: error.message().to_owned(),
        },
    }
}

/// 判据结论 + **当前状态** → 这一步该做什么（纯函数，不碰全局状态、不起线程）。
///
/// 这是幂等闸的**唯一判据**：`after_probe` 只是它的执行者。之所以拎成纯函数，是因为上一版
/// 有一条「自称守卫、其实什么都没测」的用例（复审 r1 P2-4）：它在循环里断言一个从没被传进任何
/// 函数的假下载器「没被调用」，恒绿。现在守卫直接打这张表——把 `Skip` 那一臂删掉，用例必红。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// 什么都不做（已经在下载 / 已就绪）。
    Skip,
    /// 只把状态改成这个（零网络）。
    Apply(CjkInstall),
    /// 真的发起一次下载。
    Start(&'static font::CjkFont),
}

/// 判据表（判据本体就在这张表的 `match` 里，`after_probe` 不含任何判据）。
///
/// | 当前状态 | 策略 | 这一步 |
/// |---|---|---|
/// | **下载中**（任意策略） | 任意 | **`Skip`**（闸在最上面，见下） |
/// | `Ready` | 任意 | `Skip`（幂等：已就绪不再发起） |
/// | 其它 | `NotNeeded` / `System` / `Unprobeable` | `Apply`（只改状态，零网络） |
/// | 其它 | `NeedsDownload` | `Start`（失败过的重试只能由用户再切一次语言触发） |
///
/// ## 为什么「下载中」要在**所有**策略之前先跳过（复审 r2 P2-2 ②）
///
/// 上一版把 `is_downloading()` 只放在 `NeedsDownload` 分支里，于是「下载中 + `Unprobeable`」
/// 会走 `Apply(Unprobeable)` —— 把状态从「正在下载」改回「无法探测」，而**线程还在飞**：
/// 下完之后 `Ready` 又把它盖回去，状态行在 30 秒里来回翻。当时给出的理由是「否则界面会停在
/// 正在下载」——那个理由只对**僵死**的 `Downloading` 成立（线程 panic 才会僵死），而在飞是
/// 常见情形。
///
/// ## 「僵死」那一半按构建档分两说（复审 r3 P2-1，别再当成一条统一的兜底）
///
/// | 档 | panic 语义 | 线程 panic 之后 |
/// |---|---|---|
/// | `release`（`Cargo.toml` 的 `panic = "abort"`，发布就走它） | 进程直接 abort | 不存在「停在 Downloading」，因为**进程没了**；反过来说「下载失败不崩」在 panic 这一支上对本档**不成立**（注册路径 panic ⇒ 启动器整体死掉，不是降级） |
/// | `dev` / `test` / `fast`（unwind） | 栈展开 | `worker` 的 `catch_unwind` 把状态写成 `Failed`，闸因此不会卡死 |
///
/// 也就是说：**闸前移在两种档下都安全**（release 靠「进程退出」，unwind 档靠 `catch_unwind`），
/// 但 `catch_unwind` **不能**用来给「下载失败不崩」背书——它只在 unwind 档存在。字体注册路径的
/// panic 在发布版仍会让启动器 abort，这是本 mission 未解决的一项（报告里列为未验/风险）。
///
/// `offline = true`（出图通道）时**永不** `Start`：D30 要求同一份代码出同一张图，而下载会额外
/// 产生网络流量并可能在出图过程中注册新字体。
#[must_use]
pub fn plan_with_offline(strategy: Strategy, current: &CjkInstall, offline: bool) -> Step {
    if current.is_downloading() || matches!(current, CjkInstall::Ready { .. }) {
        return Step::Skip;
    }
    match strategy {
        Strategy::NotNeeded => Step::Apply(CjkInstall::NotNeeded),
        Strategy::System { family } => Step::Apply(CjkInstall::System {
            family: family.to_owned(),
        }),
        Strategy::Unprobeable => Step::Apply(CjkInstall::Unprobeable),
        // 出图通道需要下载 ⇒ **什么都不做**（也不改状态）：写「不需要下载」是谎话，
        // 写「正在下载」也是谎话（并没有在下载）。
        Strategy::NeedsDownload(_) if offline => Step::Skip,
        Strategy::NeedsDownload(font) => Step::Start(font),
    }
}

/// 同 [`plan_with_offline`]，但按本进程当前的联网策略（生产判据入口）。
#[must_use]
pub fn plan(strategy: Strategy, current: &CjkInstall) -> Step {
    plan_with_offline(strategy, current, offline())
}

/// 判据结论 → 现在这一步的结局（生产入口；判据全在 [`plan`]，这里只执行）。
///
/// * 已经在下载 / 已就绪 → 什么都不做（幂等：切一次语言不会起第二条下载、不会重复注册 16.9 MiB）；
/// * 英文环境 / 系统已有 / 探测不出 → 只更新状态（零网络）；
/// * 上次失败过 → 只有再次切语言才会重试（用户显式动作），不会自己反复联网；
/// * 出图通道（[`set_offline`]）→ 永不发起。
pub fn after_probe(strategy: Strategy) -> CjkInstall {
    match plan(strategy, &status()) {
        Step::Skip => status(),
        Step::Apply(state) => {
            update(state.clone());
            state
        }
        Step::Start(font) => {
            worker::start(font);
            status()
        }
    }
}

/// 本进程是否**禁止**发起字体下载（出图通道用；D30：同一份代码出同一张图）。
///
/// 为什么需要它：截图/演示通道也会走 `App` 构造 → `lang.log_cjk()` → `cjk_for` → `after_probe`。
/// 一台 zh-locale、没有中文字体的机器上，出图会真的下 16.9 MiB 并在出图过程中注册新字体——
/// 这既产生流量，也让「同一份代码出同一张图」不再成立。
pub fn set_offline(offline: bool) {
    OFFLINE.store(offline, std::sync::atomic::Ordering::Relaxed);
}

/// 当前是否处于 [`set_offline`] 的状态。
#[must_use]
pub fn offline() -> bool {
    OFFLINE.load(std::sync::atomic::Ordering::Relaxed)
}

/// 进程级联网开关（默认允许）。
static OFFLINE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 状态变更时记一行（只有真变了才打，避免每秒刷一行）。
pub(super) fn update(state: CjkInstall) {
    let previous = {
        let mut guard = slot()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::replace(&mut *guard, state.clone())
    };
    if previous != state {
        eprintln!("[font] {}", state.describe());
    }
}

/// 下载线程 + 字体注册（`install/worker.rs`）：`start` 只在这里被触发。
mod worker;

#[cfg(test)]
#[path = "install/tests.rs"]
mod tests;
