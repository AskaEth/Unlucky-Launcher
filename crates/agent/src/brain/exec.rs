//! 工具执行编排：schema 校验 → 分级确认 → 执行 → 审计 → 事件。
//!
//! **YOLO 只在这里**（mission M16 任务 3 的分工）：工具自己不判断 policy，
//! 只声明「这次需要确认 + diff」；本模块按 [`ConfirmPolicy`] 决定问不问。
//!
//! 一次工具调用最多两条审计记录：
//! 1. 需要确认时先记一条 `awaiting_confirmation`（「AI 想改 X」本身必须留痕）；
//! 2. 结局记一条 `ok` / `denied` / `tool_error`（带 `yolo` 与 `confirmed` 标记）。
//!
//! 这两条区分了三种在复盘里完全不同的事：用户点了拒绝、YOLO 自动放行、用户点了允许。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::audit::{make_record, AuditDraft, AuditOutcome, AuditSink};
use crate::brain::chat::ToolCall;
use crate::clock::SharedClock;
use crate::confirm::{ConfirmDecision, ConfirmGate, ConfirmPolicy, ConfirmRequest, DenyGuidance};
use crate::event::{AgentEvent, AgentEventSink};
use crate::registry::validate::validate;
use crate::registry::{ToolCtx, ToolError, ToolRegistry};

/// 一次工具执行的结局（回灌给模型的内容 + 成败标记）。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolExecOutcome {
    /// 回灌内容（结构化）。
    pub content: Value,
    /// 是否成功（UI/事件用；模型看的是 `content`）。
    pub ok: bool,
}

impl ToolExecOutcome {
    /// 成功。
    #[must_use]
    pub fn ok(content: Value) -> Self {
        Self { content, ok: true }
    }

    /// 失败（`code` 给模型看，帮助它改正后重试）。
    #[must_use]
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            content: json!({ "status": "error", "code": code, "message": message.into() }),
            ok: false,
        }
    }

    /// 序列化成回灌给模型的字符串（工具消息的 `content`）。
    #[must_use]
    pub fn to_model_content(&self) -> String {
        serde_json::to_string(&self.content).unwrap_or_else(|_| {
            json!({ "status": "error", "code": "internal", "message": "结果序列化失败" })
                .to_string()
        })
    }
}

/// 工具执行器（无状态，可跨轮复用）。
pub struct ToolExecutor {
    registry: Arc<ToolRegistry>,
    audit: Arc<dyn AuditSink>,
    gate: Arc<dyn ConfirmGate>,
    policy: ConfirmPolicy,
    clock: SharedClock,
    sink: Arc<dyn AgentEventSink>,
    guidance: DenyGuidance,
    confirm_seq: AtomicU64,
}

impl ToolExecutor {
    /// 装配（默认拒绝口径 = 内置对话：[`DenyGuidance::InApp`]）。
    pub fn new(
        registry: Arc<ToolRegistry>,
        audit: Arc<dyn AuditSink>,
        gate: Arc<dyn ConfirmGate>,
        policy: ConfirmPolicy,
        clock: SharedClock,
        sink: Arc<dyn AgentEventSink>,
    ) -> Self {
        Self {
            registry,
            audit,
            gate,
            policy,
            clock,
            sink,
            guidance: DenyGuidance::InApp,
            confirm_seq: AtomicU64::new(0),
        }
    }

    /// 换拒绝口径（外部 MCP 客户端用 [`DenyGuidance::External`]：没有弹窗可点）。
    ///
    /// 只改**文案**：策略、闸门、审计标记一个字都不变——这正是「同一注册表、
    /// 同一安全策略、同一审计」在代码上的落点。
    #[must_use]
    pub fn with_guidance(mut self, guidance: DenyGuidance) -> Self {
        self.guidance = guidance;
        self
    }

    /// 执行一次工具调用（含确认与审计）。
    pub async fn execute(
        &self,
        agent_session_id: &str,
        call: &ToolCall,
        step: usize,
    ) -> ToolExecOutcome {
        let started_at = self.clock.now_unix_ms();
        self.sink.emit(AgentEvent::ToolCallStarted {
            tool: call.name.clone(),
            // 与审计同一套脱敏：事件（P1 会持久化）不能带出凭据明文。
            args: crate::redact::redact_args(&call.arguments),
            step,
        });

        let Some(spec) = self.registry.get(&call.name).cloned() else {
            let message = format!(
                "没有名为 `{}` 的工具。可用工具：{}",
                call.name,
                self.registry.names().join("、")
            );
            self.record(
                agent_session_id,
                &call.name,
                &call.arguments,
                AuditOutcome::UnknownTool,
                &message,
                false,
                false,
                started_at,
            );
            self.finish_event(&call.name, false, &message, false);
            return ToolExecOutcome::error("unknown_tool", message);
        };

        if let Err(errors) = validate(&spec.input_schema, &call.arguments) {
            let message = format!("参数不合法：{}", errors.join("；"));
            self.record(
                agent_session_id,
                &call.name,
                &call.arguments,
                AuditOutcome::InvalidArgs,
                &message,
                false,
                false,
                started_at,
            );
            self.finish_event(&call.name, false, &message, false);
            return ToolExecOutcome::error("invalid_args", message);
        }

        let ctx = ToolCtx {
            agent_session_id: agent_session_id.to_string(),
            confirmed: false,
        };
        let quoted = match (spec.handler)(&ctx, &call.arguments) {
            Ok(output) => output,
            Err(err) => return self.fail(agent_session_id, call, err, started_at),
        };

        let Some(confirmation) = quoted.confirmation.clone() else {
            return self.succeed(agent_session_id, call, quoted, false, false, started_at);
        };

        // ── 需要确认：决定问不问（YOLO 只影响这里）──
        let asked = self.policy.requires_confirmation(confirmation.sensitivity);
        let yolo_auto = !asked && confirmation.sensitivity == crate::confirm::Sensitivity::High;

        if asked {
            self.record(
                agent_session_id,
                &call.name,
                &call.arguments,
                AuditOutcome::AwaitingConfirmation,
                &confirmation.summary,
                false,
                false,
                started_at,
            );
            let request = ConfirmRequest {
                id: format!(
                    "confirm-{}",
                    self.confirm_seq.fetch_add(1, Ordering::Relaxed) + 1
                ),
                tool: call.name.clone(),
                summary: confirmation.summary.clone(),
                details: confirmation.details.clone(),
                sensitivity: confirmation.sensitivity,
            };
            self.sink.emit(AgentEvent::ConfirmRequested {
                request: Box::new(request.clone()),
            });
            if self.gate.request(request).await == ConfirmDecision::Denied {
                let message = self.guidance.message().to_string();
                self.record(
                    agent_session_id,
                    &call.name,
                    &call.arguments,
                    AuditOutcome::Denied,
                    &message,
                    false,
                    false,
                    started_at,
                );
                self.sink.emit(AgentEvent::ToolDenied {
                    tool: call.name.clone(),
                    reason: message.clone(),
                });
                self.finish_event(&call.name, false, &message, true);
                return ToolExecOutcome::error("denied", message);
            }
        }

        // ── 已获授权（用户允许或 YOLO）：带 confirmed 重放 ──
        let confirmed_ctx = ToolCtx {
            agent_session_id: agent_session_id.to_string(),
            confirmed: true,
        };
        match (spec.handler)(&confirmed_ctx, &call.arguments) {
            Ok(output) => {
                if let Some(second) = output.confirmation {
                    // 工具在"已确认"状态下仍要确认 = 实现写错了，明确报出来而不是死循环。
                    let message = format!(
                        "工具 `{}` 在已确认状态下仍请求确认：{}",
                        call.name, second.summary
                    );
                    self.record(
                        agent_session_id,
                        &call.name,
                        &call.arguments,
                        AuditOutcome::ToolError,
                        &message,
                        yolo_auto,
                        asked,
                        started_at,
                    );
                    self.finish_event(&call.name, false, &message, true);
                    return ToolExecOutcome::error("internal", message);
                }
                self.succeed(agent_session_id, call, output, asked, yolo_auto, started_at)
            }
            Err(err) => {
                self.fail_after_confirm(agent_session_id, call, err, asked, yolo_auto, started_at)
            }
        }
    }

    fn succeed(
        &self,
        agent_session_id: &str,
        call: &ToolCall,
        output: crate::registry::ToolOutput,
        confirmed: bool,
        yolo: bool,
        started_at: u64,
    ) -> ToolExecOutcome {
        self.record(
            agent_session_id,
            &call.name,
            &call.arguments,
            AuditOutcome::Ok,
            &output.summary,
            yolo,
            confirmed,
            started_at,
        );
        self.finish_event(
            &call.name,
            true,
            &output.summary,
            output.confirmation.is_some(),
        );
        ToolExecOutcome::ok(output.content)
    }

    fn fail(
        &self,
        agent_session_id: &str,
        call: &ToolCall,
        err: ToolError,
        started_at: u64,
    ) -> ToolExecOutcome {
        self.fail_after_confirm(agent_session_id, call, err, false, false, started_at)
    }

    fn fail_after_confirm(
        &self,
        agent_session_id: &str,
        call: &ToolCall,
        err: ToolError,
        confirmed: bool,
        yolo: bool,
        started_at: u64,
    ) -> ToolExecOutcome {
        let message = err.message().to_string();
        self.record(
            agent_session_id,
            &call.name,
            &call.arguments,
            AuditOutcome::ToolError,
            &message,
            yolo,
            confirmed,
            started_at,
        );
        self.finish_event(&call.name, false, &message, false);
        ToolExecOutcome::error(err.code(), message)
    }

    #[allow(clippy::too_many_arguments)]
    fn record(
        &self,
        agent_session_id: &str,
        tool: &str,
        args: &Value,
        outcome: AuditOutcome,
        summary: &str,
        yolo: bool,
        confirmed: bool,
        started_at: u64,
    ) {
        let duration_ms = self.clock.now_unix_ms().saturating_sub(started_at);
        let draft = AuditDraft {
            tool,
            args,
            outcome,
            summary,
            yolo,
            confirmed,
            duration_ms,
        };
        self.audit
            .record(&make_record(self.clock.as_ref(), agent_session_id, draft));
    }

    fn finish_event(&self, tool: &str, ok: bool, summary: &str, requires_confirmation: bool) {
        self.sink.emit(AgentEvent::ToolCallFinished {
            tool: tool.to_string(),
            ok,
            summary: summary.to_string(),
            requires_confirmation,
        });
    }
}
