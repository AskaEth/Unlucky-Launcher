//! 安全边界集成测试：白名单拦截、分级确认、YOLO、审计标记。
//!
//! 这些是 02 文档 §3.1「安全边界」+ §4b「YOLO」与 M12 `setting-接口约定.md` §8
//! （`Low/Medium/High` 三档 + `writable=false` 凭据）的可执行版本；
//! 判断口径对照 research/02 §7 R9（写操作的破坏半径）。

mod support;

use std::sync::Arc;

use serde_json::{json, Value};
use support::{block_on, tool_call_reply, Harness, AGENT_SESSION};
use unlucky_agent::audit::AuditOutcome;
use unlucky_agent::brain::ChatReply;
use unlucky_agent::confirm::{ConfirmDecision, ConfirmPolicy, ScriptedGate};
use unlucky_agent::event::AgentEvent;

fn set_config_reply(path: &str, value: Value) -> ChatReply {
    tool_call_reply("c1", "set_config", json!({ "path": path, "value": value }))
}

fn tool_results(harness: &Harness) -> Vec<String> {
    harness
        .provider
        .requests()
        .last()
        .map(|request| {
            request
                .messages
                .iter()
                .filter_map(|m| match m {
                    unlucky_agent::brain::ChatMessage::Tool { content, .. } => {
                        Some(content.clone())
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn confirm_events(harness: &Harness) -> usize {
    harness
        .events
        .events()
        .iter()
        .filter(|e| matches!(e, AgentEvent::ConfirmRequested { .. }))
        .count()
}

#[test]
fn low_key_writes_silently_without_confirmation() {
    let harness = Harness::new(vec![
        set_config_reply("appearance.theme_mode", json!("metro_light")),
        ChatReply::text("已切换主题"),
    ]);
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "换成浅色"),
    )
    .unwrap();

    assert_eq!(harness.settings.set_calls(), 1);
    assert_eq!(
        harness.settings.value_of("appearance.theme_mode"),
        Some(json!("metro_light"))
    );
    assert_eq!(confirm_events(&harness), 0, "Low 键不该打扰用户");

    let records = harness.audit.records_for("set_config");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, AuditOutcome::Ok);
    assert!(!records[0].confirmed, "静默生效不是「用户确认」");
    assert!(!records[0].yolo);
}

#[test]
fn medium_key_also_writes_silently() {
    // M12：Medium = 静默生效但审计标记（不弹窗）
    let harness = Harness::new(vec![
        set_config_reply("download.region", json!("cn")),
        ChatReply::text("已切到国内源"),
    ]);
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "换下载源"),
    )
    .unwrap();

    assert_eq!(harness.settings.set_calls(), 1);
    assert_eq!(confirm_events(&harness), 0);
    assert_eq!(
        harness.audit.records_for("set_config")[0].outcome,
        AuditOutcome::Ok
    );
}

#[test]
fn high_key_is_fail_closed_without_ui() {
    let harness = Harness::new(vec![
        set_config_reply("java.selected_path", json!("D:/jdk17/bin/java.exe")),
        ChatReply::text("需要你确认"),
    ]);
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "换 Java"),
    )
    .unwrap();

    assert_eq!(
        harness.settings.set_calls(),
        0,
        "P0 无 UI 时必须 fail-closed"
    );
    assert_eq!(harness.settings.value_of("java.selected_path"), None);
    assert_eq!(confirm_events(&harness), 1, "确认请求必须作为事件发出");

    let outcomes: Vec<AuditOutcome> = harness
        .audit
        .records_for("set_config")
        .iter()
        .map(|r| r.outcome)
        .collect();
    assert_eq!(
        outcomes,
        vec![AuditOutcome::AwaitingConfirmation, AuditOutcome::Denied],
        "先记「AI 想改」，再记「被拒绝」"
    );
    assert!(tool_results(&harness)
        .iter()
        .any(|content| content.contains("denied")));
    assert!(harness
        .events
        .events()
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolDenied { .. })));
}

#[test]
fn user_approval_writes_and_marks_confirmed() {
    let harness = Harness::with_gate(
        vec![
            set_config_reply("java.selected_path", json!("D:/jdk17/bin/java.exe")),
            ChatReply::text("已更新"),
        ],
        ConfirmPolicy::default(),
        |_sink| {
            Arc::new(ScriptedGate::new(
                [ConfirmDecision::Approved],
                ConfirmDecision::Denied,
            ))
        },
    );
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "换 Java"),
    )
    .unwrap();

    assert_eq!(harness.settings.set_calls(), 1, "确认后只写一次");
    let records = harness.audit.records_for("set_config");
    let last = records.last().expect("必须有审计记录");
    assert_eq!(last.outcome, AuditOutcome::Ok);
    assert!(last.confirmed, "用户点了允许");
    assert!(!last.yolo);
}

#[test]
fn yolo_skips_confirm_but_audits_flag() {
    let harness = Harness::yolo(vec![
        set_config_reply("java.selected_path", json!("D:/jdk17/bin/java.exe")),
        ChatReply::text("已更新"),
    ]);
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "换 Java"),
    )
    .unwrap();

    assert_eq!(harness.settings.set_calls(), 1);
    assert_eq!(confirm_events(&harness), 0, "YOLO 不打扰用户");
    let records = harness.audit.records_for("set_config");
    assert_eq!(records.len(), 1, "YOLO 只留结局记录");
    assert!(records[0].yolo, "审计必须标记 yolo");
    assert!(!records[0].confirmed, "自动放行不是用户确认");
    assert_eq!(records[0].outcome, AuditOutcome::Ok);
}

#[test]
fn whitelist_violation_is_rejected_and_audited() {
    let harness = Harness::new(vec![
        set_config_reply("java.home.mystery", json!("x")),
        ChatReply::text("换个方案"),
    ]);
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "改个不存在的键"),
    )
    .unwrap();

    assert_eq!(harness.settings.set_calls(), 0);
    let records = harness.audit.records_for("set_config");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, AuditOutcome::ToolError);
    let results = tool_results(&harness);
    assert!(
        results.iter().any(|c| c.contains("白名单")),
        "模型需要知道为什么被拒：{results:?}"
    );
}

#[test]
fn invalid_args_are_rejected_before_any_side_effect() {
    // 缺 value（schema required）
    let harness = Harness::new(vec![
        tool_call_reply(
            "c1",
            "set_config",
            json!({ "path": "appearance.theme_mode" }),
        ),
        ChatReply::text("参数补上"),
    ]);
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "改主题"),
    )
    .unwrap();

    assert_eq!(harness.settings.set_calls(), 0);
    let records = harness.audit.records_for("set_config");
    assert_eq!(records[0].outcome, AuditOutcome::InvalidArgs);
    assert!(tool_results(&harness)
        .iter()
        .any(|c| c.contains("缺少必填参数")));

    // 多余参数（additionalProperties: false）
    let harness = Harness::new(vec![
        tool_call_reply(
            "c2",
            "read_game_log",
            json!({ "session_id": "x", "path": "evil" }),
        ),
        ChatReply::text("好"),
    ]);
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "读日志"),
    )
    .unwrap();
    let records = harness.audit.records_for("read_game_log");
    assert_eq!(records[0].outcome, AuditOutcome::InvalidArgs);
}

#[test]
fn unknown_tool_lists_available_names() {
    let harness = Harness::new(vec![
        tool_call_reply("c1", "delete_everything", json!({})),
        ChatReply::text("抱歉"),
    ]);
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "删库跑路"),
    )
    .unwrap();

    let records = harness.audit.records_for("delete_everything");
    assert_eq!(records[0].outcome, AuditOutcome::UnknownTool);
    let results = tool_results(&harness);
    assert!(
        results.iter().any(|c| c.contains("get_config")),
        "{results:?}"
    );
}

#[test]
fn locked_credential_key_is_refused_even_under_yolo() {
    // M12：ai.api_key 是 writable=false（不在白名单）→ YOLO 也不能放宽可写范围
    let harness = Harness::yolo(vec![
        set_config_reply("ai.api_key", json!("sk-super-secret-value")),
        ChatReply::text("换不了"),
    ]);
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "换 key"),
    )
    .unwrap();

    assert_eq!(harness.settings.set_calls(), 0);
    let records = harness.audit.records_for("set_config");
    assert_eq!(records[0].outcome, AuditOutcome::ToolError);

    let args = serde_json::to_string(&records[0].args).unwrap();
    assert!(
        !args.contains("sk-super-secret-value"),
        "审计绝不能落明文密钥：{args}"
    );
    assert!(args.contains("***"), "{args}");
}

#[test]
fn get_config_refuses_credential_keys() {
    // F1：模型可自由给 path，凭据键必须读不到 —— 否则值会进模型上下文发往远端 LLM。
    let harness = Harness::new(vec![
        tool_call_reply("c1", "get_config", json!({ "path": "ai.api_key" })),
        ChatReply::text("读不到"),
    ]);
    // 模拟 M12 真实实现：凭据键在文件里确实有值（writable=false，不在白名单也要能防住读）
    harness
        .settings
        .seed_value("ai.api_key", json!("sk-live-credential-value"));
    let mut history = Vec::new();
    block_on(
        harness
            .brain
            .run_turn(AGENT_SESSION, &mut history, "看看我的 key"),
    )
    .unwrap();

    // 1) 工具报错并被审计
    let records = harness.audit.records_for("get_config");
    assert_eq!(records[0].outcome, AuditOutcome::ToolError);
    // 2) 模型拿到的是拒绝文案，不是值
    let results = tool_results(&harness);
    assert!(
        results.iter().any(|c| c.contains("凭据")),
        "应给出可读拒绝：{results:?}"
    );
    assert!(
        !results
            .iter()
            .any(|c| c.contains("sk-live-credential-value")),
        "工具结果绝不能带凭据：{results:?}"
    );
    // 3) 事件流与对话历史里也不能出现（P1 事件会落盘 / 历史会持久化）
    let leaked_in_events = harness
        .events
        .events()
        .iter()
        .any(|e| format!("{e:?}").contains("sk-live-credential-value"));
    assert!(!leaked_in_events, "事件流不能带凭据明文");
    let leaked_in_history = history
        .iter()
        .any(|m| format!("{m:?}").contains("sk-live-credential-value"));
    assert!(!leaked_in_history, "对话历史不能带凭据明文");
}
