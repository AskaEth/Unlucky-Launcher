//! [`AgentTransport`] 的 iced 侧实现：脚本化 rig + 三类失败注入（M119）。
//!
//! ## 它是什么、不是什么
//!
//! 它是 M103 契约 [`AgentTransport`] 的一个**实现**，不是契约的改写：`submit` / `confirm` 的
//! 签名一行没动。真实的 rig 后端（`crates/agent` + rig）还没装配进 host（M103 报告 §4 登记过），
//! 所以本轮用它跑端到端。
//!
//! **事件走队列，不当场返回**：`submit` 把整批事件 push 进 [`EventQueue`] 后返回 `Ok(vec![])`，
//! UI 在心跳里抽。这条形状才是异步后端落地时**不会变**的那部分——换掉的只有「谁往队列里 push」
//! （真实后端在后台线程里逐帧推流）。抓得住：事件经 Message 通道回流的整条链路；
//! 抓不住：跨线程时序、乱序、并发提交（本轮 transport 是同步的，见报告「没验的部分」）。
//!
//! ## 失败注入（任务「失败态可见」的那两条 AI 侧）
//!
//! | `ULL_AGENT_SCRIPT` | `submit` 行为 |
//! |---|---|
//! | `qa`（默认） | 一轮正常问答：用户气泡 + 流式助手消息（三条增量）+ 收敛 |
//! | `streaming` | 同上但**永不收敛**（`busy` 一直亮着：中断键与禁用态的可视图） |
//! | `fails` | 流到一半断流：已到的增量保留，整轮以 `Failed` 收尾 |
//! | `write` | 需确认的写操作（High 级 `ConfirmRequested`） |
//! | `unreachable` | **端点不可达**：`Err`，一个事件也不产生 |
//! | `unimplemented` | **未实现路由**：`Err`，同上 |
//!
//! 两种 `Err` 都先把**具名原因**写进 [`FailureSlot`] 再返回错误串——UI 据原因码选 catalog 文案，
//! 错误串本身（后端诊断）只进日志，不上屏（M114 立的口径）。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use unlucky_presenter::agent::{AgentRail, AgentTransport};
use unlucky_presenter::dto::{
    AgentEvent, AppEvent, ChatMessage, ChatRole, ConfirmRequest, Sensitivity,
};

use super::AgentFailure;

/// 事件队列（transport 侧 push、UI 侧 drain）。
pub(crate) type EventQueue = Arc<Mutex<VecDeque<AppEvent>>>;

/// 失败原因槽（transport 侧写、UI 侧读后清）。
pub(crate) type FailureSlot = Arc<Mutex<Option<AgentFailure>>>;

/// 脚本（一轮输入会产生哪种剧情）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Script {
    /// 正常问答。
    Qa,
    /// 流式永不收敛。
    Streaming,
    /// 流式中途断流。
    FailsMidStream,
    /// 需确认的写操作。
    WriteOp,
    /// 端点不可达（`Err`）。
    Unreachable,
    /// 未实现路由（`Err`）。
    NotImplemented,
}

/// 脚本环境变量（取证开关，与 `ULL_AGENT_RAIL` / `ULL_APPEARANCE_*` 同一口径）。
pub(crate) const SCRIPT_ENV: &str = "ULL_AGENT_SCRIPT";

impl Script {
    /// 全部脚本（`from_name` 的遍历表）。
    pub(crate) const ALL: [Script; 6] = [
        Script::Qa,
        Script::Streaming,
        Script::FailsMidStream,
        Script::WriteOp,
        Script::Unreachable,
        Script::NotImplemented,
    ];

    /// 稳定名字（`ULL_AGENT_SCRIPT` 的取值 / 日志）。
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Script::Qa => "qa",
            Script::Streaming => "streaming",
            Script::FailsMidStream => "fails",
            Script::WriteOp => "write",
            Script::Unreachable => "unreachable",
            Script::NotImplemented => "unimplemented",
        }
    }

    /// 反向解析（取证开关用；未知值忽略）。
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|script| script.name() == name)
    }

    /// 读 `ULL_AGENT_SCRIPT`；没设 / 解析不出来 = [`Script::Qa`]。
    pub(crate) fn from_env() -> Self {
        match std::env::var(SCRIPT_ENV) {
            Ok(raw) => match Self::from_name(raw.trim()) {
                Some(script) => script,
                None => {
                    eprintln!(
                        "[agent] {SCRIPT_ENV}={raw:?} 不是脚本之一，按 qa 处理（可选：{}）",
                        Self::ALL.map(Self::name).join("/")
                    );
                    Script::Qa
                }
            },
            Err(_) => Script::Qa,
        }
    }
}

/// 脚本化 transport：事件入队、失败入槽。
pub(crate) struct RigTransport {
    script: Script,
    /// 已发起过几轮（用来算消息下标 / 确认请求 id）。
    turns: usize,
    /// 当前待确认的写操作。
    pending: Option<ConfirmRequest>,
    queue: EventQueue,
    failure: FailureSlot,
}

impl RigTransport {
    /// 新 transport + 它与 UI 共享的两个句柄。
    pub(crate) fn new(script: Script) -> Self {
        Self {
            script,
            turns: 0,
            pending: None,
            queue: EventQueue::default(),
            failure: FailureSlot::default(),
        }
    }

    /// 与 UI 共享的事件队列（克隆一个给 [`super::AgentHost`]）。
    pub(crate) fn queue(&self) -> EventQueue {
        Arc::clone(&self.queue)
    }

    /// 与 UI 共享的失败原因槽。
    pub(crate) fn failure(&self) -> FailureSlot {
        Arc::clone(&self.failure)
    }

    /// 入队一批事件（transport 唯一的输出方向）。
    fn emit(&self, events: Vec<AppEvent>) {
        if events.is_empty() {
            return;
        }
        if let Ok(mut queue) = self.queue.lock() {
            queue.extend(events);
        }
    }

    /// 注入一次失败：原因入槽（UI 据它选 catalog 文案）、错误串回给 presenter（只进日志）。
    fn fail(&self, failure: AgentFailure, detail: &str) -> Result<Vec<AppEvent>, String> {
        if let Ok(mut slot) = self.failure.lock() {
            *slot = Some(failure);
        }
        Err(format!("{}：{detail}", failure.code()))
    }
}

impl AgentTransport for RigTransport {
    fn submit(&mut self, text: &str, history: &AgentRail) -> Result<Vec<AppEvent>, String> {
        match self.script {
            Script::Unreachable => {
                return self.fail(
                    AgentFailure::Unreachable,
                    "assistant endpoint http://127.0.0.1:8080 连不上（connection refused）",
                );
            }
            Script::NotImplemented => {
                return self.fail(
                    AgentFailure::NotImplemented,
                    "真实 LLM 路由还没装配进 host（M103 §4 的欠账）",
                );
            }
            _ => {}
        }

        self.turns += 1;
        // 后端自己算下标：用户气泡先到，助手流式占位紧随其后。
        let assistant = history.messages.len() + 1;
        let mut events = vec![
            agent_event(AgentEvent::MessageAppended(ChatMessage::user(text))),
            agent_event(AgentEvent::MessageAppended(ChatMessage::assistant_stream())),
        ];
        match self.script {
            Script::Qa | Script::Streaming => {
                for delta in ["先确认版本：", "装完再加 Fabric。"] {
                    events.push(agent_event(AgentEvent::MessageDelta {
                        index: assistant,
                        delta: delta.to_string(),
                    }));
                }
                if self.script == Script::Qa {
                    events.push(agent_event(AgentEvent::MessageFinished {
                        index: assistant,
                    }));
                }
            }
            Script::FailsMidStream => {
                events.push(agent_event(AgentEvent::MessageDelta {
                    index: assistant,
                    delta: "我先读一下最近的日志".to_string(),
                }));
                events.push(agent_event(AgentEvent::Failed {
                    message: "provider 断流：read timeout".to_string(),
                }));
            }
            Script::WriteOp => {
                events.push(agent_event(AgentEvent::MessageDelta {
                    index: assistant,
                    delta: "我准备把下载并发从 64 改成 16，改之前先问一句。".to_string(),
                }));
                events.push(agent_event(AgentEvent::MessageFinished {
                    index: assistant,
                }));
                events.push(agent_event(AgentEvent::ToolCallStarted {
                    name: "set_config".to_string(),
                    summary: "修改 download.concurrency".to_string(),
                }));
                let request = ConfirmRequest {
                    id: format!("req-{}", self.turns),
                    tool: "set_config".to_string(),
                    summary: "把下载并发 64 → 16".to_string(),
                    // 不引 serde_json（app-iced 没有这个依赖）：`Value::default()` = Null，
                    // 确认框只画 `summary`，`details` 的结构化 diff 留给后续 mission。
                    details: Default::default(),
                    sensitivity: Sensitivity::High,
                };
                self.pending = Some(request.clone());
                events.push(agent_event(AgentEvent::ConfirmRequested { request }));
            }
            // 三支失败脚本在上面的 `match` 里已经 `return`，这里只为穷尽
            Script::Unreachable | Script::NotImplemented => {}
        }
        self.emit(events);
        // 契约允许「事件分片由 presenter 顺序 apply」，也允许走事件通道——本实现走通道（见模块文档）。
        Ok(Vec::new())
    }

    fn confirm(&mut self, request_id: &str, accept: bool) -> Vec<AppEvent> {
        self.pending = None;
        let (ok, summary, reply) = if accept {
            (true, "已写入 download.concurrency = 16", "改好了。")
        } else {
            (false, "用户拒绝，未写入", "那就不改了。")
        };
        let events = vec![
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
        ];
        self.emit(events.clone());
        // 与 `submit` 同一条口径：事件走队列，这里返回空（避免同一批事件被 apply 两遍）。
        Vec::new()
    }
}

/// 包一层 [`AppEvent::Agent`]（事件队列只认顶层事件）。
fn agent_event(event: AgentEvent) -> AppEvent {
    AppEvent::Agent(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_script_name_round_trips_and_the_env_default_is_qa() {
        for script in Script::ALL {
            assert_eq!(Script::from_name(script.name()), Some(script));
        }
        assert_eq!(Script::from_name("nope"), None);
        // 环境变量在这条测试里不可靠地存在（并行测试共享进程），只验默认分支的返回值
        assert_eq!(Script::from_name("qa"), Some(Script::Qa));
    }

    #[test]
    fn a_normal_turn_queues_the_whole_batch_and_returns_no_inline_events() {
        let mut transport = RigTransport::new(Script::Qa);
        let queue = transport.queue();
        let inline = transport
            .submit("hello", &AgentRail::new())
            .expect("qa 不会失败");
        assert!(inline.is_empty(), "本实现走队列，不走进内联分片");
        assert_eq!(
            queue.lock().expect("queue").len(),
            5,
            "user + 流式占位 + 两片增量 + 收敛"
        );
    }

    #[test]
    fn the_two_failing_scripts_return_a_named_reason_and_queue_nothing() {
        for (script, expected) in [
            (Script::Unreachable, AgentFailure::Unreachable),
            (Script::NotImplemented, AgentFailure::NotImplemented),
        ] {
            let mut transport = RigTransport::new(script);
            let queue = transport.queue();
            let failure = transport.failure();
            let error = transport
                .submit("hello", &AgentRail::new())
                .expect_err("这两支必失败");
            assert!(error.contains(expected.code()), "错误串要带原因码：{error}");
            assert_eq!(*failure.lock().expect("slot"), Some(expected));
            assert!(queue.lock().expect("queue").is_empty(), "失败不产生事件");
        }
    }

    #[test]
    fn confirm_queues_the_tool_result_and_the_reply() {
        let mut transport = RigTransport::new(Script::WriteOp);
        let queue = transport.queue();
        transport
            .submit("改并发", &AgentRail::new())
            .expect("写操作");
        queue.lock().expect("queue").clear();
        let inline = transport.confirm("req-1", true);
        assert!(inline.is_empty());
        assert_eq!(
            queue.lock().expect("queue").len(),
            3,
            "工具收尾 + 确认收敛 + 回复"
        );
    }
}
