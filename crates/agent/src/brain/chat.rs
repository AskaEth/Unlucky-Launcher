//! 对话协议类型 + [`ChatProvider`] trait（四态端点配置在 [`super::provider`]）。
//!
//! 这里**没有协议方言**：OpenAI/Ollama/Anthropic 的报文差异全部由实现方
//! （阶段 B 的 [`super::rig`]）吃掉。本模块只定义循环需要的三样东西：
//! 消息、请求/回复、流式增量回调。
//!
//! 一次工具调用的完整往返：
//! `Assistant { tool_calls }` → 编排层执行 → `Tool { call_id, name, content }` → 再次补全。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::provider::{ProviderConfig, ProviderError};
use crate::event::{AgentEvent, AgentEventSink};
use crate::registry::ToolDefinition;

/// 一条工具调用请求（模型产出）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// 调用 id（OpenAI 方言的 `tool_call_id`；Ollama 原生没有 id，由适配层生成）。
    pub id: String,
    /// 工具名。
    pub name: String,
    /// 参数（JSON 对象）。
    pub arguments: Value,
}

/// 对话消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ChatMessage {
    /// 系统提示词（每轮由 [`super::BrainConfig::system_prompt`] 重新注入，不进历史）。
    System {
        /// 提示词正文。
        content: String,
    },
    /// 用户输入。
    User {
        /// 正文。
        content: String,
    },
    /// 模型回复（可能带工具调用）。
    Assistant {
        /// 文本正文（工具调用轮可能为空）。
        text: String,
        /// 工具调用。
        tool_calls: Vec<ToolCall>,
    },
    /// 工具结果回灌。
    Tool {
        /// 对应调用 id。
        call_id: String,
        /// 工具名（Ollama 原生靠 `tool_name` 配对，不是 id）。
        name: String,
        /// 结果 JSON 字符串。
        content: String,
    },
}

impl ChatMessage {
    /// 便捷：系统消息。
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self::System {
            content: content.into(),
        }
    }

    /// 便捷：用户消息。
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            content: content.into(),
        }
    }

    /// 便捷：工具结果消息。
    #[must_use]
    pub fn tool(
        call_id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::Tool {
            call_id: call_id.into(),
            name: name.into(),
            content: content.into(),
        }
    }
}

/// 一次补全请求。
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    /// 模型名。
    pub model: String,
    /// 消息（system 在首位）。
    pub messages: Vec<ChatMessage>,
    /// 工具定义（双投影的 rig 侧载荷）。
    pub tools: Vec<ToolDefinition>,
    /// 采样温度。
    pub temperature: Option<f32>,
    /// token 上限。
    pub max_tokens: Option<u32>,
}

/// token 用量（本地模型可能不报）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// 输入 token。
    pub prompt_tokens: Option<u32>,
    /// 输出 token。
    pub completion_tokens: Option<u32>,
}

/// 一次补全结果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatReply {
    /// 文本正文。
    pub text: String,
    /// 工具调用（非空 ⇒ 循环继续）。
    pub tool_calls: Vec<ToolCall>,
    /// 用量。
    pub usage: Option<Usage>,
}

impl ChatReply {
    /// 纯文本回复。
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tool_calls: Vec::new(),
            usage: None,
        }
    }

    /// 带工具调用的回复。
    #[must_use]
    pub fn with_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            text: String::new(),
            tool_calls,
            usage: None,
        }
    }
}

/// 流式增量接收者。实现方必须**非阻塞**。
pub trait DeltaObserver: Send + Sync {
    /// 收到一段文本增量。
    fn on_delta(&self, text: &str);
}

/// 丢弃增量（不关心流式的调用方）。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopObserver;

impl DeltaObserver for NoopObserver {
    fn on_delta(&self, _text: &str) {}
}

/// 把增量转成 [`AgentEvent::TextDelta`]（P1 聊天面板逐字渲染就靠它）。
#[derive(Clone)]
pub struct EventDeltaObserver {
    sink: Arc<dyn AgentEventSink>,
}

impl EventDeltaObserver {
    /// 用事件流构造。
    #[must_use]
    pub fn new(sink: Arc<dyn AgentEventSink>) -> Self {
        Self { sink }
    }
}

impl DeltaObserver for EventDeltaObserver {
    fn on_delta(&self, text: &str) {
        self.sink.emit(AgentEvent::TextDelta {
            text: text.to_string(),
        });
    }
}

/// 收集增量（测试断言流式拼接）。
#[derive(Debug, Default)]
pub struct CollectingObserver {
    chunks: std::sync::Mutex<Vec<String>>,
}

impl CollectingObserver {
    /// 收完的内容。
    #[must_use]
    pub fn collected(&self) -> String {
        self.chunks.lock().map(|c| c.concat()).unwrap_or_default()
    }
}

impl DeltaObserver for CollectingObserver {
    fn on_delta(&self, text: &str) {
        if let Ok(mut chunks) = self.chunks.lock() {
            chunks.push(text.to_string());
        }
    }
}

/// 补全未来的箱子类型。
pub type ProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ChatReply, ProviderError>> + Send + 'a>>;

/// LLM 端点抽象：**这是 agent 唯一的异步边界**。
///
/// P0 实现 = 测试里的 mock；阶段 B 实现 = [`super::rig`] 的 rig 适配器。
/// 换实现（genai / 自写循环）只需要满足本 trait，[`super::Brain`] 不动。
pub trait ChatProvider: Send + Sync {
    /// 当前配置（循环用它取 model/temperature，也用它判断是否启用）。
    fn config(&self) -> &ProviderConfig;
    /// 一次补全。流式增量通过 `observer` 回吐。
    fn complete<'a>(
        &'a self,
        request: &'a ChatRequest,
        observer: &'a dyn DeltaObserver,
    ) -> ProviderFuture<'a>;
}

/// 装配期用的 provider 工厂：按配置造实现（阶段 B 在这里接 rig，测试接 mock）。
pub type ProviderFactory =
    Arc<dyn Fn(&ProviderConfig) -> Result<Arc<dyn ChatProvider>, ProviderError> + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MemoryEventSink;

    #[test]
    fn collecting_observer_concatenates_chunks() {
        let observer = CollectingObserver::default();
        observer.on_delta("正在");
        observer.on_delta("读取日志");
        assert_eq!(observer.collected(), "正在读取日志");
    }

    #[test]
    fn event_observer_emits_text_delta() {
        let sink = Arc::new(MemoryEventSink::new());
        let observer = EventDeltaObserver::new(sink.clone());
        observer.on_delta("嗨");
        assert_eq!(sink.streamed_text(), "嗨");
    }

    #[test]
    fn message_helpers_round_trip_through_json() {
        let messages = vec![
            ChatMessage::system("你是助手"),
            ChatMessage::user("看看日志"),
            ChatMessage::tool("call_1", "read_game_log", "{}"),
        ];
        let json = serde_json::to_string(&messages).expect("可序列化");
        let back: Vec<ChatMessage> = serde_json::from_str(&json).expect("可反序列化");
        assert_eq!(back, messages);
        assert!(json.contains("\"role\":\"tool\""));
    }
}
