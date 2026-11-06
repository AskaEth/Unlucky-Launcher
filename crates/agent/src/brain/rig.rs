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
use rig_core::error::ProviderError as RigError;
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
use super::provider::{ProviderConfig, ProviderError, ProviderKind};

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

/// rig 错误 → 本方错误（文案面向用户）。
fn map_error(error: RigError) -> ProviderError {
    ProviderError::new(error.to_string())
}

/// 工具名 → rig 的 [`RigToolName`]（空名是唯一的非法输入）。
fn tool_name(name: &str) -> Result<RigToolName, ProviderError> {
    RigToolName::new(name)
        .map_err(|_| ProviderError::new("工具名不能为空（ToolSpec 注册阶段应已拦截）"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(id: &str, name: &str) -> super::super::chat::ToolCall {
        super::super::chat::ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: serde_json::json!({ "path": "appearance.theme_mode" }),
        }
    }

    #[test]
    fn disabled_config_is_rejected_before_touching_the_network() {
        let Err(err) = RigProvider::new(ProviderConfig::disabled()) else {
            panic!("禁用态应在构造阶段被拒");
        };
        assert!(err.to_string().contains("禁用"));
    }

    #[test]
    fn anthropic_without_key_is_rejected() {
        assert!(RigProvider::new(ProviderConfig::anthropic("claude-sonnet-4")).is_err());
        assert!(RigProvider::new(
            ProviderConfig::anthropic("claude-sonnet-4").with_api_key("sk-ant")
        )
        .is_ok());
    }

    #[test]
    fn local_states_build_offline() {
        assert!(RigProvider::new(ProviderConfig::llamacpp_local("qwen2.5-7b")).is_ok());
        assert!(RigProvider::new(ProviderConfig::ollama("llama3.1")).is_ok());
    }

    #[test]
    fn request_maps_messages_and_tools() {
        let request = ChatRequest {
            model: "mock".to_string(),
            messages: vec![
                ChatMessage::system("你是助手"),
                ChatMessage::user("把主题换成浅色"),
                ChatMessage::Assistant {
                    text: String::new(),
                    tool_calls: vec![tool("call_1", "set_config")],
                },
                ChatMessage::tool("call_1", "set_config", "{\"status\":\"written\"}"),
            ],
            tools: vec![crate::registry::ToolDefinition {
                name: "set_config".to_string(),
                description: "改配置".to_string(),
                input_schema: serde_json::json!({ "type": "object" }),
            }],
            temperature: Some(0.5),
            max_tokens: Some(512),
        };
        let built = build_request(&request).expect("应能构造");
        assert_eq!(built.chat_history.len(), 4);
        assert_eq!(built.tools.len(), 1);
        assert_eq!(built.tools[0].name.as_str(), "set_config");
        assert_eq!(built.temperature, Some(0.5));
        assert_eq!(built.max_tokens, Some(512));
        // 工具结果在 rig 0.45 里是 user 角色的 ToolResult 内容块
        let json = serde_json::to_string(&built.chat_history[3]).expect("可序列化");
        assert!(json.contains("toolresult"), "{json}");
    }

    #[test]
    fn empty_history_is_a_readable_error() {
        let request = ChatRequest {
            model: "m".to_string(),
            messages: Vec::new(),
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
        };
        let err = build_request(&request).unwrap_err();
        assert!(err.to_string().contains("对话历史为空"));
    }
}
