//! AI 会话 DTO：消息、工具调用可视化与 agent 事件。

/// 会话消息角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    /// 用户
    User,
    /// 助手（可流式）
    Assistant,
    /// 工具调用（会话里可见，D4 要求）
    Tool,
    /// 系统提示（弱化展示）
    System,
}

/// 工具调用可视化条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallView {
    /// 工具名（与 ToolSpec 注册表同名）
    pub name: String,
    /// 是否成功
    pub ok: bool,
    /// 一行结果摘要
    pub summary: String,
}

/// 一条会话消息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// 角色
    pub role: ChatRole,
    /// 正文（流式时持续追加）
    pub text: String,
    /// role == Tool 时带上工具调用详情
    pub tool: Option<ToolCallView>,
    /// 流式未收敛
    pub streaming: bool,
}

impl ChatMessage {
    /// 用户消息。
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            text: text.into(),
            tool: None,
            streaming: false,
        }
    }

    /// 助手流式消息起点（空文本 + streaming）。
    pub fn assistant_stream() -> Self {
        Self {
            role: ChatRole::Assistant,
            text: String::new(),
            tool: None,
            streaming: true,
        }
    }
}

/// agent 侧事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    /// 追加一条完整消息
    MessageAppended(ChatMessage),
    /// 给指定序号的流式消息追加增量
    MessageDelta {
        /// 目标消息在会话里的序号
        index: usize,
        /// 增量文本
        delta: String,
    },
    /// 流式收敛
    MessageFinished {
        /// 目标消息序号
        index: usize,
    },
    /// 开始一次工具调用（会话里插入工具条目）
    ToolCallStarted {
        /// 工具名
        name: String,
        /// 调用参数摘要
        summary: String,
    },
    /// 工具调用结束
    ToolCallFinished {
        /// 工具名
        name: String,
        /// 是否成功
        ok: bool,
        /// 结果摘要
        summary: String,
    },
}
