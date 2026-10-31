//! 后台任务的回流消息：`HostTask` 是唯一「线程 → host」的载荷。
//!
//! 线程只做 core 调用（网络/磁盘），产出的都是 `crate::dto` 的 DTO；
//! host 的 `tick` 把它们翻成 [`crate::dto::CoreEvent`]。
//! 这样「跨线程传的是什么」在一处看全，也不会出现半成品领域类型漏回 UI。

use std::sync::mpsc::{Receiver, Sender};

use crate::dto::{
    AccountView, DeviceLoginView, DiagnosisReport, InstallProgressView, InstanceId,
    InstanceRuntime, JavaPageView, LaunchFinished, LiveLogBatch, LogLine, LogOutcome, LogSession,
    NoticeView, SessionId, VersionSummary,
};
use crate::toast::Toast;

/// 后台任务 → host 的消息。
#[derive(Debug, Clone, PartialEq)]
pub enum HostTask {
    /// 版本清单（下载页）
    Versions(Result<Vec<VersionSummary>, String>),
    /// 安装进度（高频，host 只做聚合与转发）
    Install(Box<InstallProgressView>),
    /// 安装收尾（成功带实例名，失败带文案）
    InstallDone {
        /// 最终进度快照
        progress: Box<InstallProgressView>,
        /// 成功时为 `Ok(实例名)`
        result: Result<String, String>,
    },
    /// 账户列表
    Accounts(Vec<AccountView>),
    /// 设备码登录状态
    Login(Box<DeviceLoginView>),
    /// Java 页（发行版/回退链/运行时）
    Java(Box<JavaPageView>),
    /// 日志页快照（会话列表 + 选中会话的行 + 诊断）
    Logs {
        /// 会话列表
        sessions: Vec<LogSession>,
        /// 选中会话的行
        lines: Vec<LogLine>,
        /// 诊断结论
        diagnosis: Option<DiagnosisReport>,
    },
    /// 实时日志增量
    Tail(Box<LiveLogBatch>),
    /// 启动开始（pid + 会话 id）
    LaunchStarted {
        /// 实例 id
        id: InstanceId,
        /// 会话 id
        session_id: SessionId,
        /// 进程号
        pid: u32,
    },
    /// 启动阶段推进（磁贴进度环）
    Runtime {
        /// 实例 id
        id: InstanceId,
        /// 阶段状态
        runtime: InstanceRuntime,
    },
    /// 启动收尾
    LaunchEnd(Box<LaunchFinished>),
    /// 崩溃日志导入的分析结论
    Crash(Box<DiagnosisReport>),
    /// 横幅通知（隔离切换提示等）
    Notice(Box<NoticeView>),
    /// Toast
    Toast(Box<Toast>),
    /// 会话列表刷新（只换左栏）
    Sessions(Vec<LogSession>),
}

/// 后台线程用的发送端（`Sender` 克隆即可）。
pub type JobSender = Sender<HostTask>;

/// host 侧的接收端。
pub type JobReceiver = Receiver<HostTask>;

/// 建一对通道。
pub fn channel() -> (JobSender, JobReceiver) {
    std::sync::mpsc::channel()
}

/// 起一个带名字的后台线程（名字只进 panic 信息，方便定位）。
pub fn spawn(name: &str, job: impl FnOnce() + Send + 'static) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name(format!("ull-host-{name}"))
        .spawn(job)
        .unwrap_or_else(|error| panic!("起不来后台线程 {name}: {error}"))
}

/// 人读一行「启动结束」。
pub fn launch_summary(outcome: LogOutcome, exit_code: Option<i32>) -> String {
    match outcome {
        LogOutcome::Ok => "正常退出".to_string(),
        LogOutcome::Crashed => match exit_code {
            Some(code) => format!("崩溃（exit {code}）"),
            None => "崩溃".to_string(),
        },
        LogOutcome::SilentFail => "静默失败（exit 0 但游戏没起来）".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_carries_tasks_across_threads() {
        let (tx, rx) = channel();
        let handle = spawn("test", move || {
            tx.send(HostTask::Toast(Box::new(Toast {
                id: 1,
                kind: crate::toast::ToastKind::Info,
                text: "hi".to_string(),
                remaining_ms: 0,
            })))
            .expect("发送");
        });
        handle.join().expect("线程结束");
        let task = rx.recv().expect("收到");
        assert!(matches!(task, HostTask::Toast(toast) if toast.text == "hi"));
    }

    #[test]
    fn launch_summary_names_the_outcome() {
        assert_eq!(launch_summary(LogOutcome::Ok, Some(0)), "正常退出");
        assert_eq!(
            launch_summary(LogOutcome::Crashed, Some(1)),
            "崩溃（exit 1）"
        );
        assert!(launch_summary(LogOutcome::SilentFail, Some(0)).contains("静默"));
    }
}
