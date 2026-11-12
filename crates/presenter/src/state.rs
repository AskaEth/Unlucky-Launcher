//! UI 状态与 reducer：UI 框架只负责渲染，所有状态变更都汇聚到 [`AppState::apply`]。
//!
//! 数据流向：mock/host 后端 → [`crate::dto::AppEvent`] 队列 → `apply` → 状态字段 → UI 属性。
//! 偏好项在 [`crate::prefs`] 里登记了持久化键名，core::setting 的 schema 落地后按这些键读写。

use crate::agent::AgentRail;
use crate::dialogs::DialogQueue;
use crate::dto::{
    // 主线（M105 的 `LoaderKind` / M113 的 `PackState`）与 M114 的 `AuthlibLoginView` 并集。
    AccountView,
    AdoptState,
    AppEvent,
    AuthlibLoginView,
    CoreEvent,
    DeviceLoginView,
    DiagnosisReport,
    InstallProgressView,
    InstanceRuntime,
    InstanceSummary,
    JavaPageView,
    LaunchFinished,
    LoaderKind,
    LogLine,
    LogSession,
    NoticeView,
    PackState,
    Progress,
    SettingGroupView,
    UpdateView,
    VersionSummary,
};
use crate::prefs::UiPrefs;
// 内容库的视图态与其 DTO 放一起（`dto/content.rs`），路径 `state::ContentState` 不变。
pub use crate::dto::ContentState;
use crate::router::{AiPanelMode, Route, Router};
use crate::toast::ToastCenter;
use crate::wizard::WizardState;

// 账户/登录状态（倒计时、上屏前二次脱敏）拆在 `state/accounts.rs`，守 400 行硬线。
//
// `AiSession` 不再存在：M104 把它换成了 `crate::agent::AgentRail`（agent 栏的视图模型），
// 所以这里没有 `mod ai;` / `pub use ai::AiSession;`——M114 r1 曾把旧结构搬到 `state/ai.rs`，
// 该文件已随 rebase 删除，**以 main 的 agent 契约为准**。
mod accounts;
// M223：AI 栏的事件投递与可见性（纯搬移，守 400 行硬线）
mod agent;
mod live;
// M224: 本地 Mod 列表与启停的视图态（状态落地 + 在飞标记）
mod mods;
// M235: 实例存档 / 截图的只读快照（状态落地 + 迟到丢弃）
mod saves;
mod tiles;

pub use live::MAX_LIVE_LINES;
pub use mods::ModsState;
pub use saves::{RenameDraft, SavesOp, SavesState};
pub use tiles::{grid_positions, TileCell};

/// 一份加载器版本清单查询的结果（`Ok` = 最新在前；`Err` = 人读原因）。
pub type LoaderVersions = Result<Vec<String>, String>;

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
    /// AI 会话状态（右侧栏与独立页共享同一份，切换不丢）
    pub ai: AgentRail,
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
    /// 第三方轨（authlib-injector）登录/校验状态
    pub authlib_login: AuthlibLoginView,
    /// 设备码等待态的开始时刻（倒计时；`None` = 没有在等）
    pub device_login_started: Option<std::time::Instant>,
    /// 账户页横幅（一次性操作结果；页面渲染后由用户关掉）
    pub account_notice: Option<crate::dto::AccountNotice>,
    /// Java 页快照（发行版 / 回退链 / 运行时）
    pub java: JavaPageView,
    /// 安装进度（None = 没有安装任务）
    pub install: Option<InstallProgressView>,
    /// 用户取消安装的**次数**（M105）：每次 `CoreEvent::InstallCancelled` +1。
    ///
    /// 只是信号，不带文案：app 层比对前后值决定「要不要弹一条 i18n Toast」，
    /// 于是取消提示和页面用同一种语言（复审 P2-2）。
    pub install_cancelled: u64,
    /// 启动收尾次数（M161，每次 `LaunchFinished` +1）：与 `install_cancelled` 同一条纪律——
    /// presenter 只发信号，文案由 app 层按 i18n key 现求。
    pub launch_ends: u64,
    /// 最近一次启动收尾的结构（结局 + 退出码 + 失败阶段；文案在 app 层求）。
    pub last_launch_end: Option<Box<LaunchFinished>>,
    /// 加载器版本清单（下载页「安装选项」的下拉；M105）
    ///
    /// 两段键：外层是加载器的**稳定 id**（`LoaderKind::as_str`，`LoaderKind` 没派生 `Hash`，
    /// 而 `as_str` 本来就是它在 CLI/GUI 两边的身份），内层是原版版本。
    /// **原版版本进键**，所以切到另一个 MC 版本时拿到的是另一条缓存，
    /// 不会把 A 版本的 Fabric 列表挂到 B 版本下面（M105 复审的硬要求）。
    /// 值是 `Result`：「取不到」与「官方没有版本」是两件事，前者要在页面上说清原因
    /// （并禁掉确认），不许显示成空列表。
    pub loader_versions:
        std::collections::HashMap<&'static str, std::collections::HashMap<String, LoaderVersions>>,
    /// 设置分组（设置页右栏）
    pub settings_groups: Vec<SettingGroupView>,
    /// 当前选中的设置分组（download / java / data / ai / mcp）
    pub settings_tab: String,
    /// 横幅通知
    pub notices: Vec<NoticeView>,
    /// 自更新（M159）：检查结论 / 就位状态 / 是否已拒绝（提示条与设置页共用）
    pub update: UpdateView,
    /// 新建实例向导
    pub wizard: WizardState,
    /// 正在实时尾随的会话
    pub live_session: Option<String>,
    /// 内容库（M109）：条件 / 结果 / 请求态
    pub content: ContentState,
    /// 整合包导入/导出：进度 / 结果 / 探测诊断 / 导出预检（M113）
    pub pack: PackState,
    /// 导入已有版本（M223）：扫描 / 预检 / 失败 / 完成（**只装事件态**，表单在页面本地态）
    pub adopt: AdoptState,
    /// 本地 Mod（M224）：当前实例的快照 / 失败 / 在飞启停
    pub mods: ModsState,
    /// 实例存档 / 截图（M235）：当前实例的只读快照 / 失败（可重试）
    pub saves: SavesState,
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
            ai: AgentRail::new(),
            dialogs: DialogQueue::new(),
            toasts: ToastCenter::new(),
            instance_query: String::new(),
            installed_only: false,
            download_query: String::new(),
            accounts: Vec::new(),
            device_login: DeviceLoginView::new(),
            authlib_login: AuthlibLoginView::new(),
            device_login_started: None,
            account_notice: None,
            java: JavaPageView::new(),
            install: None,
            install_cancelled: 0,
            launch_ends: 0,
            last_launch_end: None,
            loader_versions: std::collections::HashMap::new(),
            settings_groups: Vec::new(),
            settings_tab: "download".to_string(),
            notices: Vec::new(),
            update: UpdateView::default(),
            wizard: WizardState::new(),
            live_session: None,
            content: ContentState::default(),
            pack: PackState::default(),
            adopt: AdoptState::default(),
            mods: ModsState::default(),
            saves: SavesState::default(),
        }
    }

    /// 某 (加载器, 原版版本) 的加载器版本清单。
    ///
    /// `None` = 还没查过（页面据此派一次 `AppCommand::ListLoaderVersions` 并显示「正在获取…」）；
    /// `Some(Err(..))` = 查过但拿不到（页面显示具名原因并**禁掉确认**）。
    #[must_use]
    pub fn loader_versions_for(
        &self,
        kind: LoaderKind,
        mc_version: &str,
    ) -> Option<&LoaderVersions> {
        self.loader_versions.get(kind.as_str())?.get(mc_version)
    }

    /// 当前页面。
    pub fn route(&self) -> &Route {
        self.router.route()
    }

    /// 跳转（一级页替换、子页面压栈；去 AI 独立页时自动展开 AI 栏偏好）。
    pub fn navigate(&mut self, route: Route) {
        if route == Route::Agent {
            self.prefs.ai_collapsed = false;
            // 用户正在看 AI 页：攒下的未读立刻算已读（红点不该在眼前亮着）
            self.ai.mark_read();
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
                // **不在这里拼文案**（M161）：presenter 不依赖 `unlucky-i18n`，写死的中文会在
                // 英文界面上弹出来。这里只记「又收尾了一场」，Toast 由 app 层按 key 现求
                // （与 `install_cancelled` 同一条纪律，见 `app/state/refresh.rs`）。
                self.launch_ends = self.launch_ends.saturating_add(1);
                self.last_launch_end = Some(Box::new(finished));
            }
            CoreEvent::DiagnosisReady(report) => self.diagnosis = Some(report),
            CoreEvent::AccountsChanged(accounts) => self.accounts = accounts,
            CoreEvent::DeviceLoginChanged(view) => self.apply_device_login(view),
            CoreEvent::AuthlibLoginChanged(view) => self.apply_authlib_login(view),
            CoreEvent::AccountNoticeChanged(notice) => self.account_notice = notice,
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
            CoreEvent::InstallCancelled => {
                self.install_cancelled = self.install_cancelled.saturating_add(1);
            }
            CoreEvent::SettingsChanged(groups) => self.settings_groups = groups,
            CoreEvent::LoaderVersions {
                kind,
                mc_version,
                result,
            } => {
                self.loader_versions
                    .entry(kind.as_str())
                    .or_default()
                    .insert(mc_version, result);
            }
            CoreEvent::NoticesChanged(notices) => self.notices = notices,
            CoreEvent::Update(view) => self.update = *view,
            // M113: import/export
            CoreEvent::Pack(event) => self.pack.apply(*event),
            // M223: 导入已有版本（唯一写入点：`AdoptState::apply`）
            CoreEvent::Adopt(event) => self.adopt.apply(*event),
            CoreEvent::ContentResults(results) => self.apply_content(*results),
            CoreEvent::ContentFailed(failure) => self.apply_content_failure(*failure),
            // M162：详情/版本列表（判断逻辑在 `state/content.rs`，这里只有一行投递）
            CoreEvent::ContentDetail { project, result } => {
                self.apply_content_detail(&project, *result)
            }
            // M224：本地 Mod（落地判据在 `state/mods.rs`，含迟到丢弃与在飞标记）
            CoreEvent::Mods(result) => self.mods.apply(*result),
            // M235：实例存档 / 截图（落地判据在 `state/saves.rs`，含迟到丢弃）
            CoreEvent::Saves(result) => self.saves.apply(*result),
        }
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
        if !self.prefs.ai_collapsed {
            // 展开常驻栏 = 用户在看它
            self.ai.mark_read();
        }
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;

mod content;

#[cfg(test)]
mod content_tests;
