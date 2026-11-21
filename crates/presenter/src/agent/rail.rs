//! AI 助手栏（D16 双形态：右侧常驻栏 / 独立页）的**唯一**视图模型。
//!
//! 两形态共用同一份 [`AgentRail`]：折叠/展开、停靠/拆页都只是换渲染位置，切形态不丢会话。
//!
//! # 1. 事件从哪里来、到哪里去
//!
//! ```text
//! agent/host 后端 ──AgentEvent──► AppState::apply ──► AgentRail::ingest ──► 字段
//!                                       └──► unread（栏不可见时累加）
//! ```
//!
//! 投影规则（四类事件各归各位）：
//!
//! | 事件 | 落到哪 |
//! | ---- | ------ |
//! | `MessageAppended` | 追加一条消息；`streaming` 的置 `rail.streaming` |
//! | `MessageDelta` | 追加到最后一条 assistant 消息的正文（**累积在一条上**，不是每条一个气泡） |
//! | `MessageFinished` | 清掉该条（以及全部）流式标记，`streaming` 由消息重算 |
//! | `ToolCallStarted` / `ToolCallFinished` | 生成 / 收敛 [`ToolCallView`] 条目（D4：工具调用会话可见） |
//! | `Failed` | 收敛流式 + 立横幅 + 追加系统条目（错误也要能回翻） |
//! | `ConfirmRequested` / `ConfirmResolved` | `rail.confirm` 置位 / 清除 |
//!
//! # 2. 红线：凭据不得进 rail，也不得**出站**
//!
//! 两道闸门，各守一侧，**不能互相替代**：
//!
//! | 闸门 | 位置 | 守什么 |
//! | ---- | ---- | ------ |
//! | 出站 | [`crate::agent::Presenter::agent_submit`] 的 `scrub_text(trimmed)` / `map_err` | 发给后端的 prompt、回给 UI 的 `Err` 文案 |
//! | 显示 | [`AgentRail::ingest`] 这里 | 进入 rail 的所有事件文本（消息正文 / 工具名与摘要 / 错误 / 确认请求） |
//!
//! 只留出站闸门 → 后端事件里的凭据会直接显示在聊天栏；只留显示闸门 → 界面一片干净而
//! **凭据已经发出去了**。两边各有独立守卫（`agent/tests_red_line.rs`）。
//!
//! [`AgentRail::ingest`] 逐字段过 [`scrub_text`] / [`scrub_details`]。凭据字段（`token` /
//! `refresh_token` / `password` / `access_token` …）本就不该出现在事件里（AGENTS §6：AI 永不
//! 接触账户凭据），这里的脱敏是**第二道**防线：万一实现方把 JWT 塞进了工具摘要或错误文案，
//! rail 上也不会出现原文。
//!
//! 规则来源（刻意不另写一套正则）：
//! - 内容侧：复用 `unlucky_core::launch::redact`（M40 上提的「带点 token」+ ≥64 不透明串）；
//! - 上下文侧：照 `unlucky_agent::redact` 补两条——`Bearer <短令牌>` 掩码、
//!   `{"path": 敏感键}` 时 `value`/`old`/`new` 一并掩码（`set_config` 的 details 正是它）。
//!
//! 与 `unlucky_agent::redact` 的实际关系（**别读成「已无差异」，也别读成「已经漂移」**）：
//! - 键名名单（8 条 needle）：**逐字相同**；
//! - path 规则：**语义相同**（presenter 额外对字符串值跑 `scrub_text`，比 `redact_args` 更严）；
//! - [`mask_bearer`]：**行为不完全等价的自写迁移**——停字符集 presenter 8 个（含 `;`，
//!   不含 `/` `\`）vs agent `segment_end` 9 个（含 `/` `\`，不含 `;`），占位符也不同
//!   （`<redacted:token>` vs `***`）。`Bearer abc;def` presenter 掩得更少（`abc` 被掩、
//!   `;def` 留着），`Bearer abc/def` 掩得更多。方向安全（多掩不漏；少掩只发生在 `;` 上），
//!   右边界由 `tests::bearer_right_boundary_is_pinned` 钉住。
//! - 两处重复实现（键名名单 + Bearer）是**已知欠账**：上提一份公共实现要单开 mission
//!   （presenter 不能依赖 `unlucky-agent`，那会拖进 rig / tokio / rmcp）。
//!
//! # 3. 刻意不做的事
//!
//! - **不裁剪 `messages`**：`MessageDelta` 用下标寻址，从头丢弃会让下标漂移。会话长度由后端
//!   的轮次上限控制；这里只保证下标稳定。
//! - **不持有后端**：rail 是纯数据 + 投影，提交/确认在 [`crate::agent::Presenter`]。

use serde_json::{Map, Value};
use unlucky_core::launch::redact::{Redactor, TOKEN_PLACEHOLDER};

use crate::dto::{AgentEvent, ChatMessage, ChatRole, ConfirmRequest, ToolCallView};

/// 键名命中即整值打码（与 `unlucky_agent::redact::is_sensitive_key` 同一份名单）。
///
/// 为什么在这里再写一遍：presenter 不依赖 `unlucky-agent`（那会把 rig / tokio / rmcp 整棵树
/// 拖进共用宿主层）。名单漂移的风险已记进 M103 报告，长期该由 core 出一份公共实现。
const SENSITIVE_KEY_NEEDLES: [&str; 8] = [
    "api_key",
    "apikey",
    "token",
    "password",
    "passwd",
    "secret",
    "authorization",
    "credential",
];

/// 文本是否像凭据键名（`access_token` / `refresh_token` / `microsoft.password` …）。
#[must_use]
pub fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEY_NEEDLES
        .iter()
        .any(|needle| lower.contains(needle))
}

/// 文本脱敏：内容规则三件套。
///
/// 1. JWT 形状（带点 token）/ ≥64 不透明串 —— 复用 core 的 `Redactor`（M40 上提的那份）；
/// 2. `Bearer` 后面的短令牌 —— core 没有这条，照 `unlucky_agent::redact::mask_bearer` 补；
/// 3. 主目录不登记：rail 不持有数据根，`<home>` 掩码是 host 侧的职责。
#[must_use]
pub fn scrub_text(text: &str) -> String {
    mask_bearer(&Redactor::new().scrub(text))
}

/// 结构化细节脱敏：按**键名**、按**值随 path 敏感**、按内容三道（见模块文档 §2）。
///
/// 同样对齐 `unlucky_agent::redact::redact_args` 的两条上下文规则：键名命中即掩码，
/// 以及 `{"path": "ai.api_key", "old": …, "new": …}` 这种「值随路径敏感」的形状
/// （`set_config` 的 details 正是它）——否则 API key 会原样躺在确认弹窗的 diff 里。
#[must_use]
pub fn scrub_details(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sensitive_path = map
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(is_sensitive_key);
            let mut out = Map::with_capacity(map.len());
            for (key, child) in map {
                let masked = is_sensitive_key(key)
                    || (sensitive_path && matches!(key.as_str(), "value" | "old" | "new"));
                let child = if masked {
                    Value::String(TOKEN_PLACEHOLDER.to_string())
                } else {
                    scrub_details(child)
                };
                out.insert(key.clone(), child);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(scrub_details).collect()),
        Value::String(text) => Value::String(scrub_text(text)),
        other => other.clone(),
    }
}

/// `Bearer` 之后的分隔符（令牌到此为止）。与 agent `segment_end` 的差异见模块文档 §2：
/// 多 `;`、少 `/` `\`（方向安全：多掩不漏），右边界由 `tests::bearer_right_boundary_is_pinned` 钉住。
const BEARER_STOPS: [char; 8] = [' ', '\t', '\n', '\r', '"', '\'', ',', ';'];

/// `Bearer <token>` → `Bearer <redacted:token>`。
///
/// core 的 `Redactor` 只认「点号 token / ≥64 不透明串」，短令牌（`Bearer abcdef123456`）
/// 会原样放过——这条规则照 `unlucky_agent::redact::mask_bearer` 的意图补上（实现是自写的，
/// 见模块文档 §2 的差异说明）。幂等：已经是 `<redacted:token>` 的再跑一遍结果不变。
fn mask_bearer(text: &str) -> String {
    const MARKER: &str = "Bearer ";
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    while let Some(position) = text[cursor..].find(MARKER) {
        let token_start = cursor + position + MARKER.len();
        let token_end = bearer_token_end(text, token_start);
        out.push_str(&text[cursor..token_start]);
        if token_end > token_start {
            out.push_str(TOKEN_PLACEHOLDER);
        }
        cursor = token_end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// 从 `from` 起取到下一个分隔符为止（令牌的右边界）。
fn bearer_token_end(text: &str, from: usize) -> usize {
    text[from..]
        .find(BEARER_STOPS)
        .map_or(text.len(), |offset| from + offset)
}

/// AI 助手栏视图模型（UI 壳只读它，不自己攒会话状态）。
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRail {
    /// 会话消息（user / assistant / tool / system），下标稳定
    pub messages: Vec<ChatMessage>,
    /// 栏不可见期间攒下的未读条数（`agent_mark_read` 清零）
    pub unread: usize,
    /// 是否该禁用输入（一轮回复进行中，或有写操作待确认）
    pub busy: bool,
    /// 输入框草稿（切形态不丢）
    pub draft: String,
    /// 待确认的写操作（`Some` = 弹确认框；非 YOLO 下 `busy` 必为 true）
    pub confirm: Option<ConfirmRequest>,
    /// 是否有流式消息在跑（Slint 壳读它切换「发送/停止」；新壳用 `busy` 即可）
    pub streaming: bool,
    /// YOLO 开关（跳过确认，审计照记）
    pub yolo: bool,
    /// 端点标签（栏顶显示）
    pub endpoint_label: String,
    /// 最近一次错误（横幅；下一次提交或 ingest 到新消息时清掉）
    pub error: Option<String>,
}

impl Default for AgentRail {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRail {
    /// 空会话。默认端点标签是 P0 的本地 llama.cpp（P1 由设置页决定）。
    #[must_use]
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            unread: 0,
            busy: false,
            draft: String::new(),
            confirm: None,
            streaming: false,
            yolo: false,
            endpoint_label: "llama.cpp · 本地".to_string(),
            error: None,
        }
    }

    /// 流式收敛：清掉全部流式标记并按消息重算 `streaming` / `busy`。
    pub fn finish_stream(&mut self) {
        for message in &mut self.messages {
            message.streaming = false;
        }
        self.settle_streaming();
    }

    /// 提交成功、后端还没回事件时先把「一轮在跑」点亮。
    ///
    /// 异步后端在「提交成功 → 第一个事件到达」之间有空窗；不先置忙的话用户能再点一次发送，
    /// 于是两条 user 消息、两轮回复并行。反正事件流收敛时会用
    /// [`AgentRail::settle_streaming`] 按消息重算，不会留下假忙。
    pub fn begin_turn(&mut self) {
        self.streaming = true;
        self.sync_busy();
    }

    /// 按消息重算 `streaming`（有任一条还在流式即为真）并同步 `busy`。
    pub fn settle_streaming(&mut self) {
        self.streaming = self.messages.iter().any(|message| message.streaming);
        self.sync_busy();
    }

    /// 清零未读（用户正在看 / 主动点掉红点）。
    pub fn mark_read(&mut self) {
        self.unread = 0;
    }

    /// 取走待确认请求（`confirm` 置空并按需重算 `busy`）；没有则返回 `None`。
    pub fn take_confirm(&mut self) -> Option<ConfirmRequest> {
        let request = self.confirm.take();
        self.sync_busy();
        request
    }

    /// 投影一条 agent 事件。
    ///
    /// 返回 `true` 表示这是一条「用户不在栏前也会想知道」的新内容（未读计数的原料）：
    /// 助手/工具消息、错误、确认请求算；用户自己说的话与纯增量不算。
    pub fn ingest(&mut self, event: &AgentEvent) -> bool {
        match event {
            AgentEvent::MessageAppended(message) => {
                let message = scrub_message(message);
                let fresh = matches!(message.role, ChatRole::Assistant | ChatRole::Tool);
                if message.streaming {
                    self.streaming = true;
                }
                if fresh {
                    self.error = None;
                }
                self.messages.push(message);
                self.sync_busy();
                fresh
            }
            AgentEvent::MessageDelta { index, delta } => {
                // 只追加正文，不动流式标记：迟到的尾增量（`Failed` 之后）不该把
                // 「一轮在跑」重新点亮，否则输入框会一直禁用。
                match self.messages.get_mut(*index) {
                    Some(message) => message.text.push_str(&scrub_text(delta)),
                    // 分片乱序/重放时正文会无声消失（用户只看到半截话）：开发期吵起来，
                    // 发布期宁可丢这一片也不去猜该拼到哪条上。
                    None => debug_assert!(
                        false,
                        "MessageDelta 指向不存在的消息下标 {index}（会话 {} 条）",
                        self.messages.len()
                    ),
                }
                self.sync_busy();
                false
            }
            AgentEvent::MessageFinished { index } => {
                if let Some(message) = self.messages.get_mut(*index) {
                    message.streaming = false;
                }
                // 以消息为准重算：并发工具条目没收敛时 `streaming` 不能提前熄
                self.settle_streaming();
                false
            }
            AgentEvent::ToolCallStarted { name, summary } => {
                let summary = scrub_text(summary);
                self.streaming = true;
                self.error = None;
                self.messages.push(ChatMessage {
                    role: ChatRole::Tool,
                    text: summary.clone(),
                    tool: Some(ToolCallView {
                        name: scrub_text(name),
                        ok: false,
                        summary,
                    }),
                    streaming: true,
                });
                self.sync_busy();
                true
            }
            AgentEvent::ToolCallFinished { name, ok, summary } => {
                let name = scrub_text(name);
                let summary = scrub_text(summary);
                // 只收敛「还在跑的同名调用」。找不到就**忽略**，绝不退回改写已收敛的旧条目
                // ——那会把历史结果覆盖掉（重放、重复结束事件、丢过 ToolCallStarted 都会命中）。
                let pending = self
                    .messages
                    .iter()
                    .rposition(|message| message.streaming && tool_named(message, &name));
                if let Some(index) = pending {
                    let message = &mut self.messages[index];
                    message.streaming = false;
                    message.text.clone_from(&summary);
                    if let Some(tool) = &mut message.tool {
                        tool.ok = *ok;
                        tool.summary = summary;
                    }
                }
                self.settle_streaming();
                false
            }
            AgentEvent::Failed { message } => {
                let text = scrub_text(message);
                self.finish_stream();
                self.error = Some(text.clone());
                self.messages.push(ChatMessage::system(text));
                true
            }
            AgentEvent::ConfirmRequested { request } => {
                self.confirm = Some(scrub_confirm(request));
                self.sync_busy();
                true
            }
            AgentEvent::ConfirmResolved { id } => {
                if self
                    .confirm
                    .as_ref()
                    .is_some_and(|pending| &pending.id == id)
                {
                    self.confirm = None;
                }
                self.sync_busy();
                false
            }
        }
    }

    /// `busy` 的唯一写入口：`busy == streaming || confirm.is_some()`。
    ///
    /// UI 靠 `busy` 禁用输入框，所以「等确认」必须算忙——否则用户能在一轮写操作中间再发一句，
    /// 两个工具调用会踩同一份配置。
    fn sync_busy(&mut self) {
        self.busy = self.streaming || self.confirm.is_some();
    }
}

/// 消息脱敏（正文 + 工具摘要；工具名也过一遍，防止实现方把凭据塞进名字）。
fn scrub_message(message: &ChatMessage) -> ChatMessage {
    let tool = message.tool.as_ref().map(|tool| ToolCallView {
        name: scrub_text(&tool.name),
        ok: tool.ok,
        summary: scrub_text(&tool.summary),
    });
    ChatMessage {
        role: message.role,
        text: scrub_text(&message.text),
        tool,
        streaming: message.streaming,
    }
}

/// 确认请求脱敏：摘要按内容、细节按键名 + 内容双道。
fn scrub_confirm(request: &ConfirmRequest) -> ConfirmRequest {
    ConfirmRequest {
        id: request.id.clone(),
        tool: scrub_text(&request.tool),
        summary: scrub_text(&request.summary),
        details: scrub_details(&request.details),
        sensitivity: request.sensitivity,
    }
}

/// 这条消息是不是指定名字的工具条目。
fn tool_named(message: &ChatMessage, name: &str) -> bool {
    message.tool.as_ref().is_some_and(|tool| tool.name == name)
}
