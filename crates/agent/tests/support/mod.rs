//! 集成测试的公共脚手架：**mock provider** + 迷你 `block_on` + 装配好的 harness。
//!
//! `block_on` 是手写的 20 行：mock provider 的 future 永远立刻 Ready，所以不需要运行时。
//! 这样 agent 的循环测试**零 tokio 依赖**，阶段 B 换成 rig 后才需要真运行时。

#![allow(dead_code)]

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use serde_json::{json, Value};
use unlucky_agent::audit::{AuditSink, MemoryAudit};
use unlucky_agent::brain::{Brain, BrainConfig, BrainDeps};
use unlucky_agent::brain::{
    ChatProvider, ChatReply, ChatRequest, DeltaObserver, ProviderConfig, ProviderError,
    ProviderFuture, ToolCall,
};
use unlucky_agent::clock::{FixedClock, SharedClock};
use unlucky_agent::confirm::{AutoApproveGate, ConfirmGate, ConfirmPolicy, NoUiGate};
use unlucky_agent::event::{AgentEventSink, MemoryEventSink};
use unlucky_agent::registry::ToolRegistry;
use unlucky_agent::testing::{mock_ports, MockSettings};

/// 立刻完成的 future 直接 poll；挂起就是测试写错了。
pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future: Pin<Box<F>> = Box::pin(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("mock provider 不应挂起：测试里没有执行器"),
    }
}

/// 脚本化 provider：按顺序回放 [`ChatReply`]，并记录收到的请求。
pub struct MockProvider {
    config: ProviderConfig,
    script: Mutex<VecDeque<ChatReply>>,
    requests: Mutex<Vec<ChatRequest>>,
    fail_with: Option<String>,
}

impl MockProvider {
    /// 用默认模型名（本地 llama.cpp 形态）回放脚本。
    #[must_use]
    pub fn new(script: Vec<ChatReply>) -> Self {
        Self {
            config: ProviderConfig::llamacpp_local("mock-model"),
            script: Mutex::new(script.into()),
            requests: Mutex::new(Vec::new()),
            fail_with: None,
        }
    }

    /// 禁用态（测 `BrainError::Disabled`）。
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            config: ProviderConfig::disabled(),
            script: Mutex::new(VecDeque::new()),
            requests: Mutex::new(Vec::new()),
            fail_with: None,
        }
    }

    /// 每次补全都失败（测 provider 错误透传）。
    #[must_use]
    pub fn failing(message: &str) -> Self {
        Self {
            fail_with: Some(message.to_string()),
            ..Self::new(Vec::new())
        }
    }

    /// 收到的请求（断言 system prompt / 工具定义 / 回灌的工具消息）。
    #[must_use]
    pub fn requests(&self) -> Vec<ChatRequest> {
        self.requests.lock().map(|r| r.clone()).unwrap_or_default()
    }
}

impl ChatProvider for MockProvider {
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn complete<'a>(
        &'a self,
        request: &'a ChatRequest,
        observer: &'a dyn DeltaObserver,
    ) -> ProviderFuture<'a> {
        Box::pin(async move {
            if let Ok(mut requests) = self.requests.lock() {
                requests.push(request.clone());
            }
            if let Some(message) = &self.fail_with {
                return Err(ProviderError::new(message.clone()));
            }
            let reply = self
                .script
                .lock()
                .ok()
                .and_then(|mut script| script.pop_front())
                .unwrap_or_default();
            // 按 4 字符切块吐增量，模拟流式。
            if !reply.text.is_empty() {
                for chunk in reply.text.chars().collect::<Vec<_>>().chunks(4) {
                    observer.on_delta(&chunk.iter().collect::<String>());
                }
            }
            Ok(reply)
        })
    }
}

/// 装配好的测试环境。
pub struct Harness {
    /// 对话循环。
    pub brain: Brain,
    /// 审计出口（内存）。
    pub audit: Arc<MemoryAudit>,
    /// 事件出口（内存）。
    pub events: Arc<MemoryEventSink>,
    /// mock provider（查请求）。
    pub provider: Arc<MockProvider>,
    /// mock 配置端口（查是否真的写入）。
    pub settings: Arc<MockSettings>,
    /// 工具注册表。
    pub registry: Arc<ToolRegistry>,
}

impl Harness {
    /// P0 默认形态：无确认 UI（`NoUiGate`，敏感操作 fail-closed）+ 非 YOLO。
    /// 确认请求本身仍会作为 [`unlucky_agent::event::AgentEvent::ConfirmRequested`] 发出。
    #[must_use]
    pub fn new(script: Vec<ChatReply>) -> Self {
        Self::with_gate(script, ConfirmPolicy::default(), |_sink| Arc::new(NoUiGate))
    }

    /// YOLO：自动放行全部（除破坏性）。
    #[must_use]
    pub fn yolo(script: Vec<ChatReply>) -> Self {
        Self::with_gate(script, ConfirmPolicy { yolo: true }, |_sink| {
            Arc::new(AutoApproveGate)
        })
    }

    /// 用指定的（可能失效/禁用的）provider 装配，其余同 [`Harness::new`]。
    #[must_use]
    pub fn from_provider(provider: Arc<MockProvider>) -> Self {
        Self::build(provider, ConfirmPolicy::default(), |_sink| {
            Arc::new(NoUiGate)
        })
    }

    /// 自定义闸门 + 策略。
    #[must_use]
    pub fn with_gate(
        script: Vec<ChatReply>,
        policy: ConfirmPolicy,
        make_gate: impl FnOnce(Arc<dyn AgentEventSink>) -> Arc<dyn ConfirmGate>,
    ) -> Self {
        Self::build(Arc::new(MockProvider::new(script)), policy, make_gate)
    }

    fn build(
        provider: Arc<MockProvider>,
        policy: ConfirmPolicy,
        make_gate: impl FnOnce(Arc<dyn AgentEventSink>) -> Arc<dyn ConfirmGate>,
    ) -> Self {
        let (ports, settings) = mock_ports();
        let registry =
            Arc::new(unlucky_agent::tools::build_v1_registry(&ports).expect("注册表应装配成功"));
        let audit = Arc::new(MemoryAudit::new());
        let events = Arc::new(MemoryEventSink::new());
        let clock: SharedClock = Arc::new(FixedClock::with_step(1_793_314_353_123, 3));
        let gate = make_gate(events.clone());
        let deps = BrainDeps::new(
            audit.clone() as Arc<dyn AuditSink>,
            gate,
            policy,
            clock,
            events.clone() as Arc<dyn AgentEventSink>,
        );
        let brain = Brain::new(
            provider.clone(),
            Arc::clone(&registry),
            deps,
            BrainConfig::default(),
        );
        Self {
            brain,
            audit,
            events,
            provider,
            settings,
            registry,
        }
    }
}

/// 构造一条工具调用回复。
#[must_use]
pub fn tool_call_reply(id: &str, name: &str, arguments: Value) -> ChatReply {
    ChatReply::with_tool_calls(vec![ToolCall {
        id: id.to_string(),
        name: name.to_string(),
        arguments,
    }])
}

/// 便捷：游戏会话 id（与 `MockSessions::crashing_session` 一致）。
pub const SESSION: &str = "20261029T124501-9f3c2a";

/// 便捷：agent 对话会话 id。
pub const AGENT_SESSION: &str = "chat-1";

/// 便捷：`list_game_sessions` 的最小参数。
#[must_use]
pub fn list_args() -> Value {
    json!({ "limit": 5 })
}
