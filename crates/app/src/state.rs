//! UI 状态与 reducer：Slint 只负责渲染，所有状态变更都汇聚到 [`AppState::apply`]。
//!
//! 数据流向：mock/host 后端 → [`crate::dto::AppEvent`] 队列 → `apply` → 状态字段 → Slint 属性。
//! 偏好项在 [`crate::prefs`] 里登记了持久化键名，core::setting 的 schema 落地后按这些键读写。

use crate::dialogs::DialogQueue;
use crate::dto::{
    AccountView, AgentEvent, AppEvent, ChatMessage, ChatRole, CoreEvent, DeviceLoginView,
    DiagnosisReport, InstallProgressView, InstanceRuntime, InstanceSummary, JavaPageView, LogLine,
    LogSession, NoticeView, Progress, SettingGroupView, ToolCallView, VersionSummary,
};
use crate::prefs::UiPrefs;
use crate::router::{AiPanelMode, Route, Router};
use crate::toast::{ToastCenter, ToastKind};
use crate::wizard::WizardState;

/// AI 会话状态（右侧栏与独立页共享同一份，切换不丢）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSession {
    /// 会话消息（user / assistant / tool / system）
    pub messages: Vec<ChatMessage>,
    /// 是否有流式消息在跑
    pub streaming: bool,
    /// YOLO 开关（跳过确认，审计照记）
    pub yolo: bool,
    /// 输入框草稿（切形态不丢）
    pub draft: String,
    /// 端点标签（显示在栏顶）
    pub endpoint_label: String,
}

impl Default for AiSession {
    fn default() -> Self {
        Self::new()
    }
}

impl AiSession {
    /// 默认端点标签（P0 是本地 llama.cpp；P1 由设置页决定）。
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            streaming: false,
            yolo: false,
            draft: String::new(),
            endpoint_label: "llama.cpp · 本地".to_string(),
        }
    }

    /// 流式收敛：清掉全部流式标记。
    pub fn finish_stream(&mut self) {
        self.streaming = false;
        for message in &mut self.messages {
            message.streaming = false;
        }
    }
}

/// 磁贴网格单元：位置由 [`grid_positions`] 算好，Slint 只消费 row/col。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileCell<'a> {
    /// 实例数据
    pub instance: &'a InstanceSummary,
    /// 网格行号（0 起）
    pub row: usize,
    /// 网格列号（0 起）
    pub col: usize,
}

/// 纯函数：把 n 个磁贴摊到 `columns` 列的网格上，返回 `(row, col)`。
///
/// 特意放在 Rust 侧：Slint DSL 里做整数除法会引入隐式浮点，且这段逻辑可以单测。
pub fn grid_positions(count: usize, columns: usize) -> Vec<(usize, usize)> {
    let columns = columns.max(1);
    (0..count)
        .map(|index| (index / columns, index % columns))
        .collect()
}

/// 实时日志最多保留的行数（滚动窗口；更多行由日志页的筛选/历史会话承担）。
pub const MAX_LIVE_LINES: usize = 2_000;

/// 应用全局状态。
#[derive(Debug)]
pub struct AppState {
    /// UI 偏好（记忆项）
    pub prefs: UiPrefs,
    /// 导航栈
    pub router: Router,
    /// 实例列表快照
    pub instances: Vec<InstanceSummary>,
    /// 当前分类的版本列表
    pub versions: Vec<VersionSummary>,
    /// 日志会话列表
    pub log_sessions: Vec<LogSession>,
    /// 当前会话日志行
    pub log_lines: Vec<LogLine>,
    /// 最近一次诊断结论
    pub diagnosis: Option<DiagnosisReport>,
    /// 选中的日志会话
    pub selected_session: Option<String>,
    /// AI 会话状态
    pub ai: AiSession,
    /// 对话框队列（一次一条）
    pub dialogs: DialogQueue,
    /// Toast 中心
    pub toasts: ToastCenter,
    /// 实例页搜索词
    pub instance_query: String,
    /// 下载页「仅已装」过滤
    pub installed_only: bool,
    /// 下载页版本号筛选词
    pub download_query: String,
    /// 账户列表（三轨统一视图；凭据不在其中）
    pub accounts: Vec<AccountView>,
    /// 微软登录状态
    pub device_login: DeviceLoginView,
    /// Java 页快照（发行版 / 回退链 / 运行时）
    pub java: JavaPageView,
    /// 安装进度（None = 没有安装任务）
    pub install: Option<InstallProgressView>,
    /// 设置分组（设置页右栏）
    pub settings_groups: Vec<SettingGroupView>,
    /// 当前选中的设置分组（download / java / data / ai / mcp）
    pub settings_tab: String,
    /// 横幅通知
    pub notices: Vec<NoticeView>,
    /// 新建实例向导
    pub wizard: WizardState,
    /// 正在实时尾随的会话
    pub live_session: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// 空状态（数据由后端事件填充，P0 是 mock）。
    pub fn new() -> Self {
        Self {
            prefs: UiPrefs::default(),
            router: Router::new(),
            instances: Vec::new(),
            versions: Vec::new(),
            log_sessions: Vec::new(),
            log_lines: Vec::new(),
            diagnosis: None,
            selected_session: None,
            ai: AiSession::new(),
            dialogs: DialogQueue::new(),
            toasts: ToastCenter::new(),
            instance_query: String::new(),
            installed_only: false,
            download_query: String::new(),
            accounts: Vec::new(),
            device_login: DeviceLoginView::new(),
            java: JavaPageView::new(),
            install: None,
            settings_groups: Vec::new(),
            settings_tab: "download".to_string(),
            notices: Vec::new(),
            wizard: WizardState::new(),
            live_session: None,
        }
    }

    /// 当前页面。
    pub fn route(&self) -> &Route {
        self.router.route()
    }

    /// 跳转（一级页替换、子页面压栈；去 AI 独立页时自动展开 AI 栏偏好）。
    pub fn navigate(&mut self, route: Route) {
        if route == Route::Agent {
            self.prefs.ai_collapsed = false;
        }
        self.router.navigate(route);
    }

    /// 回退一层。
    pub fn back(&mut self) -> bool {
        self.router.back()
    }

    /// AI 栏当前形态（跟着路由走）。
    pub fn ai_mode(&self) -> AiPanelMode {
        self.router.ai_mode()
    }

    /// 应用一条事件（唯一状态变更入口）。
    pub fn apply(&mut self, event: AppEvent) {
        match event {
            AppEvent::Core(core) => self.apply_core(core),
            AppEvent::Agent(agent) => self.apply_agent(agent),
            AppEvent::DialogRequested(request) => {
                self.dialogs.push_request(request);
            }
            AppEvent::DialogClosed(id) => {
                self.dialogs.close(id);
            }
            AppEvent::Toast(toast) => {
                self.toasts.push_request(toast);
            }
        }
    }

    /// core 事件。
    pub fn apply_core(&mut self, event: CoreEvent) {
        match event {
            CoreEvent::InstancesChanged(instances) => self.instances = instances,
            CoreEvent::InstanceRuntimeChanged { id, runtime } => {
                if let Some(instance) = self.instances.iter_mut().find(|item| item.id == id) {
                    instance.runtime = runtime;
                }
            }
            CoreEvent::VersionListChanged(versions) => self.versions = versions,
            CoreEvent::LogSessionsChanged(sessions) => self.log_sessions = sessions,
            CoreEvent::LogLinesChanged(lines) => {
                self.log_lines = lines;
                self.live_session = None;
            }
            CoreEvent::LogAppended(batch) => self.append_live_lines(&batch),
            CoreEvent::LaunchFinished(finished) => {
                self.live_session = None;
                self.selected_session = Some(finished.session.clone());
                let kind = if finished.outcome == crate::dto::LogOutcome::Ok {
                    ToastKind::Success
                } else {
                    ToastKind::Error
                };
                self.toasts.push(kind, finished.summary);
            }
            CoreEvent::DiagnosisReady(report) => self.diagnosis = Some(report),
            CoreEvent::AccountsChanged(accounts) => self.accounts = accounts,
            CoreEvent::DeviceLoginChanged(view) => {
                if view.phase == crate::dto::DeviceLoginPhase::Succeeded {
                    self.toasts
                        .push(ToastKind::Success, format!("已登录：{}", view.account_name));
                }
                if view.phase == crate::dto::DeviceLoginPhase::Failed {
                    self.toasts.push(ToastKind::Error, view.message.clone());
                }
                self.device_login = view;
            }
            CoreEvent::JavaChanged(page) => self.java = page,
            CoreEvent::InstallProgress(progress) => match progress {
                Some(progress) => {
                    let terminal = progress.stage.is_terminal();
                    self.install = Some(progress);
                    if terminal {
                        self.wizard.open = false;
                        self.wizard.submitting = false;
                    }
                }
                None => self.install = None,
            },
            CoreEvent::SettingsChanged(groups) => self.settings_groups = groups,
            CoreEvent::NoticesChanged(notices) => self.notices = notices,
        }
    }

    /// agent 事件（AI 会话的流式消息与工具调用）。
    pub fn apply_agent(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::MessageAppended(message) => {
                // 流式占位到达即视为「一轮回复进行中」：UI 的发送键/停止键据此切换
                if message.streaming {
                    self.ai.streaming = true;
                }
                self.ai.messages.push(message);
            }
            AgentEvent::MessageDelta { index, delta } => {
                if let Some(message) = self.ai.messages.get_mut(index) {
                    message.text.push_str(&delta);
                }
            }
            AgentEvent::MessageFinished { index } => {
                if let Some(message) = self.ai.messages.get_mut(index) {
                    message.streaming = false;
                }
                self.ai.streaming = false;
            }
            AgentEvent::ToolCallStarted { name, summary } => {
                self.ai.streaming = true;
                self.ai.messages.push(ChatMessage {
                    role: ChatRole::Tool,
                    text: summary.clone(),
                    tool: Some(ToolCallView {
                        name,
                        ok: false,
                        summary,
                    }),
                    streaming: true,
                });
            }
            AgentEvent::ToolCallFinished { name, ok, summary } => {
                let target =
                    self.ai.messages.iter_mut().rev().find(|message| {
                        message.tool.as_ref().is_some_and(|tool| tool.name == name)
                    });
                if let Some(message) = target {
                    message.text.clone_from(&summary);
                    message.streaming = false;
                    if let Some(tool) = &mut message.tool {
                        tool.ok = ok;
                        tool.summary = summary;
                    }
                }
            }
        }
    }

    /// 实时日志增量：只追加，封顶 [`MAX_LIVE_LINES`] 行（滚动窗口）。
    fn append_live_lines(&mut self, batch: &crate::dto::LiveLogBatch) {
        if self
            .live_session
            .as_deref()
            .is_some_and(|session| session != batch.session)
        {
            // 换了一个会话在跑：清掉上一场的行，避免两场日志混在一起
            self.log_lines.clear();
        }
        self.live_session = batch.running.then(|| batch.session.clone());
        self.selected_session = Some(batch.session.clone());
        self.log_lines.extend(batch.lines.iter().cloned());
        if self.log_lines.len() > MAX_LIVE_LINES {
            let drop = self.log_lines.len() - MAX_LIVE_LINES;
            self.log_lines.drain(..drop);
        }
    }

    /// 按搜索词过滤后的磁贴网格（顺序稳定 = 实例列表顺序）。
    pub fn tiles(&self) -> Vec<TileCell<'_>> {
        let query = self.instance_query.trim().to_lowercase();
        let filtered: Vec<&InstanceSummary> = self
            .instances
            .iter()
            .filter(|instance| query.is_empty() || instance.name.to_lowercase().contains(&query))
            .collect();
        let positions = grid_positions(filtered.len(), self.prefs.tile_size.columns());
        filtered
            .into_iter()
            .zip(positions)
            .map(|(instance, (row, col))| TileCell { instance, row, col })
            .collect()
    }

    /// 运行中的实例（实例页顶部迷你状态条）。
    pub fn running(&self) -> Vec<&InstanceSummary> {
        self.instances
            .iter()
            .filter(|instance| {
                matches!(
                    instance.runtime,
                    InstanceRuntime::Running { .. } | InstanceRuntime::Busy { .. }
                )
            })
            .collect()
    }

    /// 覆盖某实例的启动进度（mock / 真实后端共用同一入口）。
    pub fn set_progress(&mut self, id: &str, progress: Progress) {
        if let Some(instance) = self.instances.iter_mut().find(|item| item.id == id) {
            if let InstanceRuntime::Busy {
                progress: current, ..
            } = &mut instance.runtime
            {
                *current = progress;
            }
        }
    }

    /// 折叠/展开导航栏。
    pub fn toggle_nav(&mut self) {
        self.prefs.nav_collapsed = !self.prefs.nav_collapsed;
    }

    /// 折叠/展开 AI 栏；若正处在独立页形态则先收回侧栏。
    pub fn toggle_ai(&mut self) {
        if self.route() == &Route::Agent {
            self.router.back();
            self.prefs.ai_collapsed = false;
            return;
        }
        self.prefs.ai_collapsed = !self.prefs.ai_collapsed;
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
