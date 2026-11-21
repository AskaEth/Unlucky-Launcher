//! 「Java」分组的**扫描槽与生产作业**（M204 r1 从 `store.rs` 拆出：那个文件加了
//! `ScanSlot::seed_report` 之后 408 行，越过 400 行硬线）。
//!
//! 两件事：
//!
//! | 东西 | 为什么在这 |
//! |---|---|
//! | [`ScanSlot`] | 后台线程写、界面每拍读的快照 + 取消位 + 「线程还活着」+ **排队位**。它是**每份 `State` 一个**的槽（不是进程级 static），理由见结构体文档 |
//! | [`run_staged`] / [`rows`] | 生产作业：真扫盘 + 逐个探测（超时与取消都在 `core::game::java::staged::scan` 里），并把 `JavaRuntime` 翻成页面行 |
//!
//! `store.rs` 用 `pub use` 把 [`Job`] / [`ScanReport`] / [`ScanSlot`] 再导出，所以
//! `pages::settings::java::store::ScanSlot` 这类既有路径**一个都不用改**。
//!
//! ## 在飞时再请一次扫描 = 排队，不是拒绝（M373）
//!
//! 现场（M370 挖出来的产品缺口）：首屏扫描还在跑时，用户在设置页保存「安装根」⇒ `start_with`
//! 撞上 `report.running` 直接 `return false`，而 `save_java_prefs` 照样把回执写成「已保存」⇒
//! **列表停在旧目录**，界面上没有一句说要重扫。
//!
//! 三条路的取舍（详见 `DevDocs/artifacts/rescan/M373.md`）：
//!
//! | 方案 | 代价 |
//! |---|---|
//! | ①**队列化**（本文件） | 在飞的那一轮扫完再按**新目录**扫一次；用户意图是「我改了目录，请扫它」，不是「请取消」 |
//! | ②取消并重启 | 用户只是点了保存，却把正在跑的扫描踹掉：已探测的候选全部重来、进度条倒回 0，还多一段 `cancel` 竞态 |
//! | ③只改界面文案 | 列表停在旧目录直到用户自己点「立即扫描」——正是本 mission 要修的那条缺陷 |
//!
//! 队列**只有一格**（`pending: Option<Queued>`，后来的盖掉先前的）：用户连点两次保存，
//! 值得跑的是**最后一次**那份落点，中间那份已经没有意义。

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

/// 一次「请扫描」的结果。
///
/// 为什么不是 `bool`（M373）：调用方要能分开「现在就扫了」与「排在在飞的那一轮后面」——
/// 界面上这两句话不同，而旧代码把后者的回执照样写成「已保存」，正是 M370 那条缺陷的文案根源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// 线程起来了（快照当场翻成「正在扫」，同一把锁里）
    Running,
    /// 已有一轮在飞：这一次**排队**，那一轮结束之后按这份新输入自动再跑一次
    Queued,
    /// 槽的状态锁被毒死了（前一个持有者在持锁时 panic 过）：既没起线程也没排队
    Broken,
}

/// 排队中的一次请求：**输入整份存下来**（新目录必须跟着这次请求走）。
///
/// 为什么不让线程醒来再读一次配置：那条路上配置可能又被改了一次，扫出来的结果就不是用户点保存
/// 时想要的那一份。为什么 `roots` / `manual` 是所有权形式：调用方交出来的那份就归槽了，
/// 不需要它活到线程跑完。
#[derive(Debug, Clone)]
struct Queued {
    roots: Vec<PathBuf>,
    manual: Vec<PathBuf>,
    system: bool,
    job: Job,
}

/// 一次扫描的**状态槽**：快照 + 取消位 + 「线程还活着」+ 排队位。
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
    /// 排队中的那一次请求，**只有一格**（后来的盖掉先前的）。
    ///
    /// 「读 `report.running`」与「写这里」在 `report` 的**同一段临界区**里完成；「把 `running`
    /// 翻回 `false`」与「取走这里」也在同一段临界区里完成。于是不存在「线程刚判完 `running`、
    /// 线程自己收尾、这次请求被静默丢掉」的窗口。
    ///
    /// 与 `report` 分开两把锁的理由：`report` 每拍被界面读一次（`report()` 克隆整份快照），
    /// 把 `Vec<PathBuf>` 塞进快照里会让每帧都克隆一遍路径。两把锁的次序固定为
    /// **先 `report` 后 `pending`**，且没有反过来的路径（不会死锁）。
    pending: Mutex<Option<Queued>>,
    /// 已**发起**的扫描次数（**只增不减**）。`report.running` 说的是「线程此刻还活着」，会随机器
    /// 快慢翻面（容器里没有 JDK 时扫描线程几十微秒就结束）；这个计数判的是「有没有再发起一次请求」，
    /// 与线程跑完没跑完无关。
    ///
    /// 注意语义：它记的是**请求**，包含「已有一轮在跑 ⇒ 这次排队（[`Outcome::Queued`]）」那种。
    /// 排队的那一次**不会**在真正跑起来时再 +1：它是同一个请求，只是晚一轮执行。
    ///
    /// `#[cfg(test)]`：只有单测读它（判「保存后必须重扫一次」）。产品构建里这个字段与那次自增
    /// **都不存在**，所以产品路径的指令一条都没变。
    #[cfg(test)]
    requests: std::sync::atomic::AtomicU64,
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

    /// 已**发起**的扫描请求次数（单调递增）。**单测专用**：`start_with` 在锁里 +1。
    ///
    /// 判「保存后必须重扫一次」这类用例只能用它：`report().running` 判的是「线程此刻还活着」，
    /// 那是个**时序**——机器快（容器里没有 JDK）就翻面，同一份代码在快/慢机器上给出相反结论
    /// （M370 实测：无 LANG 容器里 4 次跑 3 次假红）。
    ///
    /// 语义边界见 `ScanInner::requests`：它数是**请求**，含排队等下一轮的那次（不重数）。
    #[cfg(test)]
    #[must_use]
    pub(crate) fn scan_requests(&self) -> u64 {
        self.inner.requests.load(Ordering::SeqCst)
    }

    /// 现在有没有排着一次还没跑的扫描（**单测专用**：判「取消有没有把队列一起撤掉」）。
    #[cfg(test)]
    #[must_use]
    pub(crate) fn scan_queued(&self) -> bool {
        self.inner
            .pending
            .lock()
            .map(|pending| pending.is_some())
            .unwrap_or(false)
    }

    /// 把快照**直接摆成**给定值（只给 `--screenshot` 的种子用，M204 r1）。
    ///
    /// 为什么种子不能只写 `State.scanning`：`clock.advance` 每拍都调
    /// `hydrate_java_scan` → `State::apply_scan(&self.scan.report())`，而 `scanning`
    /// 是从**槽里的快照**推出来的。只改 `State` 的话，第一拍就会被推回「没在扫」，
    /// 出图拍到的是另一种状态。与 `downloads::Slot::seed_report` 同一个理由、同一种做法。
    ///
    /// 与 [`ScanSlot::start`] 的区别：这里不起线程、不摸取消位，只摆一个静态快照——
    /// 出图要的是**可复现**的一帧，不是真的在扫。**排队位一并清掉**：留着它，那一帧会被一轮
    /// 真扫盘改掉（种子档也可能刚好赶上前一秒的一次保存）。
    pub fn seed_report(&self, report: ScanReport) {
        if let Ok(mut slot) = self.inner.report.lock() {
            *slot = report;
        }
        if let Ok(mut pending) = self.inner.pending.lock() {
            *pending = None;
        }
    }

    /// 取消正在跑的扫描：置取消位 + 标 `cancelled`，**不**动 `running`。
    ///
    /// `running` 由扫描线程自己翻——它是「线程还活着」的唯一真相。取消若也翻它，`start()` 的
    /// 「已在扫」判据就失效，用户能在旧线程还卡着的时候起第二个（复审 P2-5 的第三个问题）。
    ///
    /// 排队位一并清掉：取消的意思是「别再扫了」，留着队列就会在这一轮退出之后**自己再起一轮**
    /// （r2 复审 P2-2 同一种病）。
    ///
    /// 规则是「**最后的用户动作胜**」（M373 r1 P2-2）：清掉的只是**取消之前**排进来的请求；取消
    /// **之后**再请的那一次（例如取消到线程真退出这一段里用户点了保存）会被记进排队位，并在这一轮
    /// 收尾时**照跑**——见 [`ScanSlot::run`] 的交接。
    pub fn cancel(&self) {
        self.inner.cancel.store(true, Ordering::SeqCst);
        if let Ok(mut report) = self.inner.report.lock() {
            if report.running {
                report.cancelled = true;
            }
        }
        if let Ok(mut pending) = self.inner.pending.lock() {
            *pending = None;
        }
    }

    /// 发起一次扫描（已在扫 ⇒ 这一次**排队**，返回 [`Outcome::Queued`]）。
    pub fn start(&self, roots: Vec<PathBuf>, manual: Vec<PathBuf>, system: bool) -> Outcome {
        self.start_with(roots, manual, system, run_staged)
    }

    /// 同 [`ScanSlot::start`]，但扫描作业可注入（测试用：一个可控的、会一直卡住的作业）。
    pub fn start_with(
        &self,
        roots: Vec<PathBuf>,
        manual: Vec<PathBuf>,
        system: bool,
        job: Job,
    ) -> Outcome {
        let request = Queued {
            roots,
            manual,
            system,
            job,
        };
        {
            let Ok(mut report) = self.inner.report.lock() else {
                return Outcome::Broken;
            };
            // 计数在「已在扫 ⇒ 排队」**之前**：它判的是「这一拍又请了一次重扫」，不是「这次真的
            // 起来了」。放在后面会让判据重新变成机器的函数——本机有 JDK 时首屏扫描还在跑，
            // `start_with` 走的是排队那一支（M370 实测：放在后面，本机 zh-CN 就红）。
            #[cfg(test)]
            self.inner.requests.fetch_add(1, Ordering::SeqCst);
            if report.running {
                // 排队位与「读 running」在同一段临界区里：中间不可能有人把 `running` 翻回
                // `false` 并把这格清掉（那样这次请求就静默丢了）。
                let Ok(mut pending) = self.inner.pending.lock() else {
                    return Outcome::Broken;
                };
                *pending = Some(request);
                return Outcome::Queued;
            }
            // 「占位」与「线程起来了」在同一把锁里完成：两个调用不可能同时通过上面那道判据。
            *report = ScanReport {
                running: true,
                ..ScanReport::default()
            };
            self.inner.cancel.store(false, Ordering::SeqCst);
        }

        let slot = self.clone();
        std::thread::spawn(move || slot.run(request));
        Outcome::Running
    }

    /// 一条扫描线程里跑完整条队伍：这一轮跑完，若有人在这期间排了队就**接着**跑下一轮
    /// （不再起线程——`running` 在两轮之间不翻面，界面看不到「没在扫」的空档，外面也挤不进来）。
    fn run(&self, mut request: Queued) {
        loop {
            // 候选收集（注册表 / 常见目录）放在线程里：它是首屏那一拍最贵的一段，
            // 放在 UI 线程上会拖慢开窗（复审 P2-5 的附带项）。
            let candidates = staged::candidates(&request.roots, &request.manual, request.system);
            if let Ok(mut report) = self.inner.report.lock() {
                report.total = candidates.len();
            }
            let cancelled = || self.inner.cancel.load(Ordering::SeqCst);
            let mut progress = |done: usize, _total: usize| {
                if let Ok(mut report) = self.inner.report.lock() {
                    report.done = done;
                }
            };
            let runtimes = (request.job)(&candidates, &cancelled, &mut progress);
            let next = {
                let Ok(mut report) = self.inner.report.lock() else {
                    return;
                };
                report.running = false;
                report.done = candidates.len();
                if !report.cancelled {
                    report.finished = true;
                    report.runtimes = runtimes;
                }
                // 交接：取走排队位与「`running` 翻回 false」在同一个临界区里完成。
                //
                // **最后的用户动作胜**（M373 r1 P2-2）：`cancel()` 已经把「取消之前排进来的」清掉
                // 了，所以此刻能在这里取到的排队位必然是**取消之后**才排进来的（典型链路：改安装根
                // → 点取消 → 线程真正退出前又点保存，`can_save()` 在扫描 / 取消期间都不灰）。这种
                // 请求要照跑：回执已经承诺「会按新目录重扫」，丢掉它就变成同一句谎话。
                //
                // 复位必须与「接着跑」同一段临界区：`cancelled` 与取消位一起清，否则第二轮会以
                // 「已取消」的姿态跑完却把结论丢在 `if !report.cancelled` 里。
                let next = self.inner.pending.lock().ok().and_then(|mut p| p.take());
                if next.is_some() {
                    *report = ScanReport {
                        running: true,
                        ..ScanReport::default()
                    };
                    self.inner.cancel.store(false, Ordering::SeqCst);
                }
                next
            };
            let Some(next) = next else {
                return;
            };
            request = next;
        }
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
