//! 事件与指令：core/agent → app 的事件队列，app → core/agent 的用户意图。
//!
//! 三条通道（01c §5）：①不可变快照发布（`*Changed` 整体替换）②事件流（`LogAppended` /
//! `InstallProgress` / `DeviceLoginChanged`）③注入的显示服务（先落成事件，见 `DialogRequested`）。
//! 全部类型 `Serialize + Deserialize`：这是 IPC 契约（AGENTS §6）。

use serde::{Deserialize, Serialize};

use crate::dialogs::{DialogId, DialogRequest};
use crate::dto::CreateRequest;
use crate::dto::{
    AccountView, AgentEvent, DeviceLoginView, DiagnosisReport, InstallProgressView, InstanceId,
    InstanceRuntime, InstanceSummary, JavaPageView, LogLine, LogOutcome, LogSession, NoticeView,
    SessionId, SettingGroupView, VersionCategory, VersionSummary,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchFinished {
    /// 实例 id
    pub id: InstanceId,
    /// 会话 id
    pub session: SessionId,
    /// 结局
    pub outcome: LogOutcome,
    /// 一句人读结论（`正常退出` / `崩溃（exit 1）` / `启动失败：…`）
    pub summary: String,
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
    /// 设备码登录状态刷新
    DeviceLoginChanged(DeviceLoginView),
    /// Java 页快照（发行版/回退链/运行时）
    JavaChanged(JavaPageView),
    /// 安装进度（`None` = 清掉进度面板）
    InstallProgress(Option<InstallProgressView>),
    /// 设置分组刷新（设置页右栏）
    SettingsChanged(Vec<SettingGroupView>),
    /// 横幅通知刷新（隔离切换提示/启动失败等）
    NoticesChanged(Vec<NoticeView>),
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
    /// 修改实例隔离档位（写实例设置文件）
    SetIsolation {
        /// 实例 id
        id: InstanceId,
        /// `isolated` / `shared`
        mode: String,
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
    /// 开始微软设备码登录
    StartDeviceLogin,
    /// 取消/关掉设备码登录面板（已发出的请求会在收尾时丢弃结果）
    CancelDeviceLogin,
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
}

impl AppCommand {
    /// 构造「切换隔离档位」指令（磁贴/详情页共用）。
    pub fn set_isolation(id: &str, mode: &str) -> Self {
        Self::SetIsolation {
            id: id.to_string(),
            mode: mode.to_string(),
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
    }
}
