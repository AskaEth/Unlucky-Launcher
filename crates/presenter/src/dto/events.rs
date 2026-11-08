//! 事件与指令：core/agent → app 的事件队列，app → core/agent 的用户意图。
//!
//! 三条通道（01c §5）：①不可变快照发布（`*Changed` 整体替换）②事件流（`LogAppended` /
//! `InstallProgress` / `DeviceLoginChanged`）③注入的显示服务（先落成事件，见 `DialogRequested`）。
//! 全部类型 `Serialize + Deserialize`：这是 IPC 契约（AGENTS §6）。

use serde::{Deserialize, Serialize};

use crate::dialogs::{DialogId, DialogRequest};
use crate::dto::CreateRequest;
use crate::dto::{
    // 主线（M109 的 Content* / M113 的 Pack*）与 M114 的 Authlib*/AccountNotice 的并集。
    AccountNotice,
    AccountView,
    AgentEvent,
    AuthlibLoginView,
    AuthlibRequest,
    ContentInstallRequest,
    ContentQuery,
    ContentResults,
    DeviceLoginView,
    DiagnosisReport,
    InstallProgressView,
    InstanceId,
    InstanceRuntime,
    InstanceSummary,
    JavaPageView,
    LaunchArgsPatch,
    LaunchFailureView,
    LoaderKind,
    LogLine,
    LogOutcome,
    LogSession,
    NoticeView,
    PackCommand,
    PackEvent,
    SessionId,
    SettingGroupView,
    // M159: 自更新 —— 一条快照整体替换
    UpdateView,
    VersionCategory,
    VersionSummary,
};
use crate::toast::Toast;

/// 启动会话的一段实时日志（host 尾随 JSONL 后按批次发）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveLogBatch {
    /// 会话 id
    pub session: SessionId,
    /// 新增行
    pub lines: Vec<LogLine>,
    /// 会话是否还在跑（false = 收尾，可以触发诊断/刷新列表）
    pub running: bool,
}

/// 一次启动收尾（磁贴状态回写 + 日志页会话列表刷新）。
///
/// **不带人读散文**（M161）：早先这里有一个 `summary: String`，内容是 presenter 拼的中文
/// （`正常退出` / `崩溃（exit 1）` / `启动失败：找不到 Java`）。于是英文界面上弹出的是中文 Toast，
/// 磁贴那行还叠成「Launch failed · 启动失败：…」。现在只给**结构**，文案由 app 层按 key 现求
/// （与 `InstallCancelled` 同一条纪律：presenter 不依赖 `unlucky-i18n`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchFinished {
    /// 实例 id
    pub id: InstanceId,
    /// 会话 id
    pub session: SessionId,
    /// 结局
    pub outcome: LogOutcome,
    /// 进程退出码（`None` = 进程没起来 / 平台没给）
    pub exit_code: Option<i32>,
    /// 从 spawn 到收尾的毫秒数（没起来时是 0）
    pub uptime_ms: u64,
    /// 失败发生在哪一步（`None` = 这一场没有装配/启动阶段的失败；
    /// 「跑起来又崩了」看 `outcome`）
    pub failure: Option<LaunchFailureView>,
}

/// core 侧事件（不可变快照发布 + 事件流，01c §5 的三条通道之二）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CoreEvent {
    /// 实例列表整体刷新
    InstancesChanged(Vec<InstanceSummary>),
    /// 单个实例运行时状态变化（启动进度/运行中/失败）
    InstanceRuntimeChanged {
        /// 实例 id
        id: InstanceId,
        /// 新状态
        runtime: InstanceRuntime,
    },
    /// 下载页版本列表刷新
    VersionListChanged(Vec<VersionSummary>),
    /// 日志会话列表刷新
    LogSessionsChanged(Vec<LogSession>),
    /// 当前会话日志行刷新（整批替换）
    LogLinesChanged(Vec<LogLine>),
    /// 实时日志增量（启动中尾随 JSONL；`running=false` 表示会话收尾）
    LogAppended(LiveLogBatch),
    /// 启动收尾（磁贴状态回写 + toast）
    LaunchFinished(LaunchFinished),
    /// 诊断结论产出
    DiagnosisReady(DiagnosisReport),
    /// 账户列表刷新
    AccountsChanged(Vec<AccountView>),
    /// 微软登录状态刷新
    DeviceLoginChanged(DeviceLoginView),
    /// 第三方轨（authlib-injector）登录/校验状态刷新
    AuthlibLoginChanged(AuthlibLoginView),
    /// 账户页横幅（添加/删除/切换的结果；`None` = 清掉横幅）
    AccountNoticeChanged(Option<AccountNotice>),
    /// Java 页快照（发行版/回退链/运行时）
    JavaChanged(JavaPageView),
    /// 安装进度（`None` = 清掉进度面板）
    InstallProgress(Option<InstallProgressView>),
    /// 用户取消了安装（**不带人读文案**）
    ///
    /// 为什么只有信号没有文本：`presenter` 不依赖 `unlucky-i18n`，任何写死的中文都会和
    /// 页面语言打架（下载页目前渲染英文，一条中文 Toast 会**同屏一中一英**，复审 P2-2）。
    /// 文案由 app 层按 `Key` 现求：见 `pages::download::cancelled_toast`。
    InstallCancelled,
    /// 某 (加载器, 原版版本) 可用的加载器版本列表（下载页「安装选项」的版本下拉）。
    ///
    /// `result` 用 `Result` 而不是空列表：**取不到**与**确实没有**是两件事，
    /// 前者要给具名错误（不许让下拉显示成空还以为能装），后者是「这个组合官方就没有版本」。
    LoaderVersions {
        /// 加载器种类
        kind: LoaderKind,
        /// 目标原版版本
        mc_version: String,
        /// `Ok(最新在前)` / `Err(人读原因)`
        result: Result<Vec<String>, String>,
    },
    /// 设置分组刷新（设置页右栏）
    SettingsChanged(Vec<SettingGroupView>),
    /// 横幅通知刷新（隔离切换提示/启动失败等）
    NoticesChanged(Vec<NoticeView>),
    /// 内容库检索结果（M109）：**整批替换**（同一份 `query` 的命中项，含分页游标）
    ContentResults(Box<ContentResults>),
    /// 内容库检索失败（M109；M162 起带大类）：没网 / 站点 5xx / 缺密钥要能分开画
    ContentFailed(Box<crate::dto::ContentFailure>),
    /// 内容库项目详情 + 版本列表（M162）：`project` 是**请求时**选中的 id，迟到结果靠它判。
    ContentDetail {
        /// 请求时选中的项目 id
        project: String,
        /// `Ok` = 详情；`Err` = 失败大类 + 原因
        result: Box<Result<crate::dto::ContentDetail, crate::dto::ContentFailure>>,
    },
    // M113: import/export
    /// 导入/导出（M113）：进度、结果、探测诊断、导出预检（子枚举见 [`PackEvent`]）
    Pack(Box<PackEvent>),
    /// 自更新（M159）：整份状态快照（检查结论 + 就位状态 + 用户是否已拒绝）。
    ///
    /// 与 `SettingsChanged` 同一条形态：快照整体替换，UI 不做增量合并。
    Update(Box<UpdateView>),
}

/// 统一事件队列的元素：core / agent / UI 自产（对话框、Toast）都走这一条路，
/// 这样 [`crate::state::AppState::apply`] 是唯一的状态变更入口。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AppEvent {
    /// 来自 core
    Core(CoreEvent),
    /// 来自 agent
    Agent(AgentEvent),
    /// core/agent 请求弹对话框（会进队列，一次一条）
    DialogRequested(DialogRequest),
    /// 关闭指定对话框（任务完成收起进度条）
    DialogClosed(DialogId),
    /// 右下角 Toast
    Toast(Toast),
}

/// app → core/agent 的用户意图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppCommand {
    /// 重新扫描实例目录 + 全部页面快照（启动时与手改文件后）
    RefreshAll,
    /// 启动实例
    LaunchInstance(InstanceId),
    /// 停止实例（含取消启动中的管线）
    StopInstance(InstanceId),
    /// 拉取某分类的版本列表
    ListVersions(VersionCategory),
    /// 安装指定版本（下载页行内「安装」按钮；原版）
    InstallVersion(String),
    /// 取消安装（阶段边界生效：当前阶段跑完就停，不再进下一阶段）
    CancelInstall,
    /// 新建实例（向导提交：版本 + 加载器 + 名字）
    CreateInstance(CreateRequest),
    /// 拉取某原版版本可用的加载器版本（下载页「安装选项」的版本下拉）
    ListLoaderVersions {
        /// 加载器种类
        kind: LoaderKind,
        /// 目标原版版本（`1.20.1`）
        mc_version: String,
    },
    /// 修改实例隔离档位（写实例设置文件）
    SetIsolation {
        /// 实例 id
        id: InstanceId,
        /// `isolated` / `shared`
        mode: String,
    },
    /// 写实例启动参数（M157 的「启动参数」区：JVM/游戏参数、内存、Java、图形后端）
    ///
    /// 补丁语义：`None` = 这一项不改；`Some("")` = 清空回「跟随全局」。
    SetInstanceLaunchArgs {
        /// 实例 id
        id: InstanceId,
        /// 五项补丁
        patch: LaunchArgsPatch,
    },
    /// 刷新日志会话列表
    RefreshLogSessions,
    /// 打开某个会话（拉取日志行 + 触发诊断）
    OpenLogSession(SessionId),
    /// 导入崩溃日志文件做规则分析（`hs_err_pid*.log` / `latest.log`）
    ImportCrashLog(String),
    /// 加一个离线账户
    AddOfflineAccount(String),
    /// 删除账户（按 id 或名字）
    RemoveAccount(String),
    /// 切换当前启动账户
    SwitchAccount(String),
    /// 开始微软设备码登录（CLI/无浏览器场景、备用通道）
    StartDeviceLogin,
    /// 开始微软授权码登录（拉起系统默认浏览器 + 本地回调，GUI 的默认入口）
    StartBrowserLogin,
    /// 取消/关掉微软登录面板（已发出的请求会在收尾时丢弃结果）
    CancelDeviceLogin,
    /// 加一个第三方皮肤站（authlib-injector）账户：校验服务器 + 密码登录
    ///
    /// **凭据红线**：`request.password` 只是这一次的请求值，host 用完即弃，绝不进回程 DTO。
    AddAuthlibAccount(AuthlibRequest),
    /// 只校验第三方皮肤站地址（服务发现）：结果走 `AuthlibLoginChanged`
    ProbeAuthlibServer(String),
    /// 关掉账户页横幅（页面上的 × / Dismiss）
    DismissAccountNotice,
    /// 安装指定发行版的托管 JDK（`temurin` / `oracle` / `microsoft` / `zulu`）
    InstallJava(String),
    /// 写一条设置（`key` 必须是 core schema 里的键）
    SetSetting {
        /// schema 键
        key: String,
        /// 新值（字符串形态；host 按 schema 类型转换）
        value: String,
    },
    /// 关掉一条横幅通知
    DismissNotice(String),
    /// 向 AI 提问（会话状态在 app 侧，agent 只回事件）
    AskAgent(String),
    /// 中断当前 AI 流
    StopAgent,
    /// YOLO 开关（跳过确认，审计照记）
    SetYolo(bool),
    /// 检索内容库（M109；结果经 [`CoreEvent::ContentResults`] 回来）
    SearchContent(ContentQuery),
    /// 把某个内容项目装进指定实例（M109；整合包不支持，host 会拒绝并给原因）
    InstallContent(Box<ContentInstallRequest>),
    /// 取某个内容项目的详情与版本列表（M162；结果经 [`CoreEvent::ContentDetail`] 回来）
    LoadContentDetail(Box<crate::dto::ContentDetailRequest>),
    // M113: import/export
    /// 整合包导入/导出（M113）：导入 / 预检 / 确认写盘 / 关面板（子枚举见 [`PackCommand`]）
    Pack(Box<PackCommand>),
    // M159: 自更新
    /// 查一次更新（设置页「立即检查」；会清掉「忽略这个版本」的标记并重新请求网络）
    CheckUpdate,
    /// 把当前候选下载下来、校验、替换本程序（结果经 [`CoreEvent::Update`] 回来）
    ApplyUpdate,
    /// 忽略**这一个候选版本**（记的是版本号，不是「别再问我」）。
    ///
    /// 语义三条（`UpdateView::skipped` 的契约，`dto::update` 的模块文档与用例守着）：
    /// 同一次运行里同一个版本不再弹提示条；**更高的版本照弹**（0.2.3 被忽略不影响 0.2.4）；
    /// 设置页的「立即检查」会清掉它。候选本身仍留在快照里，设置页可手动装。
    DismissUpdate,
    /// 开关「启动时静默检查」。
    ///
    /// 落点是主配置 `config/launcher.json` 的 `update.auto_check`（M161 从更新域自己的
    /// `cache/update/prefs.json` 迁过来的 schema 键）——`ull config get update.auto_check`
    /// 读到的是同一个值。
    SetUpdateAutoCheck(bool),
}

impl AppCommand {
    /// 构造「切换隔离档位」指令（磁贴/详情页共用）。
    pub fn set_isolation(id: &str, mode: &str) -> Self {
        Self::SetIsolation {
            id: id.to_string(),
            mode: mode.to_string(),
        }
    }

    /// 构造「写实例启动参数」指令（详情页「启动参数」区）。
    pub fn set_launch_args(id: &str, patch: LaunchArgsPatch) -> Self {
        Self::SetInstanceLaunchArgs {
            id: id.to_string(),
            patch,
        }
    }
}

/// 后端通道：core/agent host 实现它；[`crate::mock::MockBackend`] 验证通道语义。
///
/// 轮询模型（而非回调）是为了让 Slint 的定时器成为唯一调度者，
/// 避免跨线程回调里碰 Slint 模型。
pub trait Backend {
    /// 取走自上次调用以来积压的事件（FIFO，不可丢序号）。
    fn drain_events(&mut self) -> Vec<AppEvent>;

    /// 下发一条用户意图。
    fn dispatch(&mut self, command: AppCommand);

    /// 时间推进（真实后端在这里收后台线程的结果并尾随日志）。
    fn tick(&mut self, elapsed: std::time::Duration) {
        let _ = elapsed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{AccountKind, DeviceLoginPhase, InstallStage};

    #[test]
    fn events_are_serializable_ipc_contracts() {
        let event = AppEvent::Core(CoreEvent::InstallProgress(Some(InstallProgressView {
            stage: InstallStage::Downloading,
            files_done: 3,
            files_total: 9,
            ..InstallProgressView::new()
        })));
        let text = serde_json::to_string(&event).expect("序列化");
        let back: AppEvent = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back, event);

        let account = AppEvent::Core(CoreEvent::AccountsChanged(vec![AccountView {
            id: "account:1".to_string(),
            name: "Steve".to_string(),
            kind: AccountKind::Offline,
            profile_id: "0f8f".to_string(),
            login_name: String::new(),
            created: "2026-10-31 20:12".to_string(),
            active: true,
            server_url: String::new(),
            skin: None,
        }]));
        let text = serde_json::to_string(&account).expect("序列化");
        let back: AppEvent = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back, account);
        // 凭据永不跨界：序列化文本里没有 token 类字段
        assert!(!text.to_lowercase().contains("token"));
    }

    #[test]
    fn commands_round_trip() {
        let command = AppCommand::CreateInstance(CreateRequest {
            version: "1.20.4".to_string(),
            loader: crate::dto::LoaderKind::Fabric,
            loader_version: "0.16.5".to_string(),
            name: "生存".to_string(),
        });
        let text = serde_json::to_string(&command).expect("序列化");
        let back: AppCommand = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back, command);

        let login = AppCommand::StartDeviceLogin;
        assert_eq!(
            serde_json::from_str::<AppCommand>(&serde_json::to_string(&login).expect("序列化"))
                .expect("反序列化"),
            login
        );
        assert!(DeviceLoginPhase::Waiting.busy());

        // 第三方轨（M114）：请求进、状态出，两边都不带 token 字段
        let authlib = AppCommand::AddAuthlibAccount(AuthlibRequest::new(
            "https://littleskin.cn/api/yggdrasil/",
            "me@example.com",
            "hunter2",
        ));
        let text = serde_json::to_string(&authlib).expect("序列化");
        assert_eq!(
            serde_json::from_str::<AppCommand>(&text).expect("反序列化"),
            authlib
        );

        let view = AppEvent::Core(CoreEvent::AuthlibLoginChanged(AuthlibLoginView::reachable(
            "https://littleskin.cn/api/yggdrasil/",
            "LittleSkin",
        )));
        let text = serde_json::to_string(&view).expect("序列化");
        let back: AppEvent = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back, view);
        assert!(
            !text.to_lowercase().contains("token"),
            "回程视图里不该有 token"
        );
    }
}
