//! 事件映射（[`Mapper`]）+ 脱敏器（[`Scrubber`]）：agent 侧事件 → presenter 的 `AgentEvent`。
//!
//! 从 `real.rs` 拆出来的唯一理由是**400 行红线**（`guards.yml` 的 file-size 门禁）：装配、
//! 映射、确认闸门是三件互不相干的事，挤在一个文件里读不动。拆分不改行为。

use std::sync::{Arc, Mutex};

use unlucky_agent::event::{AgentEvent as BrainEvent, AgentEventSink};
use unlucky_presenter::dto::{AgentEvent, AppEvent, ChatMessage};

use super::super::transport::EventQueue;

/// 诊断串脱敏器（把 provider 错误里的密钥原文掩掉；见模块文档 §3）。
///
/// 为什么包一层 newtype 而不是裸 `Arc<dyn Fn>`：函数调用表达式**不会**对 `Arc<dyn Fn>`
/// 自动解引用（`(arc)(x)` 编不过），包一层 `apply` 让每个调用点都是普通方法调用。
#[derive(Clone)]
pub(crate) struct Scrubber(Arc<dyn Fn(&str) -> String + Send + Sync>);

impl Scrubber {
    /// 用任意闭包包一个脱敏器。
    pub(crate) fn new(f: impl Fn(&str) -> String + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    /// 脱敏一段文本。
    pub(crate) fn apply(&self, text: &str) -> String {
        let f: &dyn Fn(&str) -> String = &*self.0;
        f(text)
    }
}

/// 把 agent 侧的 [`BrainEvent`] 翻成 presenter 的 [`AgentEvent`] 并投进队列。
///
/// 翻译的三条规则（逐条对应 `AgentRail::ingest` 的投影表）：
/// - **一轮里"有正文才开气泡"**：第一段正文到来时才 `MessageAppended(assistant_stream)`
///   （复审 r1 P2-2：提前开会在"工具先行 / 空回合 / 报错"时留下空白助手卡），碰到工具调用或
///   一阶段文本结束就收敛，下一阶段文本再开一条——下标靠 `appended` 递推，与 rail 的尾部
///   追加严格对齐；
/// - **`AssistantText` 与流式增量去重**：provider 已经吐过增量时不再补一遍全文
///   （rig 适配层两者都会发，重复上屏等于同一句话说两遍）；
/// - **`ConfirmRequested` 由闸门发**：编排层先发一次事件再调闸门，这里若也发，
///   确认条会出现两条（rail 只留最后一条，但事件计数与测试会漂）。
pub(crate) struct Mapper {
    pub(crate) queue: EventQueue,
    scrub: Scrubber,
    /// 本轮第一条消息的下标。
    base: usize,
    /// 本轮已经追加了几条消息（rail 侧同序追加，所以下标 = base + appended）。
    appended: usize,
    /// 当前流式助手气泡的下标（`None` = 没有开着的气泡）。
    open: Option<usize>,
    /// 当前这条气泡是否已经有正文（`AssistantText` 去重靠它）。
    has_text: bool,
    /// 本轮是否已经上过文本（`TurnFinished` 的兜底靠它）。
    streamed: bool,
    /// 本轮是否已经报过错（避免同一个错误说两遍）。
    saw_error: bool,
    /// 本轮是否已被 UI 丢弃（M388「停止」）。
    ///
    /// 置位后 [`Mapper::on_event`] 与 [`Mapper::end_turn`] **一个事件都不再产出**：worker 可能还
    /// 在 socket 上（真打断没有保证能及时生效），它后面吐的每一段正文都属于用户已经停掉的那一轮，
    /// 冒到 rail 上就是「按了停止还在写」。下一条 `begin`（下一轮）自动解除。
    muted: bool,
    /// 被丢弃的事件条数（取证与日志用：「丢弃不是中断」要能量出来）。
    dropped: usize,
}

impl Mapper {
    pub(crate) fn new(queue: EventQueue, scrub: Scrubber) -> Self {
        Self {
            queue,
            scrub,
            base: 0,
            appended: 0,
            open: None,
            has_text: false,
            streamed: false,
            saw_error: false,
            muted: false,
            dropped: 0,
        }
    }

    pub(crate) fn begin(&mut self, base: usize) {
        self.base = base;
        self.appended = 0;
        self.open = None;
        self.has_text = false;
        self.streamed = false;
        self.saw_error = false;
        // 下一轮开工：解除上一轮的丢弃态（「取消后能立刻重试」靠这一行）。
        self.muted = false;
    }

    /// 从这一刻起**丢弃**本轮的后续事件（M388「停止」），返回此前已丢弃的条数。
    pub(crate) fn mute(&mut self) -> usize {
        self.muted = true;
        self.open = None;
        self.dropped
    }

    /// 本轮被丢弃的事件条数（取证与日志用）。
    pub(crate) fn dropped(&self) -> usize {
        self.dropped
    }

    fn next_index(&mut self) -> usize {
        let index = self.base + self.appended;
        self.appended += 1;
        index
    }

    fn push_message(&mut self, message: ChatMessage) {
        self.next_index();
        push_event(
            &self.queue,
            AgentEvent::MessageAppended(scrub_message(&self.scrub, message)),
        );
    }

    fn open_assistant(&mut self) {
        if self.open.is_some() {
            return;
        }
        let index = self.next_index();
        self.open = Some(index);
        self.has_text = false;
        push_event(
            &self.queue,
            AgentEvent::MessageAppended(scrub_message(
                &self.scrub,
                ChatMessage::assistant_stream(),
            )),
        );
    }

    fn close_assistant(&mut self) {
        if let Some(index) = self.open.take() {
            self.has_text = false;
            push_event(&self.queue, AgentEvent::MessageFinished { index });
        }
    }

    fn delta(&mut self, text: &str) {
        self.open_assistant();
        if let Some(index) = self.open {
            self.has_text = true;
            self.streamed = true;
            push_event(
                &self.queue,
                AgentEvent::MessageDelta {
                    index,
                    delta: self.scrub.apply(text),
                },
            );
        }
    }

    pub(crate) fn on_event(&mut self, event: &BrainEvent) {
        if self.muted {
            self.dropped += 1;
            return;
        }
        match event {
            BrainEvent::TurnStarted { user_text, .. } => {
                // **不在这里开助手气泡**（复审 r1 P2-2）：模型这一步可能只给工具调用、一个字都
                // 不说，甚至直接报错。提前开一条 `assistant_stream()` 之后只能 `MessageFinished`
                // 收尾，而 rail 没有空消息回收 ⇒ 会话里留下一张空白助手卡（M384 r1 的 02/03
                // 两张取证图上都能看到）。改成"第一段正文到来时才开"：
                // `delta()` / `AssistantText` / `TurnFinished` 的兜底三条路径都会调
                // `open_assistant()`，下标由 `next_index()` 递推，工具条目照旧占位。
                self.push_message(ChatMessage::user(user_text.clone()));
            }
            BrainEvent::TextDelta { text } => self.delta(text),
            BrainEvent::AssistantText { text } => {
                // 增量已经上过就不重复（rig 适配层两者都会发）
                if !self.has_text && !text.trim().is_empty() {
                    self.delta(text);
                }
                self.close_assistant();
            }
            BrainEvent::ToolCallStarted { tool, args, .. } => {
                self.close_assistant();
                // `args` 是 agent 侧的 `serde_json::Value`；app-iced 的依赖表里没有 serde_json
                // （`transport.rs:226` 同一处口径），所以这里只经 agent 的 `redact_args` 脱敏后
                // 折成一行展示——不需要在本 crate 里命名那个类型。
                let redacted = unlucky_agent::redact::redact_args(args);
                let summary: String = self
                    .scrub
                    .apply(&format!("{redacted}"))
                    .chars()
                    .take(160)
                    .collect();
                push_event(
                    &self.queue,
                    AgentEvent::ToolCallStarted {
                        name: tool.clone(),
                        summary,
                    },
                );
                self.next_index();
            }
            BrainEvent::ToolCallFinished {
                tool, ok, summary, ..
            } => push_event(
                &self.queue,
                AgentEvent::ToolCallFinished {
                    name: tool.clone(),
                    ok: *ok,
                    summary: self.scrub.apply(summary),
                },
            ),
            // 闸门自己发（见类型文档）；紧随其后的 `ToolCallFinished` 会把工具条目收敛掉。
            BrainEvent::ConfirmRequested { .. } | BrainEvent::ToolDenied { .. } => {}
            BrainEvent::TurnFinished { text, .. } => {
                if !self.streamed && !text.trim().is_empty() {
                    self.delta(text);
                }
                self.close_assistant();
            }
            BrainEvent::Error { message } => {
                self.saw_error = true;
                self.close_assistant();
                let detail = self.scrub.apply(message);
                // 用户报障要能拿到这句：横幅只有一句话，完整原因进启动器日志（stderr → 日志文件）。
                eprintln!("[agent] 本轮失败：{detail}");
                push_event(&self.queue, failed(detail));
            }
        }
    }

    /// 一轮收尾：`Err` 且这一轮没报过错 ⇒ 补一条 `Failed`（否则输入框会一直禁用）。
    ///
    /// 被 UI 丢弃的那一轮（[`Mapper::mute`]）**什么都不产出**：用户已经按了停止，再补一条
    /// 「本轮失败」的横幅等于让停止看起来像出错。
    pub(crate) fn end_turn(&mut self, error: Option<String>) {
        // 收尾调用本身不是「事件」：`dropped` 只数被映射器吞掉的 agent 事件。
        if self.muted {
            return;
        }
        if let Some(message) = error.filter(|_| !self.saw_error) {
            self.saw_error = true;
            self.close_assistant();
            let detail = self.scrub.apply(&message);
            eprintln!("[agent] 本轮失败（没有 Error 事件，按 Result 补报）：{detail}");
            push_event(&self.queue, failed(detail));
        }
        self.close_assistant();
    }
}

/// [`AgentEventSink`] 的落点：编排层每发一条事件就翻一条。
pub(crate) struct MapperSink {
    pub(crate) mapper: Arc<Mutex<Mapper>>,
}

impl AgentEventSink for MapperSink {
    fn emit(&self, event: BrainEvent) {
        if let Ok(mut mapper) = self.mapper.lock() {
            mapper.on_event(&event);
        }
    }
}

/// 包一层 [`AppEvent::Agent`] 并入队（队列只认顶层事件）。
pub(crate) fn push_event(queue: &EventQueue, event: AgentEvent) {
    if let Ok(mut queue) = queue.lock() {
        queue.push_back(AppEvent::Agent(event));
    }
}

pub(crate) fn failed(message: String) -> AgentEvent {
    AgentEvent::Failed { message }
}

/// 消息正文过一道密钥脱敏（rail 还有第二道，见模块文档 §3）。
fn scrub_message(scrub: &Scrubber, message: ChatMessage) -> ChatMessage {
    ChatMessage {
        text: scrub.apply(&message.text),
        ..message
    }
}
