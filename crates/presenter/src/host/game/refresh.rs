//! 实例快照的**扫描账**（M302）：扫盘搬进后台线程之后，「哪一份结果说了算」由这里的代次管。
//!
//! ## 为什么要有这一层
//!
//! [`super::instances`] 是纯 `env` 函数（只读盘），但它慢：官方根 200 个版本约 236 ms，
//! 叠加 10 份外部档案到 1.35 s（仓库内实测，见 M297 报告）。它原来跑在 `dispatch(RefreshAll)`
//! 的那条 `match` 里——即 UI 线程上——于是列表在那段时间里不动。
//!
//! 搬进线程之后**顺序**就成了新问题：先发出的扫描可能后回来。那时它的结果已经过期（可能带着
//! 刚被删掉的实例、漏掉刚装好的），采纳它等于把新快照盖回旧的样子——用户看到「删掉的实例又
//! 回来了」「刚装好的版本不见了」。所以每次刷新领一个**只增不减的代次**，随任务进线程、随结果
//! 回来；[`super::super::handle::CoreHost::apply_instances`] 只采纳当代次的那一份。
//!
//! ## 为什么不做「取消」
//!
//! 没有可取消的东西：扫描是**纯读**（不建目录、不写 `profiles.json`、不碰 `versions/` 一个字节），
//! 提前掐断它省下的只是它自己那点 CPU，而掐断的代价是一个可能永远不回来的快照。
//! 于是一条路口：**不取消线程，只丢弃过期结果**（丢掉的是那一份，不是一个状态）。

use std::time::{Duration, Instant};

use crate::dto::{CoreEvent, InstancePhase};

use super::super::job::{spawn, HostTask, JobSender};
use super::super::HostEnv;

/// M304：快照条目的「现在」——这一条**还**在不在盘上（判据见 `live` 的模块文档）。
///
/// 它能在 `game` 外面被调到，是因为挂成了 [`Scan`] 的关联函数：`host` 其它地方只够得着
/// `game` 转出的那几个名字（`Scan` / `instances` / `game_for` …），私有子模块够不着。
mod live;

/// 首个实例快照的等待上限（M302 的**有界兜底**）。
///
/// ## 取值依据（不是随手挑的）
///
/// 仓库内最坏的一次实测扫盘是 **1.35 s**（官方根 200 个版本 + 10 份外部档案，M297/M274 r1）；
/// 15 s ≈ 它的 11 倍，足够盖住慢盘 / 网络盘上的一次 `read_dir` 抖动，又不至于让用户在
/// 「正在扫描实例…」上等到放弃。真正该防的不是「慢一点」，而是**线程 panic / 永远不回来**——
/// 那种情况下没有兜底，占位永远不会消失。
///
/// ## 超时之后发生什么
///
/// **不取消扫描线程**（理由见模块文档）：面板先翻到「扫描实例比平时久」+ 可点的「重新扫描」，
/// 而那一轮的线程仍在跑、迟到的结果照样落地（代次没变，`admit` 照放）⇒ 占位自己消失。
/// 只有「永远不回来」那种才需要用户点重试。
pub const SCAN_STALL: Duration = Duration::from_secs(15);

/// 一次刷新领到的代次（`0` 是「还没刷新过」的初值，第一次刷新从 `1` 开始）。
pub type Epoch = u64;

/// 代次 + 观测账：「最近一次被采纳的扫描多大、花了多久、之前丢了几份旧代次」。
///
/// 这一份账是 M302 的可观测口径（[`Adopted`] 给断言用，`[instances]` 日志行给人看）：
/// 没有它，回归时只能说「感觉快了」，说不清「扫了几条、扫了多久、有没有丢包」。
#[derive(Debug, Default)]
pub struct Scan {
    /// 当前代次（最近一次刷新的编号）
    epoch: Epoch,
    /// 最近一次被采纳的扫描
    adopted: Option<Adopted>,
    /// 被丢掉的**旧代次**结果条数（> 0 就说明「先发后到」真的发生过）
    ///
    /// **已知噪音（复审 N3）**：`CoreHost::build` 的 `refresh_all` 与 `on_boot` 的
    /// `AppCommand::RefreshAll` 各领一代 ⇒ **每次冷启动必然丢 1 份**。所以这个计数在人读日志上
    /// 必须配合「此前有没有快照落地过」一起看（`handle::apply_instances` 的丢弃日志已经把两种
    /// 成因分开写），单看「> 0」会误判成交错。
    dropped: u64,
    /// 本轮扫描的起跑时刻（超时兜底的唯一时钟来源；`None` = 结果已经落地 / 还没起跑）
    started: Option<Instant>,
    /// 本轮是否已经报过超时（`tick` 每拍都会问，只报一次，否则面板/日志会被刷成噪音）
    stalled: bool,
    /// 本轮的超时上限（生产恒为 [`SCAN_STALL`]；单测可以调小，不必等真实的 15 s）
    budget: Duration,
}

/// 一次被采纳的扫描：代次、实例数、纯扫盘耗时（毫秒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Adopted {
    /// 这次扫描的代次
    pub epoch: Epoch,
    /// 快照里的实例条数（磁贴数）
    pub count: usize,
    /// 后台线程里 `game::instances` 的耗时（不含线程创建与排队——那是主机的事，不是扫盘的成本）
    pub elapsed_ms: u64,
}

impl Scan {
    /// 开一轮新刷新：代次 +1 并返回它（调用方把它交给后台线程，随结果回来）。
    ///
    /// 只增不减（`wrapping_add` 与 `install_epoch` 同款）：**代次相等才是当代次**，
    /// 于是「上一轮还在飞的那份」无论什么时候回来都进不了门。
    pub fn begin(&mut self) -> Epoch {
        self.begin_with(SCAN_STALL)
    }

    /// 同 [`Scan::begin`]，但显式给上限（单测用；生产只有上面那一个入口）。
    pub fn begin_with(&mut self, budget: Duration) -> Epoch {
        self.epoch = self.epoch.wrapping_add(1);
        self.started = Some(Instant::now());
        self.stalled = false;
        self.budget = budget;
        self.epoch
    }

    /// 单测专用：把**当前这一轮**的上限调小（生产路径的上限只由 [`SCAN_STALL`] 定）。
    ///
    /// 存在的理由：超时路径要能在单测里真跑一遍（事件发出去 / 只发一次 / 迟到结果清掉它），
    /// 而等 15 s 会让整个门禁慢十几秒。**调小不等于换阈值**：阈值本身仍只有 [`SCAN_STALL`]。
    #[cfg(test)]
    pub fn shorten_budget(&mut self, budget: Duration) {
        self.budget = budget;
    }

    /// 这一轮**还要不要**报超时（有界兜底，判据是 [`SCAN_STALL`]）。
    ///
    /// `true` 只在「有扫描在飞 + 已经超过上限 + 这一轮还没报过」时返回一次；报过之后
    /// 每拍再问都是 `false`（`tick` 每帧都会问，重复报会把面板刷成噪音）。
    /// 结果落地（[`Scan::adopt`]）会停表，之后永远 `false`。
    pub fn stall(&mut self) -> bool {
        let Some(started) = self.started else {
            return false;
        };
        if self.stalled || started.elapsed() < self.budget {
            return false;
        }
        self.stalled = true;
        true
    }

    /// 这份结果还是不是当代次的？不是的话记一笔丢包（可观测）并返回 `false`。
    ///
    /// 判据是**相等**而不是「更大」：代次是唯一的命名，只有最新一轮发出去的那份作数。
    pub fn admit(&mut self, epoch: Epoch) -> bool {
        if epoch == self.epoch {
            return true;
        }
        self.dropped = self.dropped.wrapping_add(1);
        false
    }

    /// 记下这一份（只在 [`Scan::admit`] 放行之后调）。
    pub fn adopt(&mut self, epoch: Epoch, count: usize, elapsed_ms: u64) {
        // 落地即停表：之后的 `tick` 不该再报超时
        self.started = None;
        self.adopted = Some(Adopted {
            epoch,
            count,
            elapsed_ms,
        });
    }

    /// 当前代次。
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    /// 最近一次被采纳的扫描（还没落地过 = `None`）。
    pub fn adopted(&self) -> Option<Adopted> {
        self.adopted
    }

    /// 已经丢掉的旧代次结果条数。
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

/// 单测用的**扫描探针**（M302 r2 P2-1）：记录扫描体跑在哪条线程上、跑完了几趟、有没有被闸门拦住。
///
/// 为什么要它：上一轮那条「刷新返回时快照还没到」证明不了**扫盘不在调用方线程**——把
/// `spawn("instances", job)` 换成直接 `job()`（inline），事件照样要等 `tick` 才进 host 队列，
/// 那条断言仍然绿（复审 r2 实测整棵树全绿）。真正的判据只能是「扫描体在另一条线程上跑、
/// 而且调用方返回时它还没跑完」，于是这里给它一个可注入的闸门。
///
/// **只在 `cfg(test)` 里存在**：产品路径没有这个结构，也读不到它。
#[cfg(test)]
#[derive(Debug)]
pub(super) struct Probe {
    /// 只有这个数据根起的扫描才会被闸门拦住（别的用例并发跑扫描时不受影响）
    root: std::path::PathBuf,
    /// 扫描体进入时记下的线程 id（每趟一条）
    threads: std::sync::Mutex<Vec<std::thread::ThreadId>>,
    /// 已经跑完几趟
    finished: std::sync::atomic::AtomicUsize,
    /// 闸门：扫描体在这里等「放行」（带超时，免得 inline 变异把用例挂死）
    gate: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    /// 闸门有没有被放行过
    released: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
static PROBE: std::sync::Mutex<Option<std::sync::Arc<Probe>>> = std::sync::Mutex::new(None);

#[cfg(test)]
impl Probe {
    /// 扫描体进入：记线程、在闸门下等放行（最多 `GATE`，超时自行离开而不是把用例挂死）。
    fn enter(&self) {
        self.threads
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(std::thread::current().id());
        let receiver = self.gate.lock().unwrap_or_else(|error| error.into_inner());
        if receiver.recv_timeout(GATE).is_ok() {
            self.released
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn leave(&self) {
        self.finished
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// 跑过扫描的线程 id。
    pub(super) fn threads(&self) -> Vec<std::thread::ThreadId> {
        self.threads
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// 跑完了几趟。
    pub(super) fn finished(&self) -> usize {
        self.finished.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 闸门被放行过没有。
    pub(super) fn released(&self) -> bool {
        self.released.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// 闸门的等待上限：够大到「inline 变异一定超时」（那时用例本来也要靠它才不挂死），
/// 又够小到不会把门禁拖慢（正常路径毫秒级就放行）。
#[cfg(test)]
pub(super) const GATE: Duration = Duration::from_secs(2);

/// 装一个探针（返回放行端）；同一时刻只允许一个（用例自己负责先卸再装）。
#[cfg(test)]
pub(super) fn install_probe(
    root: std::path::PathBuf,
) -> (std::sync::Arc<Probe>, std::sync::mpsc::Sender<()>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let probe = std::sync::Arc::new(Probe {
        root,
        threads: std::sync::Mutex::new(Vec::new()),
        finished: std::sync::atomic::AtomicUsize::new(0),
        gate: std::sync::Mutex::new(rx),
        released: std::sync::atomic::AtomicBool::new(false),
    });
    *PROBE.lock().unwrap_or_else(|error| error.into_inner()) = Some(std::sync::Arc::clone(&probe));
    (probe, tx)
}

/// 卸掉探针（用例结束前必调，否则会影响同进程里的其它用例）。
#[cfg(test)]
pub(super) fn clear_probe() {
    *PROBE.lock().unwrap_or_else(|error| error.into_inner()) = None;
}

/// 这一趟扫描要不要过闸门（只有「探针在场 + 数据根相同」才拦）。
#[cfg(test)]
fn probe_for(env: &HostEnv) -> Option<std::sync::Arc<Probe>> {
    let guard = PROBE.lock().unwrap_or_else(|error| error.into_inner());
    guard
        .as_ref()
        .filter(|probe| probe.root == env.game.root())
        .cloned()
}

/// 后台扫一遍实例目录，把结果连代次一起送回 host（[`HostTask::Instances`]）。
///
/// 与 `versions::spawn_list` / `loader_versions::spawn_list` 同一条形状：线程里只做 core 调用，
/// 出来的已经是 [`crate::dto::InstanceSummary`]（跨线程传的东西在 `HostTask` 一处看全）。
pub fn spawn_scan(env: HostEnv, tx: JobSender, epoch: Epoch) {
    spawn("instances", move || scan_body(env, tx, epoch));
}

/// 扫描体的本体（**从 `spawn_scan` 里提出来**，好让单测证明它跑在另一条线程上）。
fn scan_body(env: HostEnv, tx: JobSender, epoch: Epoch) {
    #[cfg(test)]
    let probe = probe_for(&env);
    #[cfg(test)]
    if let Some(probe) = &probe {
        probe.enter();
    }
    let started = Instant::now();
    let instances = super::instances(&env);
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    // 接收端没了（host 已销毁）也不必报错：这条线程的活儿本来就作废了
    let _ = tx.send(HostTask::Instances {
        epoch,
        instances,
        elapsed_ms,
    });
    #[cfg(test)]
    if let Some(probe) = &probe {
        probe.leave();
    }
}

impl super::super::CoreHost {
    /// 每拍看一眼「首个快照」有没有超时（有界兜底）。
    ///
    /// 为什么要兜底：扫描线程若 panic 或卡在慢盘上，`ready` 就永远是 `false` ⇒ 用户永远看着
    /// 「正在扫描实例…」。这里只**报**超时（面板翻到可点的「重新扫描」），**不**掐线程——
    /// 判据与取值理由在 [`Scan::stall`] / [`SCAN_STALL`]。
    pub(crate) fn check_scan_stall(&mut self) {
        // **已就绪之后不再报超时**（复审 r2 P2-2）：列表里已经有真数据，一轮慢刷新不该把界面
        // 打回「扫描实例比平时久 + 重新扫描」——那会把有数据的一屏换成占位。判据与
        // `refresh_instances` 发 `Scanning` 的那道守卫**同一个**（`adopted().is_none()`）。
        if self.instances_scan.adopted().is_some() {
            return;
        }
        if !self.instances_scan.stall() {
            return;
        }
        eprintln!(
            "[instances] 首个快照 {SCAN_STALL:?} 内没回来：报超时（扫描线程仍在跑，迟到的结果照样落地）"
        );
        self.send(CoreEvent::InstancePhaseChanged(InstancePhase::Stalled));
    }
}

/// 单测要 `tick` 抽通道，它的定义在 `crate::dto::Backend` 这条 trait 上。
#[cfg(test)]
use crate::dto::Backend;

/// 单测等待「这一代快照落地」的上限。
///
/// 用例的数据根都是临时目录里的一到三条档案（扫盘是毫秒级），5 s 只是给 CI 的抖动留余量；
/// 超时意味着**真的坏了**（线程没起、通道没人抽、代次判据写反），所以见 [`CoreHost::pump_instances`]。
#[cfg(test)]
const PUMP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[cfg(test)]
impl super::super::CoreHost {
    /// 单测专用：抽通道直到**当代次**的实例快照落地。
    ///
    /// 为什么必须叫：`refresh_instances()` 现在是「派一条后台线程就返回」，用例若调完就读事件，
    /// 拿到的一定是空队列——后续 `.expect("实例存在")` 会把「结果还没回来」误报成
    /// 「这个实例不存在」，那是最误导的一种红。所以这里**超时就 panic**，并印出诊断。
    ///
    /// 不抽事件队列（`self.events` 原样留给调用方读）：判据读的是账本
    /// （[`Scan::adopted`] 在 `handle::apply_instances` 里**先记账再发事件**），
    /// 于是「账本已经是当代次」⇒ 事件已经在队列里。
    ///
    /// 产品路径**不调它**（`#[cfg(test)]`，进不了产物）；生产由 `tick` 抽通道。
    pub(crate) fn pump_instances(&mut self) {
        let epoch = self.instances_scan.epoch();
        let started = Instant::now();
        while started.elapsed() < PUMP_TIMEOUT {
            self.tick(std::time::Duration::from_millis(1));
            if self
                .instances_scan
                .adopted()
                .is_some_and(|adopted| adopted.epoch == epoch)
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!(
            "等实例快照超时（{:?}）：当代次 {epoch}，账本停在 {:?}，已丢旧代次 {} 份，\
             事件队列 {} 条，最后一条 {:?}——刷新结果没回来（线程没起 / 通道没人抽 / 代次判据写反）",
            PUMP_TIMEOUT,
            self.instances_scan.adopted(),
            self.instances_scan.dropped(),
            self.events.len(),
            self.events.back(),
        );
    }
}

#[cfg(test)]
#[path = "refresh_stall_tests.rs"]
mod stall_tests;

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;
