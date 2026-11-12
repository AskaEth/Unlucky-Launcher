//! 「Java」分组的**扫描槽与生产作业**（M204 r1 从 `store.rs` 拆出：那个文件加了
//! `ScanSlot::seed_report` 之后 408 行，越过 400 行硬线）。
//!
//! 两件事：
//!
//! | 东西 | 为什么在这 |
//! |---|---|
//! | [`ScanSlot`] | 后台线程写、界面每拍读的快照 + 取消位 + 「线程还活着」。它是**每份 `State` 一个**的槽（不是进程级 static），理由见结构体文档 |
//! | [`run_staged`] / [`rows`] | 生产作业：真扫盘 + 逐个探测（超时与取消都在 `core::game::java::staged::scan` 里），并把 `JavaRuntime` 翻成页面行 |
//!
//! `store.rs` 用 `pub use` 把 [`Job`] / [`ScanReport`] / [`ScanSlot`] 再导出，所以
//! `pages::settings::java::store::ScanSlot` 这类既有路径**一个都不用改**。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use unlucky_core::game::java::{staged, JavaRuntime};

use super::state::RuntimeRow;

/// 扫描结果快照（后台线程写、界面读）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanReport {
    /// 扫描线程**还活着**（取消之后到它真退出之前仍是 `true`——这一段界面上叫「正在取消…」）
    pub running: bool,
    /// 已探测
    pub done: usize,
    /// 候选总数（`0` = 候选表还在收集，界面不显示 `0/0`）
    pub total: usize,
    /// 至少完整扫完过一次
    pub finished: bool,
    /// 用户按过取消（线程还活着时是 `true`）
    pub cancelled: bool,
    /// 扫描结论（`finished` 且没被取消时有效）
    pub runtimes: Vec<RuntimeRow>,
}

/// 一次扫描的作业（生产实现是 [`run_staged`]；测试注入可控的假作业）。
pub type Job =
    fn(&[staged::Candidate], &dyn Fn() -> bool, &mut dyn FnMut(usize, usize)) -> Vec<RuntimeRow>;

/// 一次扫描的**状态槽**：快照 + 取消位 + 「线程还活着」。
///
/// 为什么不是进程级 static（M175 r1 复审 P2-5 的附带项）：全局状态有两个坏处——
/// ①候选收集要在 UI 线程上抢同一把锁；②单测之间互相污染（一个用例起的扫描会让另一个用例
/// 看到「变了」）。挂在 `State` 上之后，每个 `App` / 每个用例都有一份自己的槽。
///
/// 内部是 `Arc`：`State` 会被整体替换（hydrate），而扫描线程要活得比某一次 `State` 赋值更久。
#[derive(Debug, Clone, Default)]
pub struct ScanSlot {
    inner: std::sync::Arc<ScanInner>,
}

#[derive(Debug, Default)]
struct ScanInner {
    report: Mutex<ScanReport>,
    cancel: AtomicBool,
}

impl ScanSlot {
    /// 当前快照（界面每拍读一次；锁只在这几行里持有）。
    #[must_use]
    pub fn report(&self) -> ScanReport {
        self.inner
            .report
            .lock()
            .map(|report| report.clone())
            .unwrap_or_default()
    }

    /// 把快照**直接摆成**给定值（只给 `--screenshot` 的种子用，M204 r1）。
    ///
    /// 为什么种子不能只写 `State.scanning`：`clock.advance` 每拍都调
    /// `hydrate_java_scan` → `State::apply_scan(&self.scan.report())`，而 `scanning`
    /// 是从**槽里的快照**推出来的。只改 `State` 的话，第一拍就会被推回「没在扫」，
    /// 出图拍到的是另一种状态。与 `downloads::Slot::seed_report` 同一个理由、同一种做法。
    ///
    /// 与 [`ScanSlot::start`] 的区别：这里不起线程、不摸取消位，只摆一个静态快照——
    /// 出图要的是**可复现**的一帧，不是真的在扫。
    pub fn seed_report(&self, report: ScanReport) {
        if let Ok(mut slot) = self.inner.report.lock() {
            *slot = report;
        }
    }

    /// 取消正在跑的扫描：置取消位 + 标 `cancelled`，**不**动 `running`。
    ///
    /// `running` 由扫描线程自己翻——它是「线程还活着」的唯一真相。取消若也翻它，`start()` 的
    /// 「已在扫」判据就失效，用户能在旧线程还卡着的时候起第二个（复审 P2-5 的第三个问题）。
    pub fn cancel(&self) {
        self.inner.cancel.store(true, Ordering::SeqCst);
        if let Ok(mut report) = self.inner.report.lock() {
            if report.running {
                report.cancelled = true;
            }
        }
    }

    /// 发起一次扫描（已在扫 ⇒ 什么都不做，返回 `false`）。
    pub fn start(&self, roots: Vec<PathBuf>, manual: Vec<PathBuf>, system: bool) -> bool {
        self.start_with(roots, manual, system, run_staged)
    }

    /// 同 [`ScanSlot::start`]，但扫描作业可注入（测试用：一个可控的、会一直卡住的作业）。
    pub fn start_with(
        &self,
        roots: Vec<PathBuf>,
        manual: Vec<PathBuf>,
        system: bool,
        job: Job,
    ) -> bool {
        {
            let Ok(mut report) = self.inner.report.lock() else {
                return false;
            };
            if report.running {
                return false;
            }
            // 「占位」与「线程起来了」在同一把锁里完成：两个调用不可能同时通过上面那道判据。
            *report = ScanReport {
                running: true,
                ..ScanReport::default()
            };
            self.inner.cancel.store(false, Ordering::SeqCst);
        }

        let slot = self.clone();
        std::thread::spawn(move || {
            // 候选收集（注册表 / 常见目录）放在线程里：它是首屏那一拍最贵的一段，
            // 放在 UI 线程上会拖慢开窗（复审 P2-5 的附带项）。
            let candidates = staged::candidates(&roots, &manual, system);
            if let Ok(mut report) = slot.inner.report.lock() {
                report.total = candidates.len();
            }
            let cancelled = || slot.inner.cancel.load(Ordering::SeqCst);
            let mut progress = |done: usize, _total: usize| {
                if let Ok(mut report) = slot.inner.report.lock() {
                    report.done = done;
                }
            };
            let runtimes = job(&candidates, &cancelled, &mut progress);
            if let Ok(mut report) = slot.inner.report.lock() {
                report.running = false;
                report.done = candidates.len();
                if !report.cancelled {
                    report.finished = true;
                    report.runtimes = runtimes;
                }
            }
        });
        true
    }
}

/// 生产扫描作业：真扫盘 + 逐个探测（超时与取消都在 `staged::scan` 里）。
fn run_staged(
    candidates: &[staged::Candidate],
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(usize, usize),
) -> Vec<RuntimeRow> {
    let found = staged::scan(
        candidates,
        |done, total| {
            progress(done, total);
            true
        },
        cancelled,
    );
    rows(&found, candidates)
}

/// `JavaRuntime` → 页面行（来源从候选表里按路径回查：`JavaRuntime.origin` 分不出「手动」）。
fn rows(found: &[JavaRuntime], candidates: &[staged::Candidate]) -> Vec<RuntimeRow> {
    found
        .iter()
        .map(|runtime| RuntimeRow {
            binary: runtime.binary.display().to_string(),
            version: runtime.version.clone().unwrap_or_default(),
            major: runtime.major,
            vendor: runtime.vendor.clone(),
            source: candidates
                .iter()
                .find(|candidate| candidate.binary == runtime.binary)
                .map_or("system", |candidate| candidate.source.id()),
        })
        .collect()
}
