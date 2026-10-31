//! Agent 大脑：Provider 抽象 + 对话循环 + 流式事件。
//!
//! 选型（M10 定稿）：`rig-core` + `rig-agent`，**版本锁 `=0.45.x`**——上游 12 周 5 个 minor
//! 的 breaking 频率决定了必须锁死；依赖面只在本文件收口，其它模块不许直接碰 rig，
//! 以便随时换 genai / 自写循环逃生（`DevDocs/PlanDoc/02-ai-agent-design.md` §0）。
//!
//! Provider 形态（02 §2）：OpenAI 兼容（默认，含本地 llama.cpp `127.0.0.1:PORT/v1`）、
//! Anthropic 原生、Ollama 原生、以及「禁用」——禁用时规则引擎仍可用。
//!
//! 统一 trait（M16 落地）：`stream_chat(messages, tools) -> Stream<Event>`，含 tool_call 事件。
//! 本 crate 的落地形态是 [`chat::ChatProvider::complete`]（`request + observer → ChatReply`）：
//! rig 0.45 的流在 [`rig`] 适配层里展开成增量回调，循环侧只消费「文本增量 + 工具调用」两件事。
//! **rig 依赖面收口在 [`rig`] 一个文件**，本文件不 `use rig_*`。
//!
//! ## 对话循环（02 文档 §3.3）
//!
//! ```text
//! run_turn(user_input)
//!   ├─ provider.complete(request)  ← 流式增量经 EventDeltaObserver → UI
//!   ├─ 有 tool_calls → ToolExecutor.execute（校验/确认/YOLO/审计）→ Tool 消息回灌
//!   └─ 没有 tool_calls → 结束，返回最终文本
//! ```
//!
//! 三条不变量：
//! 1. **system prompt 每轮重新注入**，不进历史（改提示词立刻生效，也不会被压缩逻辑吃掉）；
//! 2. 工具结果**永远回灌**（包括失败），模型才有机会改正；
//! 3. 步数上限 [`DEFAULT_MAX_STEPS`] 兜底，防止模型在两个工具间打转烧 token。

pub mod chat;
pub mod exec;
pub mod provider;
pub mod rig;

use std::fmt;
use std::sync::Arc;

use crate::audit::AuditSink;
use crate::clock::SharedClock;
use crate::confirm::{ConfirmGate, ConfirmPolicy};
use crate::event::{AgentEvent, AgentEventSink};
use crate::registry::ToolRegistry;

pub use chat::{
    ChatMessage, ChatProvider, ChatReply, ChatRequest, DeltaObserver, EventDeltaObserver,
    NoopObserver, ProviderFactory, ProviderFuture, ToolCall, Usage,
};
pub use exec::{ToolExecOutcome, ToolExecutor};
pub use provider::{ProviderConfig, ProviderError, ProviderKind};

/// 一轮对话最多几轮工具调用。
pub const DEFAULT_MAX_STEPS: usize = 8;

/// 内置系统提示词（P0 版本；P2 会按实例上下文按需注入）。
#[must_use]
pub fn default_system_prompt() -> String {
    [
        "你是 Unlucky-Launcher（ULL，Minecraft 启动器）的内置助手。",
        "你能调用工具：读配置（get_config）、改配置（set_config）、列出游戏会话（list_game_sessions）、",
        "读会话日志（read_game_log）、跑崩溃规则引擎（diagnose_crash）。",
        "纪律：",
        "1. 先查后改：改任何配置前先用 get_config 确认键名与现值。",
        "2. 崩溃问题先看 diagnose_crash 的规则结论；规则未命中或需要更多信息时，才用 read_game_log",
        "   取 stderr / exit / crash_found，不要拉全量日志。",
        "3. 只能改 get_config 列出的可写键。敏感键会要求用户确认：被拒绝就换方案，不要重复尝试同一改动。",
        "4. 用中文回答，给用户可以立即执行的下一步；不确定时直说不确定，不要编造规则或日志内容。",
    ]
    .join("\n")
}

/// 循环参数。
#[derive(Debug, Clone)]
pub struct BrainConfig {
    /// 最多几轮工具调用。
    pub max_steps: usize,
    /// 系统提示词。
    pub system_prompt: String,
}

impl Default for BrainConfig {
    fn default() -> Self {
        Self {
            max_steps: DEFAULT_MAX_STEPS,
            system_prompt: default_system_prompt(),
        }
    }
}

impl BrainConfig {
    /// 自定义提示词（步数用默认）。
    #[must_use]
    pub fn with_system_prompt(system_prompt: impl Into<String>) -> Self {
        Self {
            max_steps: DEFAULT_MAX_STEPS,
            system_prompt: system_prompt.into(),
        }
    }
}

/// 循环的旁路依赖（审计/确认/时钟/事件）。
pub struct BrainDeps {
    /// 审计出口。
    pub audit: Arc<dyn AuditSink>,
    /// 确认闸门（P0 = [`crate::confirm::NoUiGate`]）。
    pub gate: Arc<dyn ConfirmGate>,
    /// YOLO 等策略快照。
    pub policy: ConfirmPolicy,
    /// 时间源。
    pub clock: SharedClock,
    /// 事件出口。
    pub sink: Arc<dyn AgentEventSink>,
}

impl BrainDeps {
    /// 装配。
    pub fn new(
        audit: Arc<dyn AuditSink>,
        gate: Arc<dyn ConfirmGate>,
        policy: ConfirmPolicy,
        clock: SharedClock,
        sink: Arc<dyn AgentEventSink>,
    ) -> Self {
        Self {
            audit,
            gate,
            policy,
            clock,
            sink,
        }
    }
}

/// 一轮对话的结局。
#[derive(Debug, Clone, PartialEq)]
pub struct TurnOutcome {
    /// 最终文本（可能是空的，例如撞上步数上限且模型最后一句只有工具调用）。
    pub text: String,
    /// 用掉的轮次。
    pub steps: usize,
    /// 工具调用总次数。
    pub tool_calls: usize,
    /// 为什么停下来。
    pub stopped_reason: StopReason,
}

/// 停止原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// 模型给出了最终回答。
    Completed,
    /// 撞上 [`BrainConfig::max_steps`]。
    StepLimit,
}

/// 循环级错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrainError {
    /// AI 被禁用（规则引擎仍可用）。
    Disabled,
    /// provider 报错。
    Provider(ProviderError),
}

impl fmt::Display for BrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => f.write_str(DISABLED_MESSAGE),
            Self::Provider(err) => write!(f, "LLM 调用失败：{err}"),
        }
    }
}

impl std::error::Error for BrainError {}

/// AI 被禁用时给用户的话（装配层直接展示，不用先构造 Brain）。
pub const DISABLED_MESSAGE: &str =
    "AI 已禁用：请在设置 → AI 中配置端点。规则引擎崩溃诊断不受影响。";

/// 对话循环。
pub struct Brain {
    provider: Arc<dyn ChatProvider>,
    registry: Arc<ToolRegistry>,
    executor: ToolExecutor,
    config: BrainConfig,
    sink: Arc<dyn AgentEventSink>,
}

impl Brain {
    /// 装配。
    pub fn new(
        provider: Arc<dyn ChatProvider>,
        registry: Arc<ToolRegistry>,
        deps: BrainDeps,
        config: BrainConfig,
    ) -> Self {
        let executor = ToolExecutor::new(
            Arc::clone(&registry),
            Arc::clone(&deps.audit),
            Arc::clone(&deps.gate),
            deps.policy,
            Arc::clone(&deps.clock),
            Arc::clone(&deps.sink),
        );
        Self {
            provider,
            registry,
            executor,
            config,
            sink: deps.sink,
        }
    }

    /// 当前 provider 配置（设置页/UI 展示用）。
    #[must_use]
    pub fn provider_config(&self) -> &ProviderConfig {
        self.provider.config()
    }

    /// 跑一轮对话。`history` 由调用方持有（P1 落盘为 `agent/sessions/*.jsonl`）。
    ///
    /// # Errors
    /// AI 被禁用（[`BrainError::Disabled`]）或 provider 报错（[`BrainError::Provider`]）。
    pub async fn run_turn(
        &self,
        agent_session_id: &str,
        history: &mut Vec<ChatMessage>,
        user_input: &str,
    ) -> Result<TurnOutcome, BrainError> {
        if !self.provider.config().is_enabled() {
            self.sink.emit(AgentEvent::Error {
                message: DISABLED_MESSAGE.to_string(),
            });
            return Err(BrainError::Disabled);
        }

        self.sink.emit(AgentEvent::TurnStarted {
            session_id: agent_session_id.to_string(),
            user_text: user_input.to_string(),
        });
        history.push(ChatMessage::user(user_input));

        let observer = EventDeltaObserver::new(Arc::clone(&self.sink));
        let mut tool_calls = 0usize;
        let mut last_text = String::new();
        let mut used_steps = 0usize;

        for step in 1..=self.config.max_steps {
            used_steps = step;
            let request = self.build_request(history);
            let reply = match self.provider.complete(&request, &observer).await {
                Ok(reply) => reply,
                Err(err) => {
                    self.sink.emit(AgentEvent::Error {
                        message: err.to_string(),
                    });
                    return Err(BrainError::Provider(err));
                }
            };

            if !reply.text.trim().is_empty() {
                last_text = reply.text.clone();
                self.sink.emit(AgentEvent::AssistantText {
                    text: reply.text.clone(),
                });
            }

            if reply.tool_calls.is_empty() {
                history.push(ChatMessage::Assistant {
                    text: reply.text.clone(),
                    tool_calls: Vec::new(),
                });
                self.sink.emit(AgentEvent::TurnFinished {
                    steps: step,
                    text: reply.text.clone(),
                });
                return Ok(TurnOutcome {
                    text: reply.text,
                    steps: step,
                    tool_calls,
                    stopped_reason: StopReason::Completed,
                });
            }

            history.push(ChatMessage::Assistant {
                text: reply.text.clone(),
                tool_calls: reply.tool_calls.clone(),
            });
            for call in &reply.tool_calls {
                let outcome = self.executor.execute(agent_session_id, call, step).await;
                tool_calls += 1;
                history.push(ChatMessage::tool(
                    call.id.clone(),
                    call.name.clone(),
                    outcome.to_model_content(),
                ));
            }
        }

        self.sink.emit(AgentEvent::TurnFinished {
            steps: used_steps,
            text: last_text.clone(),
        });
        Ok(TurnOutcome {
            text: last_text,
            steps: used_steps,
            tool_calls,
            stopped_reason: StopReason::StepLimit,
        })
    }

    fn build_request(&self, history: &[ChatMessage]) -> ChatRequest {
        let config = self.provider.config();
        let mut messages = Vec::with_capacity(history.len() + 1);
        messages.push(ChatMessage::system(self.config.system_prompt.clone()));
        messages.extend_from_slice(history);
        ChatRequest {
            model: config.model.clone(),
            messages,
            tools: self.registry.definitions(),
            temperature: config.temperature,
            max_tokens: config.max_tokens,
        }
    }
}
