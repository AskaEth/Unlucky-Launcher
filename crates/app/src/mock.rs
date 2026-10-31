//! P0 假后端：用脚本化事件驱动骨架，验证 app ↔ core/agent 通道语义。
//!
//! 覆盖四条通道：实例运行状态（磁贴进度环）、AI 流式会话（含工具调用）、
//! 对话框队列（进度对话框）、Toast。阶段 B 里由真正的 core/agent host 替换
//! [`MockBackend`]，[`crate::dto::Backend`] 之上的 state/reducer 不动。
//! 假数据本体在 [`crate::mock_data`]。

use std::collections::VecDeque;
use std::time::Duration;

use crate::dialogs::{DialogKind, DialogRequest, DialogSource};
use crate::dto::{
    AgentEvent, AppCommand, AppEvent, Backend, ChatMessage, ChatRole, CoreEvent, InstanceId,
    InstanceRuntime, LaunchStage, Progress,
};
use crate::mock_data::{mock_instances, mock_log_lines, mock_log_sessions, mock_versions};
use crate::toast::{Toast, ToastKind};

/// 启动脚本：按时间推进 download → verify → launch → running。
#[derive(Debug)]
struct LaunchScript {
    id: InstanceId,
    elapsed_ms: u64,
}

/// AI 流式脚本：每条提问回一段可切分的话 + 一次工具调用。
#[derive(Debug)]
struct ChatScript {
    elapsed_ms: u64,
    chunks: VecDeque<String>,
    /// 流式助手消息在 app 会话里的序号
    message_index: usize,
}

/// 假后端。
#[derive(Debug, Default)]
pub struct MockBackend {
    events: VecDeque<AppEvent>,
    launch: Option<LaunchScript>,
    chat: Option<ChatScript>,
    /// 已投递给 app 的会话消息条数（用来给 MessageDelta 算序号）
    emitted_messages: usize,
    next_dialog: u64,
}

impl MockBackend {
    /// 预置一批假数据事件（实例/版本/日志会话）。
    pub fn new() -> Self {
        let mut backend = Self::default();
        backend.seed();
        backend
    }

    fn seed(&mut self) {
        self.events.push_back(AppEvent::Core(
            CoreEvent::InstancesChanged(mock_instances()),
        ));
        self.events
            .push_back(AppEvent::Core(CoreEvent::VersionListChanged(
                mock_versions(),
            )));
        self.events
            .push_back(AppEvent::Core(CoreEvent::LogSessionsChanged(
                mock_log_sessions(),
            )));
        self.events
            .push_back(AppEvent::Core(CoreEvent::LogLinesChanged(mock_log_lines())));
    }

    fn emit(&mut self, event: AppEvent) {
        self.events.push_back(event);
    }

    fn next_dialog_id(&mut self) -> u64 {
        self.next_dialog += 1;
        self.next_dialog
    }

    fn script_launch(&mut self, id: InstanceId) {
        self.emit(AppEvent::Core(CoreEvent::InstanceRuntimeChanged {
            id: id.clone(),
            runtime: InstanceRuntime::Busy {
                stage: LaunchStage::Download,
                progress: Progress {
                    done: 0,
                    total: 1_500,
                },
            },
        }));
        self.launch = Some(LaunchScript { id, elapsed_ms: 0 });
    }

    fn step_launch(&mut self, elapsed: Duration) {
        let Some((id, elapsed_ms)) = self.launch.as_mut().map(|script| {
            script.elapsed_ms += u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
            (script.id.clone(), script.elapsed_ms)
        }) else {
            return;
        };

        // 0~1.5s 下载、1.5~2.5s 校验、2.5~3s 启动、3s 后转为运行中
        let runtime = if elapsed_ms < 1_500 {
            InstanceRuntime::Busy {
                stage: LaunchStage::Download,
                progress: Progress {
                    done: elapsed_ms,
                    total: 1_500,
                },
            }
        } else if elapsed_ms < 2_500 {
            InstanceRuntime::Busy {
                stage: LaunchStage::Verify,
                progress: Progress {
                    done: elapsed_ms - 1_500,
                    total: 1_000,
                },
            }
        } else if elapsed_ms < 3_000 {
            InstanceRuntime::Busy {
                stage: LaunchStage::Launch,
                progress: Progress {
                    done: elapsed_ms - 2_500,
                    total: 500,
                },
            }
        } else {
            self.launch = None;
            self.emit(AppEvent::Toast(Toast {
                id: 0,
                kind: ToastKind::Success,
                text: "游戏已启动".to_string(),
                remaining_ms: 0,
            }));
            InstanceRuntime::Running { pid: 12_345 }
        };

        self.emit(AppEvent::Core(CoreEvent::InstanceRuntimeChanged {
            id,
            runtime,
        }));
    }

    fn script_chat(&mut self, text: String) {
        let user_index = self.emitted_messages;
        self.emit(AppEvent::Agent(AgentEvent::MessageAppended(
            ChatMessage::user(text),
        )));
        self.emit(AppEvent::Agent(AgentEvent::MessageAppended(
            ChatMessage::assistant_stream(),
        )));
        self.emitted_messages += 2;
        self.chat = Some(ChatScript {
            elapsed_ms: 0,
            chunks: VecDeque::from(vec![
                "先确认版本：".to_string(),
                "官方 1.20.4 是正式版，".to_string(),
                "装完再用 Fabric 加加载器。".to_string(),
            ]),
            message_index: user_index + 1,
        });
    }

    fn step_chat(&mut self, elapsed: Duration) {
        let Some(chat) = self.chat.as_mut() else {
            return;
        };
        chat.elapsed_ms += u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        if chat.elapsed_ms < 300 {
            return;
        }
        chat.elapsed_ms = 0;
        let index = chat.message_index;
        let chunk = chat.chunks.pop_front();

        if let Some(delta) = chunk {
            self.emit(AppEvent::Agent(AgentEvent::MessageDelta { index, delta }));
            return;
        }

        // 流结束后补一次工具调用（工具执行在会话里可见）+ 收尾消息
        self.emit(AppEvent::Agent(AgentEvent::ToolCallStarted {
            name: "read_game_log".to_string(),
            summary: "读取最近一次崩溃日志".to_string(),
        }));
        self.emit(AppEvent::Agent(AgentEvent::ToolCallFinished {
            name: "read_game_log".to_string(),
            ok: true,
            summary: "读到 128 行，无 ERROR".to_string(),
        }));
        self.emit(AppEvent::Agent(AgentEvent::MessageFinished { index }));
        self.emit(AppEvent::Agent(AgentEvent::MessageAppended(ChatMessage {
            role: ChatRole::Assistant,
            text: "日志没有 ERROR，问题更可能在 Java 版本。".to_string(),
            tool: None,
            streaming: false,
        })));
        self.emitted_messages += 2;
        self.chat = None;
    }

    fn script_install(&mut self, version: String) {
        let id = self.next_dialog_id();
        self.emit(AppEvent::DialogRequested(DialogRequest {
            id,
            source: DialogSource::BackgroundTask,
            title: format!("正在安装 {version}"),
            body: String::new(),
            kind: DialogKind::Progress {
                label: "下载资源".to_string(),
                progress: 0.0,
            },
        }));
        self.emit(AppEvent::DialogClosed(id));
        self.emit(AppEvent::Toast(Toast {
            id: 0,
            kind: ToastKind::Success,
            text: format!("{version} 安装完成"),
            remaining_ms: 0,
        }));
    }
}

impl Backend for MockBackend {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        self.events.drain(..).collect()
    }

    fn dispatch(&mut self, command: AppCommand) {
        match command {
            AppCommand::LaunchInstance(id) => self.script_launch(id),
            AppCommand::StopInstance(id) => {
                self.launch = None;
                self.emit(AppEvent::Core(CoreEvent::InstanceRuntimeChanged {
                    id,
                    runtime: InstanceRuntime::Idle,
                }));
            }
            AppCommand::AskAgent(text) => self.script_chat(text),
            AppCommand::InstallVersion(version) => self.script_install(version),
            AppCommand::SetYolo(enabled) => self.emit(AppEvent::Toast(Toast {
                id: 0,
                kind: ToastKind::Info,
                text: if enabled {
                    "YOLO 已开启（审计照记）"
                } else {
                    "YOLO 已关闭"
                }
                .to_string(),
                remaining_ms: 0,
            })),
            AppCommand::ListVersions(category) => {
                self.emit(AppEvent::Core(CoreEvent::VersionListChanged(
                    crate::mock_data::mock_versions_for(category),
                )));
            }
            AppCommand::OpenLogSession(id) => {
                // P0：换会话 = 换一批假日志行 + 一条诊断结论（日志页诊断卡片可见）
                self.emit(AppEvent::Core(CoreEvent::LogLinesChanged(mock_log_lines())));
                self.emit(AppEvent::Core(CoreEvent::DiagnosisReady(
                    crate::mock_data::mock_diagnosis(id),
                )));
            }
            AppCommand::RefreshInstances
            | AppCommand::RefreshLogSessions
            | AppCommand::StopAgent => {}
        }
    }

    fn tick(&mut self, elapsed: Duration) {
        self.step_launch(elapsed);
        self.step_chat(elapsed);
    }
}

#[cfg(test)]
mod tests {
    use super::MockBackend;
    use crate::dto::{AgentEvent, AppCommand, AppEvent, Backend, CoreEvent, InstanceRuntime};

    #[test]
    fn seeds_instances_and_versions() {
        let mut backend = MockBackend::new();
        let events = backend.drain_events();
        assert_eq!(events.len(), 4);
        assert!(events
            .iter()
            .all(|event| matches!(event, AppEvent::Core(_))));
        assert!(backend.drain_events().is_empty());
    }

    #[test]
    fn launching_emits_progress_then_running() {
        let mut backend = MockBackend::new();
        backend.drain_events();
        backend.dispatch(AppCommand::LaunchInstance("survival".to_string()));

        let first = backend.drain_events();
        assert!(matches!(
            first[0],
            AppEvent::Core(CoreEvent::InstanceRuntimeChanged { .. })
        ));

        backend.tick(std::time::Duration::from_millis(1_600));
        let verify = backend.drain_events();
        match &verify[0] {
            AppEvent::Core(CoreEvent::InstanceRuntimeChanged {
                runtime: InstanceRuntime::Busy { stage, .. },
                ..
            }) => assert_eq!(*stage, crate::dto::LaunchStage::Verify),
            other => panic!("unexpected event: {other:?}"),
        }

        backend.tick(std::time::Duration::from_millis(2_000));
        let done = backend.drain_events();
        assert!(done.iter().any(|event| matches!(
            event,
            AppEvent::Core(CoreEvent::InstanceRuntimeChanged {
                runtime: InstanceRuntime::Running { .. },
                ..
            })
        )));
    }

    #[test]
    fn stopping_cancels_the_launch_script() {
        let mut backend = MockBackend::new();
        backend.drain_events();
        backend.dispatch(AppCommand::LaunchInstance("a".to_string()));
        backend.drain_events();

        backend.dispatch(AppCommand::StopInstance("a".to_string()));
        let events = backend.drain_events();
        assert!(matches!(
            events[0],
            AppEvent::Core(CoreEvent::InstanceRuntimeChanged {
                runtime: InstanceRuntime::Idle,
                ..
            })
        ));

        // 脚本已取消：再推进时间也不会再产出事件
        backend.tick(std::time::Duration::from_millis(5_000));
        assert!(backend.drain_events().is_empty());
    }

    #[test]
    fn asking_the_agent_streams_and_finishes() {
        let mut backend = MockBackend::new();
        backend.drain_events();
        backend.dispatch(AppCommand::AskAgent("装哪个版本".to_string()));

        let start = backend.drain_events();
        assert_eq!(start.len(), 2);

        let mut deltas = 0;
        for _ in 0..6 {
            backend.tick(std::time::Duration::from_millis(400));
            let events = backend.drain_events();
            deltas += events
                .iter()
                .filter(|event| matches!(event, AppEvent::Agent(AgentEvent::MessageDelta { .. })))
                .count();
        }
        assert_eq!(deltas, 3);
    }
}
