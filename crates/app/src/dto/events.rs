//! 事件与指令：core/agent → app 的事件队列，app → core/agent 的用户意图。

use crate::dialogs::{DialogId, DialogRequest};
use crate::dto::{AgentEvent, DiagnosisReport, InstanceId, InstanceRuntime, InstanceSummary};
use crate::dto::{LogLine, LogSession, SessionId, VersionCategory, VersionSummary};
use crate::toast::Toast;

/// core 侧事件（不可变快照发布 + 事件流，01c §5 的三条通道之二）。
#[derive(Debug, Clone, PartialEq)]
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
    /// 当前会话日志行刷新（P0 整批替换；P1 改增量追加）
    LogLinesChanged(Vec<LogLine>),
    /// 诊断结论产出
    DiagnosisReady(DiagnosisReport),
}

/// 统一事件队列的元素：core / agent / UI 自产（对话框、Toast）都走这一条路，
/// 这样 [`crate::state::AppState::apply`] 是唯一的状态变更入口。
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCommand {
    /// 重新扫描实例目录
    RefreshInstances,
    /// 启动实例
    LaunchInstance(InstanceId),
    /// 停止实例（含取消启动中的管线）
    StopInstance(InstanceId),
    /// 拉取某分类的版本列表
    ListVersions(VersionCategory),
    /// 安装指定版本
    InstallVersion(String),
    /// 刷新日志会话列表
    RefreshLogSessions,
    /// 打开某个会话（拉取日志行 + 触发诊断）
    OpenLogSession(SessionId),
    /// 向 AI 提问（会话状态在 app 侧，agent 只回事件）
    AskAgent(String),
    /// 中断当前 AI 流
    StopAgent,
    /// YOLO 开关（跳过确认，审计照记）
    SetYolo(bool),
}

/// 后端通道：core/agent host 实现它；P0 用 [`crate::mock::MockBackend`] 验证通道语义。
///
/// 轮询模型（而非回调）是为了让 Slint 的定时器成为唯一调度者，
/// 避免跨线程回调里碰 Slint 模型。
pub trait Backend {
    /// 取走自上次调用以来积压的事件（FIFO，不可丢序号）。
    fn drain_events(&mut self) -> Vec<AppEvent>;

    /// 下发一条用户意图。
    fn dispatch(&mut self, command: AppCommand);

    /// 时间推进（mock 用脚本推进进度；真实后端可以是空实现）。
    fn tick(&mut self, elapsed: std::time::Duration) {
        let _ = elapsed;
    }
}
