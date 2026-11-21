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
pub mod guard;
pub mod provider;
pub mod rig;
pub mod session;
pub mod turn;

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::audit::AuditSink;
use crate::clock::SharedClock;
use crate::confirm::{ConfirmGate, YoloSwitch};
use crate::event::AgentEventSink;
use crate::registry::ToolRegistry;

pub use chat::{
    ChatMessage, ChatProvider, ChatReply, ChatRequest, DeltaObserver, EventDeltaObserver,
    NoopObserver, ProviderFactory, ProviderFuture, ToolCall, Usage,
};
pub use exec::{ToolExecOutcome, ToolExecutor};
pub use guard::{CancelToken, DEFAULT_IDLE_TIMEOUT};
pub use provider::{ProviderConfig, ProviderError, ProviderFailure, ProviderKind};
pub use session::{Session, SessionConfig, SessionError};
pub use turn::TURN_AUDIT_TOOL;

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
    /// YOLO 的判据（M389：活开关，不是快照）。
    ///
    /// `new` 收 `impl Into<YoloSwitch>`（既有调用方传 `crate::confirm::ConfirmPolicy`，等价于
    /// 固定开关）；生产装配点随后用 [`BrainDeps::with_yolo_switch`] 换成现读设置文件的探针。
    pub yolo: YoloSwitch,
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
        policy: impl Into<YoloSwitch>,
        clock: SharedClock,
        sink: Arc<dyn AgentEventSink>,
    ) -> Self {
        Self {
            audit,
            gate,
            yolo: policy.into(),
            clock,
            sink,
        }
    }

    /// 换成活开关（M389）：GUI / MCP 的装配点给「现读 `ai.yolo`」的探针。
    #[must_use]
    pub fn with_yolo_switch(mut self, yolo: YoloSwitch) -> Self {
        self.yolo = yolo;
        self
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
    /// **这一轮被用户取消**（界面上的「停止」）。
    ///
    /// 它不是失败：provider 没做错任何事。调用方要做的三件事是「本轮不再追加正文 /
    /// 以一条『已取消』收尾 / 释放输入区」，而不是弹错误横幅（M388）。
    Cancelled,
    /// **provider 在时限内一句都没回**（空闲秒数见 [`guard::DEFAULT_IDLE_TIMEOUT`]）。
    ///
    /// 必须是**一条独立的原因**而不是「provider 报错」：超时要给用户一句「点发送可以再试一次」，
    /// 而 provider 的 401 / 404 要指向配置。混成一档，界面就只能说一句含糊话。
    Timeout {
        /// 生效的空闲秒数（写进结论正文）。
        secs: u64,
    },
}

impl fmt::Display for BrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => f.write_str(DISABLED_MESSAGE),
            Self::Provider(err) => write!(f, "LLM 调用失败：{err}"),
            Self::Cancelled => f.write_str("本轮已被用户取消"),
            Self::Timeout { secs } => write!(f, "请求超时：{secs} 秒没有收到任何响应"),
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
    /// 取消旗标（M388）：由持有会话的一方交给 UI，UI 按「停止」时置位。
    cancel: CancelToken,
    /// 空闲时限（每轮开头的 [`guard::resolve_timeout`] 会再叠一次环境变量，**不是**启动期快照）。
    timeout: Duration,
    /// 一轮超时/取消时写一条**轮次级**审计记录（`tool = "brain.turn"`）用的旁路。
    audit: Arc<dyn AuditSink>,
    clock: SharedClock,
    /// YOLO 的判据（与执行器同一份：轮次审计与确认闸门必须看到同一个答案）。
    yolo: YoloSwitch,
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
            deps.yolo.clone(),
            Arc::clone(&deps.clock),
            Arc::clone(&deps.sink),
        );
        Self {
            provider,
            registry,
            executor,
            config,
            sink: deps.sink,
            cancel: CancelToken::new(),
            timeout: DEFAULT_IDLE_TIMEOUT,
            audit: deps.audit,
            clock: deps.clock,
            yolo: deps.yolo,
        }
    }

    /// 接一条外部的取消旗标（M388）：UI 侧持有同一份，按「停止」时置位，本循环在判据里看见。
    ///
    /// 为什么不塞进 [`Brain::new`] 的参数表：那个签名的调用方包括 `crates/agent/tests/support`
    /// （不在本 mission 的 scope），改签名等于逼它跟着改。加一个 builder 方法对既有调用方零影响。
    #[must_use]
    pub fn with_cancel(mut self, cancel: CancelToken) -> Self {
        self.cancel = cancel;
        self
    }

    /// 换空闲时限（默认 [`DEFAULT_IDLE_TIMEOUT`]；每轮还会被 `ULL_AGENT_TIMEOUT_SECS` 覆盖）。
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 取消旗标的句柄（装配方把它交给 UI，见 [`Brain::with_cancel`]）。
    #[must_use]
    pub fn cancel_handle(&self) -> CancelToken {
        self.cancel.clone()
    }

    /// 当前 provider 配置（设置页/UI 展示用）。
    #[must_use]
    pub fn provider_config(&self) -> &ProviderConfig {
        self.provider.config()
    }
}
