//! mock 版 agent 后端：四条路径的脚本化事件流（正常问答 / 流式中途出错 / 需确认的写操作 /
//! 未读计数）。
//!
//! 与 [`crate::mock::MockBackend`] 的分工：那个是**整机演示后端**（按 tick 推进剧情，带定时器）；
//! 这个是 [`crate::agent::AgentTransport`] 的最小实现，**同步返回整轮事件分片**，专供 presenter
//! 单测与 UI 壳的回归预览（没有真实 LLM 也能把栏画全）。
//!
//! 抓得住什么：事件顺序、投影结果（消息/工具/确认/未读/审计）、脱敏闸门。
//! 抓不住什么：真实时序（这里一次性返回，真实后端是逐帧推流）、真实模型的措辞与工具选择、
//! 网络/超时/取消语义——那些要接 rig 之后跑回放回归。

use crate::agent::{AgentRail, AgentTransport};
use crate::dto::{AgentEvent, AppEvent, ChatMessage, ChatRole, ConfirmRequest, Sensitivity};

/// 脚本：一轮输入会产生哪种剧情。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    /// 正常问答：流式增量收敛成一条完整助手消息
    Qa,
    /// 流式中途断流：已到的增量保留，整轮以 `Failed` 收尾
    FailsMidStream,
    /// 需确认的写操作：工具调用开始后挂一个 High 级确认请求
    WriteOp,
}

/// 脚本化后端。
#[derive(Debug, Clone)]
pub struct MockAgentTransport {
    script: Script,
    /// 已发起过几轮（mock 用它算消息下标；真实后端自己维护会话）
    turns: usize,
    /// 当前待确认的写操作（`None` = 没有）
    pending: Option<ConfirmRequest>,
}

impl MockAgentTransport {
    /// 指定脚本。
    #[must_use]
    pub fn new(script: Script) -> Self {
        Self {
            script,
            turns: 0,
            pending: None,
        }
    }

    /// 正常问答。
    #[must_use]
    pub fn qa() -> Self {
        Self::new(Script::Qa)
    }

    /// 流式中途出错。
    #[must_use]
    pub fn fails_mid_stream() -> Self {
        Self::new(Script::FailsMidStream)
    }

    /// 需确认的写操作（改下载并发，High 级）。
    #[must_use]
    pub fn write_op() -> Self {
        Self::new(Script::WriteOp)
    }

    /// 当前待确认请求（测试断言用）。
    #[must_use]
    pub fn pending(&self) -> Option<&ConfirmRequest> {
        self.pending.as_ref()
    }
}

impl AgentTransport for MockAgentTransport {
    fn submit(&mut self, text: &str, history: &AgentRail) -> Result<Vec<AppEvent>, String> {
        self.turns += 1;
        // 后端自己算下标：用户气泡先到，助手流式占位紧随其后。
        let assistant_index = history.messages.len() + 1;
        let mut events = vec![
            agent_event(AgentEvent::MessageAppended(ChatMessage::user(text))),
            agent_event(AgentEvent::MessageAppended(ChatMessage::assistant_stream())),
        ];
        match self.script {
            Script::Qa => {
                for delta in [
                    "先确认版本：",
                    "官方 1.20.4 是正式版，",
                    "装完再加 Fabric。",
                ] {
                    events.push(agent_event(AgentEvent::MessageDelta {
                        index: assistant_index,
                        delta: delta.to_string(),
                    }));
                }
                events.push(agent_event(AgentEvent::MessageFinished {
                    index: assistant_index,
                }));
            }
            Script::FailsMidStream => {
                events.push(agent_event(AgentEvent::MessageDelta {
                    index: assistant_index,
                    delta: "我先读一下最近的日志".to_string(),
                }));
                events.push(agent_event(AgentEvent::Failed {
                    message: "provider 断流：read timeout".to_string(),
                }));
            }
            Script::WriteOp => {
                events.push(agent_event(AgentEvent::MessageDelta {
                    index: assistant_index,
                    delta: "我准备把下载并发从 64 改成 16。".to_string(),
                }));
                events.push(agent_event(AgentEvent::MessageFinished {
                    index: assistant_index,
                }));
                events.push(agent_event(AgentEvent::ToolCallStarted {
                    name: "set_config".to_string(),
                    summary: "修改 download.concurrency".to_string(),
                }));
                let request = ConfirmRequest {
                    id: format!("req-{}", self.turns),
                    tool: "set_config".to_string(),
                    summary: "把下载并发 64 → 16".to_string(),
                    details: serde_json::json!({
                        "path": "download.concurrency",
                        "old": 64,
                        "new": 16,
                    }),
                    sensitivity: Sensitivity::High,
                };
                self.pending = Some(request.clone());
                events.push(agent_event(AgentEvent::ConfirmRequested { request }));
            }
        }
        Ok(events)
    }

    fn confirm(&mut self, request_id: &str, accept: bool) -> Vec<AppEvent> {
        self.pending = None;
        let (ok, summary, reply) = if accept {
            (true, "已写入 download.concurrency = 16", "改好了。")
        } else {
            (false, "用户拒绝，未写入", "那就不改了。")
        };
        vec![
            agent_event(AgentEvent::ToolCallFinished {
                name: "set_config".to_string(),
                ok,
                summary: summary.to_string(),
            }),
            agent_event(AgentEvent::ConfirmResolved {
                id: request_id.to_string(),
            }),
            agent_event(AgentEvent::MessageAppended(ChatMessage {
                role: ChatRole::Assistant,
                text: reply.to_string(),
                tool: None,
                streaming: false,
            })),
        ]
    }
}

/// 包一层 [`AppEvent::Agent`]（presenter 的事件队列只认顶层事件）。
fn agent_event(event: AgentEvent) -> AppEvent {
    AppEvent::Agent(event)
}
