//! 对话循环集成测试：mock provider 驱动**完整链路**（provider → 工具编排 → 审计 → 事件）。
//!
//! 这里测"循环的骨架"：多轮工具调用、步数上限、流式增量、禁用态、provider 失败。
//! 安全边界（白名单/确认/YOLO/审计标记）在 `tools_security.rs`。

mod support;

use std::sync::Arc;

use serde_json::json;
use support::{
    block_on, list_args, tool_call_reply, Harness, MockProvider, AGENT_SESSION, SESSION,
};
use unlucky_agent::audit::AuditOutcome;
use unlucky_agent::brain::{BrainError, ChatMessage, ChatReply, StopReason, ToolCall};
use unlucky_agent::event::AgentEvent;

#[test]
fn single_turn_without_tools_returns_text() {
    let harness = Harness::new(vec![ChatReply::text("你好，需要我看看最近的崩溃吗？")]);
    let mut history = Vec::new();

    let outcome = block_on(harness.brain.run_turn(AGENT_SESSION, &mut history, "你好")).unwrap();

    assert_eq!(outcome.text, "你好，需要我看看最近的崩溃吗？");
    assert_eq!(outcome.steps, 1);
    assert_eq!(outcome.tool_calls, 0);
    assert_eq!(outcome.stopped_reason, StopReason::Completed);
    assert_eq!(history.len(), 2, "应只有 user + assistant");
    assert!(harness.audit.is_empty(), "没调工具就不该有审计记录");
}

#[test]
fn system_prompt_leads_every_request_and_tools_are_projected() {
    let harness = Harness::new(vec![ChatReply::text("好的")]);
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "看看配置"),
    )
    .unwrap();

    let requests = harness.provider.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert!(
        matches!(&request.messages[0], ChatMessage::System { content } if content.contains("Unlucky-Launcher")),
        "system prompt 必须在首位"
    );
    assert_eq!(request.model, "mock-model");
    let names: Vec<&str> = request.tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "get_config",
            "set_config",
            "get_config_schema",
            "list_instances",
            "list_versions",
            "get_java_status",
            "list_accounts",
            "list_game_sessions",
            "read_game_log",
            "diagnose_crash",
        ],
        "注册表的双投影载荷必须原样交给 provider"
    );
    // 双投影契约：provider 收到的 schema 与注册表定义逐字节相等。
    assert_eq!(request.tools, harness.registry.definitions());
}

#[test]
fn tool_call_round_trip_feeds_result_back_and_finishes() {
    let harness = Harness::new(vec![
        tool_call_reply("call_1", "list_game_sessions", list_args()),
        ChatReply::text("最近一次会话崩溃了，要不要跑诊断？"),
    ]);
    let mut history = Vec::new();

    let outcome = block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "最近崩了吗"),
    )
    .unwrap();

    assert_eq!(outcome.tool_calls, 1);
    assert_eq!(outcome.steps, 2);
    assert_eq!(outcome.stopped_reason, StopReason::Completed);

    let requests = harness.provider.requests();
    assert_eq!(requests.len(), 2);
    let tool_message = requests[1]
        .messages
        .iter()
        .find_map(|m| match m {
            ChatMessage::Tool {
                call_id,
                name,
                content,
            } => Some((call_id.clone(), name.clone(), content.clone())),
            _ => None,
        })
        .expect("必须有工具结果消息");
    assert_eq!(tool_message.0, "call_1");
    assert_eq!(tool_message.1, "list_game_sessions");
    assert!(
        tool_message.2.contains(SESSION),
        "工具结果应包含会话 id：{}",
        tool_message.2
    );

    let audits = harness.audit.records();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].tool, "list_game_sessions");
    assert_eq!(audits[0].outcome, AuditOutcome::Ok);
    assert!(!audits[0].yolo);
    assert!(!audits[0].confirmed);
}

#[test]
fn multiple_tool_calls_in_one_step_all_execute() {
    let reply = ChatReply::with_tool_calls(vec![
        ToolCall {
            id: "c1".to_string(),
            name: "list_game_sessions".to_string(),
            arguments: list_args(),
        },
        ToolCall {
            id: "c2".to_string(),
            name: "diagnose_crash".to_string(),
            arguments: json!({ "session_id": SESSION }),
        },
    ]);
    let harness = Harness::new(vec![reply, ChatReply::text("诊断完成")]);
    let mut history = Vec::new();

    let outcome = block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "查一下"),
    )
    .unwrap();
    assert_eq!(outcome.tool_calls, 2);
    assert_eq!(harness.audit.len(), 2);
    assert_eq!(
        harness
            .audit
            .records()
            .iter()
            .map(|r| r.tool.as_str())
            .collect::<Vec<_>>(),
        vec!["list_game_sessions", "diagnose_crash"]
    );
}

#[test]
fn step_limit_stops_runaway_loop() {
    // 每一步都要求调工具，永远不会给最终回答。
    let script: Vec<ChatReply> = (0..16)
        .map(|i| tool_call_reply(&format!("c{i}"), "get_config", json!({})))
        .collect();
    let harness = Harness::new(script);
    let mut history = Vec::new();

    let outcome = block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "死循环"),
    )
    .unwrap();
    assert_eq!(outcome.stopped_reason, StopReason::StepLimit);
    assert_eq!(outcome.steps, unlucky_agent::brain::DEFAULT_MAX_STEPS);
    assert_eq!(outcome.tool_calls, unlucky_agent::brain::DEFAULT_MAX_STEPS);
}

#[test]
fn streaming_deltas_are_forwarded_as_events() {
    let harness = Harness::new(vec![ChatReply::text("日志里是 NoClassDefFoundError")]);
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "什么错误"),
    )
    .unwrap();

    assert_eq!(
        harness.events.streamed_text(),
        "日志里是 NoClassDefFoundError"
    );
    let events = harness.events.events();
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::TurnStarted { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::AssistantText { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::TurnFinished { .. })));
}

#[test]
fn disabled_provider_fails_fast() {
    let harness = Harness::from_provider(Arc::new(MockProvider::disabled()));
    let mut history = Vec::new();

    let err = block_on(harness.brain.run_turn(AGENT_SESSION, &mut history, "你好")).unwrap_err();
    assert_eq!(err, BrainError::Disabled);
    assert!(err.to_string().contains("规则引擎"), "要提示降级能力");
    assert!(
        harness.provider.requests().is_empty(),
        "禁用态不该发起任何请求"
    );
    assert!(history.is_empty(), "禁用态不该改动历史");
}

#[test]
fn provider_error_surfaces_as_brain_error_and_event() {
    let harness = Harness::from_provider(Arc::new(MockProvider::failing(
        "连接 http://127.0.0.1:8080 失败",
    )));
    let mut history = Vec::new();

    let err = block_on(harness.brain.run_turn(AGENT_SESSION, &mut history, "你好")).unwrap_err();
    assert_eq!(
        err,
        BrainError::Provider(unlucky_agent::brain::ProviderError::new(
            "连接 http://127.0.0.1:8080 失败"
        ))
    );
    assert!(harness
        .events
        .events()
        .iter()
        .any(|e| matches!(e, AgentEvent::Error { message } if message.contains("8080"))));
}

#[test]
fn empty_script_yields_empty_completion_turn() {
    let harness = Harness::new(vec![]);
    let mut history = Vec::new();
    let outcome = block_on(harness.brain.run_turn(AGENT_SESSION, &mut history, "你好")).unwrap();
    assert_eq!(outcome.text, "");
    assert_eq!(outcome.stopped_reason, StopReason::Completed);
}
