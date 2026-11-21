//! agent 事件流：**UI 与审计之外的观察者**都从这里接（02 文档 §1 的「event channel」）。
//!
//! P0 只做输出（[`AgentEventSink`]），P1 的 Slint 聊天面板订阅同一路事件把
//! 「正在读取日志…」「请求修改设置：下载并发 64→16」画出来，确认弹窗直接复用
//! [`AgentEvent::ConfirmRequested`]。
//!
//! 实现方必须**非阻塞**：`emit` 在对话循环的同步路径上被调用。

use std::sync::mpsc::Sender;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;

use crate::confirm::ConfirmRequest;

/// 一条 agent 事件（全部可序列化：P1 走 IPC 给 app 时不写两套类型）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// 一轮对话开始。
    TurnStarted {
        /// agent 会话 id。
        session_id: String,
        /// 用户输入。
        user_text: String,
    },
    /// 流式文本增量（P1 聊天面板逐字渲染）。
    TextDelta {
        /// 增量文本。
        text: String,
    },
    /// 模型这一轮的非流式完整文本（流式之外的兜底，也是审计用的正文）。
    AssistantText {
        /// 文本。
        text: String,
    },
    /// 开始执行一个工具调用。
    ToolCallStarted {
        /// 工具名。
        tool: String,
        /// 模型给的参数。
        args: Value,
        /// 第几轮工具循环（从 1 开始）。
        step: usize,
    },
    /// 一个工具调用结束。
    ToolCallFinished {
        /// 工具名。
        tool: String,
        /// 是否成功。
        ok: bool,
        /// 一句话结果摘要（UI 直接显示）。
        summary: String,
        /// 这次是否触发了确认请求。
        requires_confirmation: bool,
    },
    /// 需要用户确认（P0 只发事件，P1 变成可点的弹窗）。
    ConfirmRequested {
        /// 待确认请求。
        request: Box<ConfirmRequest>,
    },
    /// 工具调用被拒绝（策略或用户）。
    ToolDenied {
        /// 工具名。
        tool: String,
        /// 拒绝原因。
        reason: String,
    },
    /// 一轮对话结束。
    TurnFinished {
        /// 实际用掉的最大工具轮次。
        steps: usize,
        /// 最终给用户看的文本。
        text: String,
    },
    /// 循环级错误（provider 挂了、AI 被禁用…）。
    Error {
        /// 人类可读原因。
        message: String,
    },
}

/// 事件出口。
pub trait AgentEventSink: Send + Sync {
    /// 发一条事件。实现方不得阻塞。
    fn emit(&self, event: AgentEvent);
}

/// 丢弃事件。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSink;

impl AgentEventSink for NoopSink {
    fn emit(&self, _event: AgentEvent) {}
}

/// 内存事件流（测试断言 + P0 的「暂不接 UI」）。
#[derive(Debug, Default)]
pub struct MemoryEventSink {
    events: Mutex<Vec<AgentEvent>>,
}

impl MemoryEventSink {
    /// 空 sink。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 取快照。
    #[must_use]
    pub fn events(&self) -> Vec<AgentEvent> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }

    /// 已发条数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.lock().map(|e| e.len()).unwrap_or(0)
    }

    /// 是否还没发过事件。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 拼接所有文本增量（测试断言「流式内容拼起来等于完整文本」）。
    #[must_use]
    pub fn streamed_text(&self) -> String {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                AgentEvent::TextDelta { text } => Some(text),
                _ => None,
            })
            .collect()
    }
}

impl AgentEventSink for MemoryEventSink {
    fn emit(&self, event: AgentEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }
}

/// 标准库 channel 出口（P1 的 app 用一个线程收、喂给 Slint 的事件循环）。
#[derive(Debug, Clone)]
pub struct MpscSink {
    tx: Sender<AgentEvent>,
}

impl MpscSink {
    /// 包一个 `std::sync::mpsc::Sender`。
    #[must_use]
    pub fn new(tx: Sender<AgentEvent>) -> Self {
        Self { tx }
    }
}

impl AgentEventSink for MpscSink {
    fn emit(&self, event: AgentEvent) {
        // 接收端已关闭说明 UI 没了，事件直接丢弃（不能拖累对话）。
        let _ = self.tx.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_sink_collects_and_concatenates_deltas() {
        let sink = MemoryEventSink::new();
        sink.emit(AgentEvent::TextDelta {
            text: "正在".into(),
        });
        sink.emit(AgentEvent::TextDelta {
            text: "读取日志".into(),
        });
        assert_eq!(sink.streamed_text(), "正在读取日志");
        assert_eq!(sink.len(), 2);
    }

    #[test]
    fn mpsc_sink_forwards() {
        let (tx, rx) = std::sync::mpsc::channel();
        let sink = MpscSink::new(tx);
        sink.emit(AgentEvent::Error {
            message: "boom".into(),
        });
        assert!(matches!(rx.recv(), Ok(AgentEvent::Error { .. })));
    }
}
