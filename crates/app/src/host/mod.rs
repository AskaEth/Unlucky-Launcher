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
mod identity;
mod install;
mod java;
mod job;
mod launch;
mod logs;
mod meter;
mod settings;
mod versions;

use std::collections::VecDeque;
use std::time::Duration;

use crate::dto::{
    AccountView, AppCommand, AppEvent, Backend, CoreEvent, DiagnosisReport, InstallProgressView,
    InstanceRuntime, JavaChainStep, LogOutcome, NoticeView,
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
    /// 最近一次启动的 pid（停止按钮用）
    launch_pid: Option<u32>,
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
            launch_pid: None,
            tail: logs::Tail::default(),
            chat: MockBackend::default(),
            install_cancel: None,
        };
        host.refresh_all();
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

    fn refresh_instances(&mut self) {
        let instances = game::instances(&self.env);
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

    fn start_launch(&mut self, id: &str) {
        let Some(target) = self.launch_target(id) else {
            self.toast(ToastKind::Error, format!("找不到实例 {id}"));
            return;
        };
        let instance_id = target.id.clone();
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

    /// 一台在跑的实例的 pid（从快照或最近一次启动里取）。
    fn running_pid(&self, id: &str) -> Option<u32> {
        if let Some(pid) = self.launch_pid {
            return Some(pid);
        }
        game::instances(&self.env)
            .into_iter()
            .find(|instance| instance.id == id)
            .and_then(|instance| match instance.runtime {
                InstanceRuntime::Running { pid } => Some(pid),
                _ => None,
            })
    }

    /// 后台消息 → 事件。
    fn handle(&mut self, task: HostTask) {
        match task {
            HostTask::Versions(Ok(versions)) => self.send(CoreEvent::VersionListChanged(versions)),
            HostTask::Versions(Err(reason)) => {
                self.toast(ToastKind::Warn, format!("拉取版本清单失败：{reason}"));
            }
            HostTask::Install(progress) => {
                self.install = Some(*progress.clone());
                self.send(CoreEvent::InstallProgress(Some(*progress)));
            }
            HostTask::InstallDone { progress, result } => {
                self.install_cancel = None;
                self.install = Some(*progress.clone());
                self.send(CoreEvent::InstallProgress(Some(*progress)));
                match result {
                    Ok(_) => {
                        self.refresh_instances();
                        self.send(CoreEvent::LogSessionsChanged(logs::sessions(&self.env)));
                    }
                    Err(reason) => self.toast(ToastKind::Error, reason),
                }
            }
            HostTask::Accounts(accounts) => {
                self.accounts = accounts.clone();
                self.send(CoreEvent::AccountsChanged(accounts));
                self.refresh_instances();
            }
            HostTask::Login(view) => self.send(CoreEvent::DeviceLoginChanged(*view)),
            HostTask::Java(page) => self.send(CoreEvent::JavaChanged(*page)),
            HostTask::Logs {
                sessions,
                lines,
                diagnosis,
            } => {
                self.send(CoreEvent::LogSessionsChanged(sessions));
                self.send(CoreEvent::LogLinesChanged(lines));
                if let Some(report) = diagnosis {
                    self.send(CoreEvent::DiagnosisReady(report));
                }
            }
            HostTask::Sessions(sessions) => self.send(CoreEvent::LogSessionsChanged(sessions)),
            HostTask::Tail(batch) => {
                if batch.running {
                    self.send(CoreEvent::LogLinesChanged(batch.lines));
                } else {
                    self.send(CoreEvent::LogAppended(*batch));
                }
            }
            HostTask::LaunchStarted {
                id,
                session_id,
                pid,
            } => {
                self.launch_pid = Some(pid);
                self.tail.attach(
                    self.env
                        .sessions_root()
                        .join("game")
                        .join(format!("{session_id}.jsonl")),
                    &session_id,
                );
                self.send(CoreEvent::InstanceRuntimeChanged {
                    id,
                    runtime: InstanceRuntime::Running { pid },
                });
            }
            HostTask::Runtime { id, runtime } => {
                self.send(CoreEvent::InstanceRuntimeChanged { id, runtime });
            }
            HostTask::LaunchEnd(finished) => {
                self.launch_pid = None;
                let runtime = if finished.outcome == LogOutcome::Ok {
                    InstanceRuntime::Idle
                } else {
                    InstanceRuntime::Failed {
                        reason: finished.summary.clone(),
                    }
                };
                self.send(CoreEvent::InstanceRuntimeChanged {
                    id: finished.id.clone(),
                    runtime,
                });
                self.send(CoreEvent::LaunchFinished((*finished).clone()));
                self.tail.detach();
                self.send(CoreEvent::LogSessionsChanged(logs::sessions(&self.env)));
                self.refresh_instances();
            }
            HostTask::Crash(report) => {
                self.send(CoreEvent::DiagnosisReady(*report));
            }
            HostTask::Notice(notice) => self.push_notice(*notice),
            HostTask::Toast(toast) => self.events.push_back(AppEvent::Toast(*toast)),
        }
    }

    /// 尾随运行中会话（每 `TAIL_POLL_TICKS` 拍一次）。
    fn poll_tail(&mut self) {
        let Some((session, lines)) = self.tail.poll() else {
            return;
        };
        if lines.is_empty() {
            return;
        }
        let running = self.launch_pid.is_some();
        self.send(CoreEvent::LogAppended(crate::dto::LiveLogBatch {
            session,
            lines,
            running,
        }));
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
