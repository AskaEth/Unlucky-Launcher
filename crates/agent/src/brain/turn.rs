//! 一轮对话的**循环本体**（从 `brain/mod.rs` 拆出来守 400 行硬线）。
//!
//! `run_turn` 是「provider 补全 → 有工具调用就执行并回灌 → 再补全」的循环。搬到独立文件只是
//! 行数问题；M388 在它上面新增的**唯一**东西是套在 provider 那个 await 外面的两条判据
//! （空闲时限 / 取消，见 [`super::guard`]），以及超时/取消时那一条轮次级审计。

use crate::audit::{make_record, AuditDraft, AuditOutcome};
use crate::event::AgentEvent;

use super::guard;
use super::{
    Brain, BrainError, ChatMessage, ChatRequest, EventDeltaObserver, StopReason, TurnOutcome,
    DISABLED_MESSAGE,
};

/// 轮次级审计记录用的 `tool` 名（不是工具：它记的是「这一轮怎么了」）。
///
/// 与工具记录共用一个落盘 schema（`AuditRecord`），靠 `tool` 与 `outcome` 两列区分：
/// `tool == "brain.turn"` 的行是**轮次事件**，不是某个工具的执行结果。
pub const TURN_AUDIT_TOOL: &str = "brain.turn";

impl Brain {
    /// 跑一轮对话。`history` 由调用方持有（P1 落盘为 `agent/sessions/*.jsonl`）。
    ///
    /// 两条判据（M388）套在 provider 那个 await 外面，见 [`guard`]：**空闲时限**与**取消**。
    /// 两者都不写「模型回复」——超时/取消是一条独立原因，界面据此说一句人话。
    ///
    /// # Errors
    /// AI 被禁用（[`BrainError::Disabled`]）、provider 报错（[`BrainError::Provider`]）、
    /// 空闲超时（[`BrainError::Timeout`]）或本轮被取消（[`BrainError::Cancelled`]）。
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

        let started = self.clock.now_unix_ms();
        // 时限**每轮现读**：环境变量改了，下一句话就生效（M384「配置是启动期快照」的教训）。
        let timeout = guard::resolve_timeout(self.timeout);
        let activity = guard::Activity::new();

        self.sink.emit(AgentEvent::TurnStarted {
            session_id: agent_session_id.to_string(),
            user_text: user_input.to_string(),
        });
        history.push(ChatMessage::user(user_input));

        let observer = EventDeltaObserver::new(std::sync::Arc::clone(&self.sink));
        // 增量一来就重置空闲计时（「沉默」才是病，长回答不是）。
        let counting = guard::ActivityObserver::new(&observer, std::sync::Arc::clone(&activity));
        let mut tool_calls = 0usize;
        let mut last_text = String::new();
        let mut used_steps = 0usize;

        for step in 1..=self.config.max_steps {
            used_steps = step;
            // 步与步之间也要能停（工具刚跑完、下一句还没发出去的那一刻）。
            if self.cancel.is_cancelled() {
                return Err(self.stop(agent_session_id, started, BrainError::Cancelled));
            }
            // **每步请求都把空闲计时重新起算**（复审 r1 P2-2）：上一步的工具执行可能比时限还长，
            // 不重起算的话这一句刚发出去就会被判「超时」，还把工具那一轮白回滚掉。
            activity.restart();
            let request = self.build_request(history);
            let guarded = guard::Guard::new(
                self.provider.complete(&request, &counting),
                self.cancel.clone(),
                std::sync::Arc::clone(&activity),
                timeout,
            );
            let reply = match guarded.await {
                Ok(reply) => reply,
                Err(guard::GuardError::Provider(err)) => {
                    self.sink.emit(AgentEvent::Error {
                        message: err.to_string(),
                    });
                    return Err(BrainError::Provider(err));
                }
                // 超时不是 provider 的错，也**不是模型回复**：不发 `Error` / `AssistantText`，
                // 结论由 UI 侧按当前语言落地（与「停止」同一条收尾通路）。
                Err(guard::GuardError::Timeout { secs }) => {
                    return Err(self.stop(agent_session_id, started, BrainError::Timeout { secs }));
                }
                Err(guard::GuardError::Cancelled) => {
                    return Err(self.stop(agent_session_id, started, BrainError::Cancelled));
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

    /// 超时 / 取消的**唯一出口**：先写一条**轮次级**审计，再把原因原样交回。
    ///
    /// ## 为什么这两档必须进审计（M388）
    ///
    /// `agent/audit/*.jsonl` 是事后复盘的权威记录。「被用户取消」与「工具报错」如果写成同一个
    /// `outcome`，那份日志以后既查不了用户容忍度，也查不了工具可靠性——两个完全不同的问题会混进
    /// 同一个计数。所以 `crates/agent/src/audit.rs` 里 `cancelled` / `timeout` 是**两个独立变体**。
    ///
    /// ## 为什么不发 `AgentEvent::Error`
    ///
    /// 用户按了停止、或者端点沉默到时限，都**不是模型说的话**。把它们写成 `Error` 事件会在 rail
    /// 上留下一条「像助手发言」的条目，并把同一件事说两遍（UI 侧按当前语言给出的那条才是结论）。
    /// 这里只留两条**机器可读**的痕迹：审计一行 + 启动器日志一行。
    fn stop(&self, agent_session_id: &str, started_ms: u64, error: BrainError) -> BrainError {
        let (outcome, summary) = match &error {
            BrainError::Cancelled => (
                AuditOutcome::Cancelled,
                "用户中断了本轮：后续正文不再进会话，历史回滚到提交前".to_string(),
            ),
            BrainError::Timeout { secs } => (
                AuditOutcome::Timeout,
                format!("请求超时：{secs} 秒没有收到任何响应"),
            ),
            // 只有上面两档走这里；其余原因原样交回（调用方自己去报）。
            _ => return error,
        };
        let args = serde_json::json!({});
        let record = make_record(
            self.clock.as_ref(),
            agent_session_id,
            AuditDraft {
                tool: TURN_AUDIT_TOOL,
                args: &args,
                outcome,
                summary: &summary,
                yolo: self.yolo.get(),
                confirmed: false,
                duration_ms: self.clock.now_unix_ms().saturating_sub(started_ms),
            },
        );
        self.audit.record(&record);
        eprintln!("[agent] {summary}");
        error
    }
}
