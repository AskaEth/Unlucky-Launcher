//! 真实后端：把 core（game/launch/install/auth/java/diagnose/setting）装配成 GUI 的 host。
//!
//! 边界：本模块是整个 app 里**唯一**允许 `use unlucky_core::…` 的地方（连同 `settings_bridge`）；
//! 出口只有 [`crate::dto`] 的快照/事件。线程模型：
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
mod dispatch;
mod engine;
pub mod env;
mod game;
mod handle;
mod identity;
mod install;
mod java;
mod job;
mod launch;
mod logs;
mod meter;
mod settings;
mod versions;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;

use crate::dto::{
    AccountView, AppCommand, AppEvent, Backend, CoreEvent, DiagnosisReport, InstallProgressView,
    InstanceRuntime, JavaChainStep, NoticeView,
};
use crate::mock::MockBackend;
use crate::toast::{Toast, ToastKind};

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
    failed: BTreeMap<String, String>,
    /// 实时日志尾随器
    tail: logs::Tail,
    /// AI 对话的临时后端（agent GUI host 落地前的占位）
    chat: MockBackend,
    /// 当前安装任务的取消标记（阶段边界检查）
    install_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
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
        let mut host = Self {
            env,
            events: VecDeque::new(),
            tx,
            rx,
            ticks: 0,
            active_account: None,
            accounts: Vec::new(),
            login: None,
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
        };
        if refresh {
            host.refresh_all();
        }
        host
    }

    /// 数据根（截图/诊断展示用）。
    pub fn env(&self) -> &HostEnv {
        &self.env
    }

    fn send(&mut self, event: CoreEvent) {
        self.events.push_back(AppEvent::Core(event));
    }

    fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.events.push_back(AppEvent::Toast(Toast {
            id: 0,
            kind,
            text: text.into(),
            remaining_ms: 0,
        }));
    }

    /// 全量刷新：实例 / 会话 / 账户 / 设置 / Java，并触发一次版本清单拉取。
    pub fn refresh_all(&mut self) {
        self.refresh_instances();
        self.send(CoreEvent::LogSessionsChanged(logs::sessions(&self.env)));
        self.refresh_accounts();
        self.send(CoreEvent::SettingsChanged(settings::groups(&self.env)));
        self.refresh_java();
        versions::spawn_list(self.env.clone(), self.tx.clone());
    }

    /// 重建实例快照，并把**真实运行时**盖回去。
    ///
    /// `game::instances` 只读盘（每条都是 `Idle`），所以在跑的实例必须在重建后按
    /// host 自己的启动表覆写 `Running{pid}`/`Busy`，否则任意一次刷新（装版本成功、
    /// 加删账户、另一实例收尾）都会把磁贴打回 Idle：停止键与「运行中」条消失，
    /// 进程还在跑却再也停不下来。
    fn refresh_instances(&mut self) {
        let mut instances = game::instances(&self.env);
        for instance in &mut instances {
            if let Some(live) = self
                .launches
                .iter()
                .find(|live| live.instance == instance.id)
            {
                instance.runtime = InstanceRuntime::Running { pid: live.pid };
            } else if self.pending.contains(&instance.id) {
                instance.runtime = InstanceRuntime::Busy {
                    stage: crate::dto::LaunchStage::Launch,
                    progress: crate::dto::Progress { done: 0, total: 1 },
                };
            } else if let Some(reason) = self.failed.get(&instance.id) {
                instance.runtime = InstanceRuntime::Failed {
                    reason: reason.clone(),
                };
            }
        }
        self.send(CoreEvent::InstancesChanged(instances));
    }

    fn refresh_accounts(&mut self) {
        self.accounts = accounts::accounts(&self.env, self.active_account.as_deref());
        if self.active_account.is_none() {
            // 默认选中第一个账户（没有账户时保持 None = 离线占位）
            self.active_account = self.accounts.first().map(|account| account.id.clone());
            self.accounts = accounts::accounts(&self.env, self.active_account.as_deref());
        }
        self.send(CoreEvent::AccountsChanged(self.accounts.clone()));
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

    /// 由实例名/版本 id 找启动目标。
    fn launch_target(&self, id: &str) -> Option<launch::LaunchTarget> {
        let instance = game::instances(&self.env)
            .into_iter()
            .find(|instance| instance.id == id || instance.name == id)?;
        Some(launch::LaunchTarget {
            id: instance.id,
            name: instance.name,
            version_id: instance.version_id,
            account: self.active_account.clone(),
        })
    }

    /// 某实例是否已在启动/运行（`pending` 与 `launches` 的并集）。
    ///
    /// 只查 `launches` 不够：`launch::start` 到后台线程回报 `LaunchStarted` 之间有窗口，
    /// 这期间重复点启动会起第二个进程、并把旧 pid 在启动表里顶掉（旧进程变孤儿）。
    fn in_flight(&self, key: &str) -> bool {
        self.pending.contains(key) || self.launches.iter().any(|live| live.instance == key)
    }

    /// 标记「已受理、等后台线程回报」（`launch::start` 之后的窗口）。
    ///
    /// 单测也走这个入口而不是直接改 `pending`，保证测的是真实状态流转。
    fn mark_pending(&mut self, id: &str) {
        self.pending.insert(id.to_string());
        self.failed.remove(id);
    }

    fn start_launch(&mut self, id: &str) {
        if self.in_flight(id) {
            self.toast(ToastKind::Warn, "这个实例已经在启动/运行中");
            return;
        }
        let Some(target) = self.launch_target(id) else {
            self.toast(ToastKind::Error, format!("找不到实例 {id}"));
            return;
        };
        let instance_id = target.id.clone();
        if self.in_flight(&instance_id) {
            self.toast(ToastKind::Warn, "这个实例已经在启动/运行中");
            return;
        }
        self.mark_pending(&instance_id);
        if let Some(notice) = game::mode_change_notice(&self.env, &instance_id) {
            self.push_notice(notice);
        }
        self.send(CoreEvent::InstanceRuntimeChanged {
            id: instance_id.clone(),
            runtime: InstanceRuntime::Busy {
                stage: crate::dto::LaunchStage::Launch,
                progress: crate::dto::Progress { done: 0, total: 1 },
            },
        });
        launch::start(self.env.clone(), self.tx.clone(), target);
    }

    /// 某实例在跑的 pid（按实例 id 查启动表；不再有「最近一次启动」这种单槽）。
    ///
    /// 兜底分支读实例快照的 `Running { pid }`：host 自己发的运行时事件会写回快照，
    /// 但快照是重建的（`game::instances` 每次 Idle），所以这层只是给将来留的口。
    fn running_pid(&self, id: &str) -> Option<u32> {
        if let Some(live) = self.launches.iter().find(|live| live.instance == id) {
            return Some(live.pid);
        }
        game::instances(&self.env)
            .into_iter()
            .find(|instance| instance.id == id)
            .and_then(|instance| match instance.runtime {
                InstanceRuntime::Running { pid } => Some(pid),
                _ => None,
            })
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
        if self.ticks.is_multiple_of(TAIL_POLL_TICKS) {
            self.poll_tail();
        }
    }
}

#[cfg(test)]
#[path = "launch_state_tests.rs"]
mod launch_state_tests;
