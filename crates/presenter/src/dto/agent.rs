//! AI 会话 DTO：消息、工具调用可视化、确认请求与 agent 事件。
//!
//! 与 agent 侧的对应关系（`crates/agent/src/event.rs` / `confirm.rs`）：这里是**扁平镜像**，
//! 不 import `unlucky_agent::*`（dto 是 IPC 边界，见 `dto/mod.rs` 的纪律）。字段口径刻意逐字
//! 对齐，翻译在接线层做：`AgentEvent::ConfirmRequested` ← `confirm::ConfirmRequest`。

use serde::{Deserialize, Serialize};

/// 会话消息角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallView {
    /// 工具名（与 ToolSpec 注册表同名）
    pub name: String,
    /// 是否成功
    pub ok: bool,
    /// 一行结果摘要
    pub summary: String,
}

/// 一条会话消息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    /// 系统提示（弱化展示；错误横幅也进会话历史，用户回翻时看得到）。
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            text: text.into(),
            tool: None,
            streaming: false,
        }
    }
}

/// 敏感等级（与 agent 侧 `confirm::Sensitivity` 逐字一致）。
///
/// | 等级 | 语义 |
/// |------|------|
/// | `Low` | 静默生效 |
/// | `Medium` | 静默生效，审计里能查到 |
/// | `High` | 必须用户确认（YOLO 可跳过确认，但**仍受白名单约束**） |
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    /// 静默生效。
    #[default]
    Low,
    /// 静默生效但审计标记。
    Medium,
    /// 必须用户确认。
    High,
}

impl Sensitivity {
    /// 是否会触发确认弹窗（非 YOLO 时）。
    #[must_use]
    pub fn needs_confirm(self) -> bool {
        self == Self::High
    }
}

/// 一次待确认的写操作（确认弹窗的原料；与 agent 侧 `confirm::ConfirmRequest` 对齐）。
///
/// 构造时**不要**放凭据：`details` 会按键名与内容双道脱敏（[`crate::agent::rail`]），
/// 但白名单之外的键本来就不该出现在这里。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfirmRequest {
    /// 请求 id（编排层生成，UI 回填决定时原样带回）
    pub id: String,
    /// 工具名（与 ToolSpec 注册表同名）
    pub tool: String,
    /// 一句话请求（「AI 请求把下载并发 64 改为 16」）
    pub summary: String,
    /// 结构化细节（`{"path":…, "old":…, "new":…}`），UI 画 diff 用
    pub details: serde_json::Value,
    /// 敏感等级
    pub sensitivity: Sensitivity,
}

/// agent 侧事件。
///
/// 四类都投到 [`crate::agent::AgentRail`] 上：流式增量 → 最后一条 assistant 消息；工具调用 →
/// 会话里可见的 tool 条目（D4）；错误 → 系统条目 + 横幅；确认请求 → `rail.confirm`。
///
/// 注意这里**没有** `Eq`：`ConfirmRequest::details` 是 `serde_json::Value`（可含浮点）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// 循环级错误（provider 断流、AI 被禁用、工具内部异常）。
    ///
    /// 与 `ToolCallFinished { ok: false }` 的分工：那个是**某一次工具调用**失败，对话继续；
    /// 这个是**这一轮回复**中止，rail 要收敛掉所有流式标记并立一个错误横幅。
    Failed {
        /// 人类可读原因
        message: String,
    },
    /// 需要用户确认（分级确认：写操作先问再改）。
    ConfirmRequested {
        /// 待确认请求
        request: ConfirmRequest,
    },
    /// 确认请求已收敛（重放/超时/别处已裁决）：把 rail 上同 id 的待确认清掉，避免僵尸弹窗。
    ConfirmResolved {
        /// 请求 id
        id: String,
    },
}
