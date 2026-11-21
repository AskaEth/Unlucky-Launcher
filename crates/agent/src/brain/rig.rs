//! rig 适配层：[`ChatProvider`] 的**唯一**实现，也是整个 crate 唯一允许 `use rig_*` 的文件。
//!
//! 依赖面收窄纪律（`DevDocs/PlanDoc/02-ai-agent-design.md` §0 + research/02 §7 R1）：
//! rig 是 pre-1.0、12 周 5 个 minor、CHANGELOG 满是 `[**breaking**]`（0.43 把 MCP 拆出 rig-agent、
//! 0.45 升 rmcp 3.5 并改 owned connection）。所以把它关在这一个文件里：它变了，只有这里编译失败，
//! [`super::ChatProvider`] 以上的循环/编排/审计一行不用动。
//!
//! ## 0.45 的接线（实读源码，非照文档猜）
//!
//! ```text
//! ProviderConfig ─┬─ OpenAiCompatible → OpenAIConfig::new(key).with_base_url(url).client().chat(model)
//!                 ├─ Anthropic        → AnthropicConfig::new(key).with_base_url(url).client().completion(model)
//!                 ├─ Ollama           → OllamaConfig::new().with_base_url(url).client().completion(model)
//!                 └─ Disabled         → 不构造（Brain::run_turn 已提前返回）
//!                        ↓ Model<W>.erase()
//!                 DynModel<Completion>（wire + transport 擦除，本文件只持有这一个类型）
//! ```
//!
//! 循环侧只要两件事：流式文本增量、以及折好的 [`ChatReply`]（文本 + 工具调用）。
//! 所以这里用 `model.stream(request)` 边转发 [`StreamEvent::Text`] 增量给 [`DeltaObserver`]，
//! 最后 `Streamed::finish()` 拿到完整 [`CompletionResponse`]——工具调用从响应里取，
//! 不自己拼分片 JSON（拼分片是 rig 的 `ReplyFold` 已经做过的事）。
//!
//! ## 三个已确认的坑
//!
//! 1. **llama.cpp 默认端点就是 `http://localhost:8080/v1`**（`providers/llamacpp/mod.rs`），
//!    与 [`LLAMACPP_DEFAULT_BASE_URL`] 一致。这里统一走 OpenAI 方言 + 显式 base_url：
//!    llama.cpp / LM Studio / DeepSeek 都是 OpenAI 兼容面，方言差异主要在默认端点与特有位
//!    （`chat_template_kwargs`、`num_ctx`）——真要下发这些，再换成 `providers::llamacpp` 方言或
//!    Ollama 的 `native_completion`。
//! 2. **Ollama 原生方言的工具结果用 `tool_name` 配对（没有 id）**，OpenAI 方言用 `tool_call_id`。
//!    这里走 rig 的 `Ollama::completion`（OpenAI 兼容 `/v1/chat/completions`），由 rig 负责回灌形状；
//!    要下发 `num_ctx`（崩溃日志几千行，上下文必须可配）才需要 `native_completion`，留作 P1。
//! 3. **取消**：`Streamed` 是普通 future/stream，丢弃即断开——P0 用「丢弃 future」实现停止生成，
//!    P1 接 UI 的「停止」按钮时按 rig 的取消语义（drop stream）对齐即可，不需要 unsafe。
//!
//! ## 逃生口（保留但不是 Plan A）
//!
//! rig 的破坏变更真伤到（或 Rust 1.92 MSRV 追不上 rig 的 1.97.1）时：换 `genai`
//! （provider + function calling，无 agent 循环）或自己写 OpenAI 方言（约 300~500 行）。
//! **代价只是实现一个新的 [`ChatProvider`]**——这正是把它关在一个文件里的意义。
//!
//! [`LLAMACPP_DEFAULT_BASE_URL`]: super::provider::LLAMACPP_DEFAULT_BASE_URL
//! [`StreamEvent::Text`]: rig_core::streaming::StreamEvent::Text

use std::sync::Arc;

use futures_util::StreamExt;
use rig_core::completion::{CompletionRequest, ToolDefinition as RigToolDefinition};
use rig_core::error::{ErrorKind, ProviderError as RigError};
use rig_core::message::{
    AssistantContent as RigAssistantContent, AssistantMessage as RigAssistantMessage, CallId,
    Message as RigMessage, Text as RigText, ToolCall as RigToolCall, ToolFunction,
    ToolName as RigToolName, ToolResult, ToolResultContent, UserContent,
};
use rig_core::operation::Completion;
use rig_core::providers::anthropic::AnthropicConfig;
use rig_core::providers::ollama::OllamaConfig;
use rig_core::providers::openai::OpenAIConfig;
use rig_core::streaming::{Item, StreamEvent};
use rig_core::DynModel;

use super::chat::{
    ChatMessage, ChatProvider, ChatReply, ChatRequest, DeltaObserver, ProviderFactory,
    ProviderFuture,
};
use super::provider::{ProviderConfig, ProviderError, ProviderFailure, ProviderKind};

/// 锁定的 rig 版本（与 `Cargo.toml` 的唯一记录点，避免两处不一致）。
pub const RIG_VERSION_PIN: &str = "=0.45.0";

/// 擦掉 wire / transport 之后的补全模型——本文件对外只暴露这一个 rig 类型之外的东西。
type ErasedModel = DynModel<Completion>;

/// rig 实现的 [`ChatProvider`]：四态配置 + 一个擦除后的模型。
pub struct RigProvider {
    config: ProviderConfig,
    model: ErasedModel,
}

impl RigProvider {
    /// 按配置造 provider。`Disabled`、缺模型名、缺 key 都在这里被拒（fail-fast）。
    ///
    /// # Errors
    /// AI 被禁用、配置不完整、或 rig 端点构造失败时返回可读错误。
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        config.validate()?;
        let model = build_model(&config)?;
        Ok(Self { config, model })
    }

    /// 装配层用的工厂：`ProviderFactory` 直接塞给 `crates/app`。
    #[must_use]
    pub fn factory() -> ProviderFactory {
        Arc::new(|config: &ProviderConfig| {
            Ok(Arc::new(Self::new(config.clone())?) as Arc<dyn ChatProvider>)
        })
    }
}

impl ChatProvider for RigProvider {
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn complete<'a>(
        &'a self,
        request: &'a ChatRequest,
        observer: &'a dyn DeltaObserver,
    ) -> ProviderFuture<'a> {
        Box::pin(async move {
            let rig_request = build_request(request)?;
            let mut stream = self.model.stream(rig_request).map_err(map_error)?;

            // 先转发增量，再拿折好的完整响应（工具调用只从响应取，不拼分片）。
            let mut text = String::new();
            while let Some(item) = stream.next().await {
                match item.map_err(map_error)? {
                    Item::Event(StreamEvent::Text { text: fragment, .. }) => {
                        text.push_str(&fragment);
                        observer.on_delta(&fragment);
                    }
                    Item::Event(_) | Item::Unknown(_) => {}
                }
            }
            let response = stream.finish().await.map_err(map_error)?;
            to_reply(&response, text)
        })
    }
}

/// 四态 → rig 模型。
fn build_model(config: &ProviderConfig) -> Result<ErasedModel, ProviderError> {
    let model = config.model.trim().to_string();
    let base_url = config.resolved_base_url().to_string();
    let api_key = config.api_key.clone().unwrap_or_default();
    let erased = match config.kind {
        ProviderKind::OpenAiCompatible => OpenAIConfig::new(api_key)
            .with_base_url(base_url)
            .client()
            .chat(model)
            .erase(),
        ProviderKind::Anthropic => AnthropicConfig::new(api_key)
            .with_base_url(base_url)
            .client()
            .completion(model)
            .erase(),
        ProviderKind::Ollama => OllamaConfig::new()
            .with_base_url(base_url)
            .client()
            .completion(model)
            .erase(),
        ProviderKind::Disabled => {
            return Err(ProviderError::new(
                "AI 已禁用：请在设置 → AI 中选择端点（规则引擎诊断不受影响）",
            ))
        }
    };
    Ok(erased)
}

/// 本方消息 → rig 补全请求。
fn build_request(request: &ChatRequest) -> Result<CompletionRequest, ProviderError> {
    let mut history = to_rig_messages(&request.messages)?;
    // rig 把历史最后一条当 prompt，其余是上下文；空历史没有可发的 prompt。
    let prompt = history
        .pop()
        .ok_or_else(|| ProviderError::new("对话历史为空：至少需要一条用户消息"))?;
    let prompt = Arc::unwrap_or_clone(prompt);
    let tools = request
        .tools
        .iter()
        .map(to_rig_tool)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(CompletionRequest::new(prompt)
        .messages(history)
        .tools(tools)
        .temperature(request.temperature.map(f64::from))
        .max_tokens(request.max_tokens.map(u64::from)))
}

/// 本方工具定义 → rig 工具定义（入参都是裸 JSON Schema，这正是双投影的支点）。
fn to_rig_tool(tool: &crate::registry::ToolDefinition) -> Result<RigToolDefinition, ProviderError> {
    let name = tool_name(&tool.name)?;
    Ok(RigToolDefinition::new(
        name,
        tool.description.clone(),
        tool.input_schema.clone(),
    ))
}

/// 本方消息 → rig 消息。
fn to_rig_messages(messages: &[ChatMessage]) -> Result<Vec<Arc<RigMessage>>, ProviderError> {
    let mut out = Vec::with_capacity(messages.len());
    for message in messages {
        let rig = match message {
            ChatMessage::System { content } => RigMessage::system(content.clone()),
            ChatMessage::User { content } => RigMessage::user(content.clone()),
            ChatMessage::Assistant { text, tool_calls } => {
                let mut parts = Vec::with_capacity(tool_calls.len() + 1);
                if !text.trim().is_empty() {
                    parts.push(RigAssistantContent::Text(RigText::new(text.clone())));
                }
                for call in tool_calls {
                    parts.push(RigAssistantContent::ToolCall(RigToolCall::new(
                        CallId::from_wire(call.id.clone()),
                        ToolFunction::new(tool_name(&call.name)?, call.arguments.clone()),
                    )));
                }
                RigMessage::Assistant(RigAssistantMessage::new(parts))
            }
            ChatMessage::Tool {
                call_id,
                name,
                content,
            } => RigMessage::User {
                content: vec![UserContent::ToolResult(ToolResult {
                    call: CallId::from_wire(call_id.clone()),
                    name: tool_name(name)?,
                    content: vec![ToolResultContent::text(content.clone())],
                    is_error: false,
                })],
            },
        };
        out.push(Arc::new(rig));
    }
    Ok(out)
}

/// rig 响应 → 本方回复（文本 + 工具调用 + 用量）。
fn to_reply(
    response: &rig_core::completion::CompletionResponse,
    streamed: String,
) -> Result<ChatReply, ProviderError> {
    let mut text = streamed;
    let mut tool_calls = Vec::new();
    for part in &response.choice {
        match part {
            RigAssistantContent::Text(chunk) => {
                if text.is_empty() {
                    text.push_str(&chunk.text);
                }
            }
            RigAssistantContent::ToolCall(call) => {
                if let Some(raw) = call.function.invalid_arguments.as_deref() {
                    return Err(ProviderError::new(format!(
                        "模型返回的工具调用参数不是 JSON 对象（工具 `{}`）：{raw}",
                        call.function.name.as_str()
                    )));
                }
                tool_calls.push(super::chat::ToolCall {
                    id: call.id.to_string(),
                    name: call.function.name.as_str().to_string(),
                    arguments: serde_json::Value::Object(call.function.arguments.clone()),
                });
            }
            // 推理链/图片/未知块不进对话循环（P0 只消费文本 + 工具调用）。
            _ => {}
        }
    }
    Ok(ChatReply {
        text,
        tool_calls,
        usage: Some(super::chat::Usage {
            prompt_tokens: response
                .usage
                .input_tokens
                .and_then(|v| u32::try_from(v).ok()),
            completion_tokens: response
                .usage
                .output_tokens
                .and_then(|v| u32::try_from(v).ok()),
        }),
    })
}

/// rig 错误 → 本方错误：**分类取自 rig 的结构化报告**（`ErrorReport` 的 `kind` /
/// `http_status` / `code`），不猜 `Display` 文本（M174 r1 复审 P2-5）。
///
/// | rig 侧信号 | 本方分类 |
/// |---|---|
/// | `http_status ∈ {401, 403}` | [`ProviderFailure::Auth`] |
/// | `http_status == 404` | [`ProviderFailure::ModelNotFound`]（OpenAI 兼容面的 404 只可能是模型不在，`/chat/completions` 本身是固定路径；Anthropic 的 404 也是同一语义） |
/// | `code == "model_not_found"` / `"model_not_found_error"` | [`ProviderFailure::ModelNotFound`] |
/// | `ErrorKind::Timeout` 或传输层 `io::ErrorKind::TimedOut` | [`ProviderFailure::Timeout`] |
/// | `ErrorKind::Http`（连接被拒 / DNS / TLS / 截断） | [`ProviderFailure::Unreachable`] |
/// | 其余（JSON / URL / 请求构造 / 5xx / 未知） | [`ProviderFailure::Other`] |
fn map_error(error: RigError) -> ProviderError {
    let report = error.report();
    let kind = match report.kind {
        ErrorKind::Timeout => ProviderFailure::Timeout,
        ErrorKind::Http => {
            if transport_timed_out(&error) {
                ProviderFailure::Timeout
            } else {
                ProviderFailure::Unreachable
            }
        }
        ErrorKind::ProviderResponse | ErrorKind::Provider => match report.http_status {
            Some(401 | 403) => ProviderFailure::Auth,
            Some(404) => ProviderFailure::ModelNotFound,
            _ if matches!(
                report.code.as_deref(),
                Some("model_not_found" | "model_not_found_error")
            ) =>
            {
                ProviderFailure::ModelNotFound
            }
            _ => ProviderFailure::Other,
        },
        _ => ProviderFailure::Other,
    };
    ProviderError::classified(kind, report.message)
}

/// 传输层失败里有没有「超时」：沿 `source()` 链找 `io::ErrorKind::TimedOut`
/// （reqwest 把超时折在 transport error 里，rig 只留一个 `Box<dyn Error>`）。
///
/// 结构化判据（类型 + `ErrorKind`），不是文本匹配：这条路径上的其它失败（连接被拒、DNS、
/// TLS）都归 `Unreachable`。
fn transport_timed_out(error: &RigError) -> bool {
    let RigError::Http(http_error) = error else {
        return false;
    };
    let mut source = std::error::Error::source(http_error);
    for _ in 0..8 {
        let Some(cause) = source else { return false };
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            return io.kind() == std::io::ErrorKind::TimedOut;
        }
        source = cause.source();
    }
    false
}

/// 工具名 → rig 的 [`RigToolName`]（空名是唯一的非法输入）。
fn tool_name(name: &str) -> Result<RigToolName, ProviderError> {
    RigToolName::new(name)
        .map_err(|_| ProviderError::new("工具名不能为空（ToolSpec 注册阶段应已拦截）"))
}

#[cfg(test)]
#[path = "rig/tests.rs"]
mod tests;
