//! 确认闸门（纯 std future）：请求进队列，然后挂起等 UI 的「允许 / 拒绝」。
//!
//! 从 `real.rs` 拆出来的唯一理由是**400 行红线**（`guards.yml` 的 file-size 门禁）。行为与
//! 拆分前逐字一致：`ConfirmGate::request` 把请求投进 UI 队列，返回的 future 在 UI 裁决之前
//! 一直是 `Pending`——**fail-closed**，没人点就不动。

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use unlucky_agent::confirm::{
    ConfirmDecision, ConfirmFuture, ConfirmGate, ConfirmRequest as BrainConfirm,
};
use unlucky_presenter::dto::{AgentEvent, ConfirmRequest, Sensitivity};

use super::super::transport::EventQueue;
use super::mapper::{push_event, Scrubber};

pub(crate) type DecisionSlot = Arc<Mutex<DecisionState>>;

/// 一次待确认的裁决（谁在等 + 用户点了什么）。
#[derive(Default)]
pub(crate) struct DecisionState {
    pub(crate) id: Option<String>,
    pub(crate) value: Option<bool>,
    pub(crate) waker: Option<Waker>,
}

impl DecisionState {
    /// 用户裁决：id 对得上才算数（返回是否吃下这一票）。
    pub(crate) fn decide(&mut self, request_id: &str, accept: bool) -> bool {
        if self.id.as_deref() != Some(request_id) {
            return false;
        }
        self.value = Some(accept);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        true
    }
}

/// 确认闸门：请求进队列，然后挂起等 UI 的裁决。
pub(crate) struct ConfirmBridge {
    pub(crate) queue: EventQueue,
    pub(crate) decision: DecisionSlot,
    pub(crate) scrub: Scrubber,
}

impl ConfirmGate for ConfirmBridge {
    fn request(&self, request: BrainConfirm) -> ConfirmFuture {
        let id = request.id.clone();
        if let Ok(mut state) = self.decision.lock() {
            state.id = Some(id.clone());
            state.value = None;
            state.waker = None;
        }
        push_event(
            &self.queue,
            AgentEvent::ConfirmRequested {
                request: to_dto_request(self.scrub.clone(), request),
            },
        );
        Box::pin(WaitDecision {
            decision: self.decision.clone(),
            id,
        })
    }
}

/// 等用户裁决的 future（手写 poll：不引 tokio / futures-util 到 app-iced）。
struct WaitDecision {
    decision: DecisionSlot,
    id: String,
}

impl Future for WaitDecision {
    type Output = ConfirmDecision;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<ConfirmDecision> {
        let Ok(mut state) = self.decision.lock() else {
            // 锁中毒：fail-closed（宁可拒一次写，也不静默放行）
            return Poll::Ready(ConfirmDecision::Denied);
        };
        if state.id.as_deref() != Some(self.id.as_str()) {
            // 被别的请求顶掉了（正常不会发生：一轮只有一个待确认）：按拒绝收场
            return Poll::Ready(ConfirmDecision::Denied);
        }
        match state.value {
            Some(true) => Poll::Ready(ConfirmDecision::Approved),
            Some(false) => Poll::Ready(ConfirmDecision::Denied),
            None => {
                state.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

fn to_dto_request(scrub: Scrubber, request: BrainConfirm) -> ConfirmRequest {
    ConfirmRequest {
        id: request.id,
        tool: request.tool,
        summary: scrub.apply(&request.summary),
        details: unlucky_agent::redact::redact_args(&request.details),
        sensitivity: match request.sensitivity {
            unlucky_agent::confirm::Sensitivity::Low => Sensitivity::Low,
            unlucky_agent::confirm::Sensitivity::Medium => Sensitivity::Medium,
            unlucky_agent::confirm::Sensitivity::High => Sensitivity::High,
        },
    }
}
