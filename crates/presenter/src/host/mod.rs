//! 真实后端：把 core（game/launch/install/auth/java/diagnose/setting）装配成 GUI 的 host。
//!
//! 边界：本模块是**整个 presenter crate 里唯一**允许 `use unlucky_core::…` 的地方（连同
//! `settings_bridge`）；出口只有 [`crate::dto`] 的快照/事件。线程模型：
//!
//! ```text
//! dispatch(AppCommand) ──► 立即动作（读盘/写设置）或 spawn 后台线程
//! 后台线程 ──HostTask──► rx ──tick()──► CoreEvent ──► state.apply()
//! tick() 另外每 ~600ms 尾随一次运行中会话的 JSONL（实时日志滚动）
//! ```
//!
//! AI 对话暂时仍由 [`MockBackend`] 提供脚本回复（agent 的 GUI host 是后续 mission）；
//! 其余页面全部走真实 core。

mod accounts;
mod addon;
// M223：导入已有版本（扫描 / 预检 / 确认三条后台线程）
mod adopt;
mod dispatch;
mod engine;
pub mod env;
mod game;
mod handle;
mod identity;
mod install;
mod install_journal;
mod install_processors;
mod java;
mod job;
mod launch;
mod launch_args;
mod loader_versions;
mod logs;
mod meter;
mod mods;
mod opener;
mod saves;
mod settings;
mod update;
mod update_state;
mod versions;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;

use crate::dto::{
    AccountView, AppCommand, AppEvent, Backend, CoreEvent, DiagnosisReport, InstallProgressView,
    InstanceSummary, JavaChainStep, LaunchFailureCause, NoticeView, SkinImage,
};
use crate::mock::MockBackend;
use crate::toast::{Toast, ToastKind};

// M304：`running_pid` 搬进了 `launch::accept`，本文件不再用它；但 `launch_state_tests`（`use super::*`）
// 仍然拿 `InstanceRuntime` 写断言，所以只在单测里引一次（与 `game::refresh` 里 `Backend` 同一个写法）。
#[cfg(test)]
use crate::dto::InstanceRuntime;

use accounts::LoginJob;
pub use env::HostEnv;
use job::{HostTask, JobReceiver, JobSender};

/// 每拍最多处理的后台消息数（防止一次 tick 吃掉整帧预算）。
const MAX_TASKS_PER_TICK: usize = 64;

/// 实时日志尾随间隔（拍数，33ms/拍 → 约 600ms 一次）。
const TAIL_POLL_TICKS: u64 = 18;

/// 一次在跑的启动（并发启动时按实例 id 区分，停止/尾随都不会串到别的实例）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct LiveLaunch {
    instance: String,
    session: String,
    pid: u32,
}

/// 真实 host。
pub struct CoreHost {
    env: HostEnv,
    events: VecDeque<AppEvent>,
    tx: JobSender,
    rx: JobReceiver,
    ticks: u64,
    /// 当前启动账户 id（内存态；跨会话持久化要 core schema 增键，见 handoff）
    active_account: Option<String>,
    accounts: Vec<AccountView>,
    login: Option<LoginJob>,
    /// 第三方轨（authlib）登录任务句柄（与微软两条流共用取消语义，但**不复用槽位**：
    /// 两条轨同时点「登录」时各自的结果不能互相踩）
    authlib: Option<LoginJob>,
    /// 已抓到的皮肤（账户 id → 原图；`None` = 试过但没取到，不再重试）
    skins: BTreeMap<String, Option<SkinImage>>,
    /// 已经发起过皮肤抓取的账户 id（防每次刷新都重新抓一遍）
    skin_requested: BTreeSet<String>,
    java_chain: Vec<JavaChainStep>,
    java_busy: bool,
    java_message: String,
    install: Option<InstallProgressView>,
    notices: Vec<NoticeView>,
    /// 在跑的启动（实例 → 会话 → pid；支持多个实例同时启动，停止按 id 找正确的 pid）
    launches: Vec<LiveLaunch>,
    /// 已受理但后台线程还没回报 `LaunchStarted` 的实例（防「启动中重复点启动」起第二个进程）
    pending: BTreeSet<String>,
    /// 上次启动失败的实例与原因（刷新快照时保住磁贴的「失败/重试」外观）
    /// 上次启动失败（阶段 + 原文；磁贴的「失败/重试」外观靠它重建）
    failed: BTreeMap<String, LaunchFailureCause>,
    /// 实时日志尾随器
    tail: logs::Tail,
    /// AI 对话的临时后端（agent GUI host 落地前的占位）
    chat: MockBackend,
    /// 当前安装任务的取消标记（`execute_with_cancel` 与段边界检查共同消费）
    install_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// 当前安装任务的代次。
    ///
    /// 开工（`InstallVersion` / `CreateInstance`）与取消都 +1：还在飞的后台线程把事件带回来时
    /// 一眼能看出「这是旧任务的」，于是既不会被取消掉的安装重新点亮面板，也不会串到下一轮安装
    /// （两轮安装同时存在时，旧任务的事件曾经把新任务的进度条顶掉）。
    install_epoch: u64,
    /// 「导入已有版本」的代次（M223）：每条 scan/preview/confirm 命令 +1，关面板也 +1。
    ///
    /// 只认最新代次的回程事件——用户连点或点了关闭之后，旧线程带回来的结果不许再把面板点亮。
    adopt_epoch: u64,
    /// 自更新（M159）：检查结论 / 应用状态 / 更新域偏好。
    update: update_state::Runtime,
    /// 实例快照的代次 + 观测账（M302）：每次刷新 +1，**旧代次的回程结果一律丢掉**。
    ///
    /// 扫盘搬进后台线程之后，「先发出的扫描可能后回来」：它的结果已经过期（可能带着刚被删掉的
    /// 实例、漏掉刚装好的），采纳它等于把新快照盖回旧的样子。判据在 `handle::apply_instances`，
    /// 账目（实例数 / 扫盘耗时 / 丢了几份）在 [`game::Scan`] 里，单测直接读它。
    instances_scan: game::Scan,
    /// 最近一次**发给界面**的实例快照（M304：启动/停止的实例解析读它，不再扫盘）。
    ///
    /// 由 [`CoreHost::send`] 发 `InstancesChanged` 时顺手留一份——于是「host 认为的列表」与
    /// 「界面上的列表」永远是同一份，不会再出现「扫盘那一刻的列表」这种第三份真相。
    ///
    /// 它可能**落后于盘面**（用户在两拍之间删了实例 / 改了版本）：落点的正确性由
    /// `game::Scan::revalidate` 在受理那一刻复核单条实例，理由与判据见 `launch::target` 模块文档。
    published: Vec<InstanceSummary>,
}

impl CoreHost {
    /// 按便携数据根装配；解析不出数据根返回 `None`（调用方退回 mock）。
    pub fn open() -> Option<Self> {
        HostEnv::resolve().map(Self::new)
    }

    /// 用给定环境装配（截图/单测通道）。
    pub fn new(env: HostEnv) -> Self {
        Self::build(env, true)
    }

    /// 单测用：不跑 `refresh_all`（不起版本清单网络线程），其余装配一致。
    #[cfg(test)]
    pub(crate) fn headless(env: HostEnv) -> Self {
        Self::build(env, false)
    }

    fn build(env: HostEnv, refresh: bool) -> Self {
        let (tx, rx) = job::channel();
        game::ensure_root(&env);
        // M188：把「会话日志换了家」这件事记一笔（旧目录还在不在、有几个文件、搬了什么）。
        // 策略是**不搬**，所以这份记录是「我们没动你的数据」的唯一凭据。
        logs::note_location(&env);
        // M159：更新状态要在构造时就有一份（提示条/设置页都读它），静默检查在 `refresh_all`
        // 里按开关派发——`headless` 走 `refresh=false`，于是单测不会发任何网络请求。
        let update = update_state::Runtime::for_env(&env);
        let mut host = Self {
            env,
            events: VecDeque::new(),
            tx,
            rx,
            ticks: 0,
            active_account: None,
            accounts: Vec::new(),
            login: None,
            authlib: None,
            skins: BTreeMap::new(),
            skin_requested: BTreeSet::new(),
            java_chain: Vec::new(),
            java_busy: false,
            java_message: String::new(),
            install: None,
            notices: Vec::new(),
            launches: Vec::new(),
            pending: BTreeSet::new(),
            failed: BTreeMap::new(),
            tail: logs::Tail::default(),
            chat: MockBackend::default(),
            install_cancel: None,
            install_epoch: 0,
            adopt_epoch: 0,
            update,
            instances_scan: game::Scan::default(),
            published: Vec::new(),
        };
        if refresh {
            host.refresh_all();
        }
        // M109：CF 源可用与否只在装配时打一行（**不含密钥内容**，见 `addon` 模块文档）。
        addon::log_curseforge_state(host.env());
        host
    }

    /// 数据根（截图/诊断展示用）。
    pub fn env(&self) -> &HostEnv {
        &self.env
    }

    fn send(&mut self, event: CoreEvent) {
        // M304：**已发布快照**在这里留一份（启动/停止的实例解析读它，不再扫盘）。
        // 选这里而不是扫描回程：这里发出去的就是界面上的那一份，两者不可能分叉——所以运行时态事件
        // 也必须同时落进快照（app 侧 `apply` 会把它**就地**写进自己的列表，只认扫描回程就会分叉）。
        match &event {
            CoreEvent::InstancesChanged(instances) => self.published.clone_from(instances),
            CoreEvent::InstanceRuntimeChanged { id, runtime } => {
                if let Some(instance) = self.published.iter_mut().find(|entry| &entry.id == id) {
                    instance.runtime.clone_from(runtime);
                }
            }
            _ => {}
        }
        self.events.push_back(AppEvent::Core(event));
    }

    /// 开一轮安装：代次 +1 并把新代次交出去（后台线程随每条进度带回来）。
    ///
    /// 返回**新**代次而不是旧代次：调用方拿它去 `spawn_install`，于是「谁是新任务」只有一处答案。
    fn next_install_epoch(&mut self) -> u64 {
        self.install_epoch = self.install_epoch.wrapping_add(1);
        self.install_epoch
    }

    fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.events.push_back(AppEvent::Toast(Toast {
            id: 0,
            kind,
            text: text.into(),
            remaining_ms: 0,
        }));
    }

    /// 全量刷新：实例 / 会话 / 账户 / 设置 / Java / 更新，并触发一次版本清单拉取。
    pub fn refresh_all(&mut self) {
        self.refresh_instances();
        self.send(CoreEvent::LogSessionsChanged(logs::sessions(&self.env)));
        self.refresh_accounts();
        self.send(CoreEvent::SettingsChanged(settings::groups(&self.env)));
        self.refresh_java();
        versions::spawn_list(self.env.clone(), self.tx.clone());
        // M159：更新快照先发一份（设置页首帧就有「当前版本 / 上次结果」），
        // 再按开关决定要不要静默查一次。关掉开关 ⇒ 这里一行网络都不发。
        self.publish_update();
        if self.update.should_check_on_start() {
            self.start_update_check(false);
        }
    }

    /// 重扫实例目录（M302：**扫盘在后台线程**，回程由 `handle::apply_instances` 落地）。
    ///
    /// 两半分家：`game::instances(&env)` 只读盘（纯函数，可搬线程）；「把运行时态盖回去」要读
    /// host 自己的 `launches/pending/failed`，**必须留在 host 线程**，并且在**结果回来那一刻**盖
    /// ——判断理由见 `handle::apply_instances`。
    ///
    /// 代次只增不减：连点刷新 / 收尾触发的刷新 / 扫描途中删实例这三种交错下，先发出的扫描可能
    /// 后回来，它的结果已经过期，一律丢（[`game::Scan`]）。**不做线程取消**：扫描是纯读，
    /// 掐断它省不下什么，却可能让一份快照永远不回来。
    fn refresh_instances(&mut self) {
        let epoch = self.instances_scan.begin();
        // 还没有任何快照落地（冷启动 / 超时后点「重新扫描」）⇒ 面板回到「正在扫描实例…」。
        // **已就绪时绝不发这条**：那会让列表在每次刷新时退回占位——把卡顿换成闪屏，正是本 mission 禁止的。
        if self.instances_scan.adopted().is_none() {
            self.send(CoreEvent::InstancePhaseChanged(
                crate::dto::InstancePhase::Scanning,
            ));
        }
        game::spawn_scan(self.env.clone(), self.tx.clone(), epoch);
    }

    fn refresh_java(&mut self) {
        let page = java::page(
            &self.env,
            self.java_chain.clone(),
            self.java_busy,
            &self.java_message,
        );
        self.send(CoreEvent::JavaChanged(page));
    }

    fn push_notice(&mut self, notice: NoticeView) {
        self.notices.retain(|existing| existing.id != notice.id);
        self.notices.push(notice);
        self.send(CoreEvent::NoticesChanged(self.notices.clone()));
    }

    /// 打开一个会话（日志行 + 诊断）。
    fn open_session(&mut self, id: &str) {
        let (lines, diagnosis) = logs::open(&self.env, id);
        self.send(CoreEvent::LogLinesChanged(lines));
        self.send(CoreEvent::DiagnosisReady(diagnosis.unwrap_or(
            DiagnosisReport {
                session: id.to_string(),
                diagnosis: crate::dto::Diagnosis {
                    rule: String::new(),
                    title: "该会话没有命中已知规则".to_string(),
                    confidence: 0,
                    hint: "可点「让 AI 详解」把日志交给模型进一步分析".to_string(),
                    fix: None,
                },
            },
        )));
    }
}

impl Backend for CoreHost {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        self.events.drain(..).collect()
    }

    fn dispatch(&mut self, command: AppCommand) {
        self.dispatch_command(command);
    }

    fn tick(&mut self, elapsed: Duration) {
        self.chat.tick(elapsed);
        for event in self.chat.drain_events() {
            self.events.push_back(event);
        }
        for _ in 0..MAX_TASKS_PER_TICK {
            let Ok(task) = self.rx.try_recv() else {
                break;
            };
            self.handle(task);
        }
        self.ticks += 1;
        // M302 的有界兜底：首个快照太久没到就报超时（面板翻到「重新扫描」，线程不掐）
        self.check_scan_stall();
        if self.ticks.is_multiple_of(TAIL_POLL_TICKS) {
            self.poll_tail();
        }
    }
}

#[cfg(test)]
#[path = "cancel_tests.rs"]
mod cancel_tests;
#[cfg(test)]
#[path = "launch_state_tests.rs"]
mod launch_state_tests;
