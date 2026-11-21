//! 端到端：真端口 + 十个真工具 + 真 JSONL 审计，**不用网络**（provider 是回放件）。
//!
//! 从 `real/tests.rs` 拆出来的唯一理由是 400 行红线（`guards.yml` 的 file-size 门禁）。

use std::path::Path;
use std::sync::atomic::Ordering;

use serde_json::json;
use unlucky_agent::brain::chat::ChatReply;
use unlucky_agent::brain::rig::RigProvider;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;
use unlucky_presenter::agent::{AgentRail, AgentTransport};
use unlucky_presenter::dto::AgentEvent as DtoEvent;

use unlucky_cli::context::Ctx;

use super::*;

/// 造一条「崩过」的游戏会话（索引 + 事件流），供 `read_game_log` / `diagnose_crash` 读。
fn fake_crashing_session(data: &Path, session_id: &str) {
    let ctx = Ctx::resolve_in(
        &unlucky_cli::cli::Globals {
            data_dir: Some(data.to_path_buf()),
            ..unlucky_cli::cli::Globals::default()
        },
        None,
    )
    .expect("数据根");
    let root = ctx.sessions_root();
    let game = root.join("game");
    std::fs::create_dir_all(&game).expect("建 game 目录");
    let entry = json!({
        "v": 1,
        "session_id": session_id,
        "instance_id": "inst-1",
        "instance_name": "Fabric 1.21",
        "game_version": "1.21.1",
        "started_at": "2026-11-18T10:00:00Z",
        "ended_at": "2026-11-18T10:02:00Z",
        "duration_ms": 120000u64,
        "exit_code": 1,
        "exit_type": "crash",
        "silent_failure": false,
        "window_detected": true,
        "event_count": 3u64,
        "log_path": format!("game/{session_id}.jsonl"),
        "crash_found": true,
        "diagnosis_top": "fabric_loader_missing"
    });
    std::fs::write(root.join("game").join("index.jsonl"), format!("{entry}\n")).expect("写索引");
    let events = [
        json!({ "seq": 1, "ts": "2026-11-18T10:00:01Z", "t_ms": 1000, "kind": "stderr", "payload": { "line": "java.lang.NoClassDefFoundError: net/fabricmc/loader/api/FabricLoader" } }),
        json!({ "seq": 2, "ts": "2026-11-18T10:00:02Z", "t_ms": 2000, "kind": "crash_found", "payload": { "file": "crash-reports/crash-2026-11-18.txt" } }),
        json!({ "seq": 3, "ts": "2026-11-18T10:02:00Z", "t_ms": 120000, "kind": "exit", "payload": { "code": 1 } }),
    ];
    let body: String = events
        .iter()
        .map(|event| format!("{event}\n"))
        .collect::<Vec<_>>()
        .concat();
    std::fs::write(game.join(format!("{session_id}.jsonl")), body).expect("写事件流");
}

#[test]
fn a_gui_turn_reads_a_crash_log_through_the_real_tool_kernel() {
    let session_id = "20261118T100000-abc123";
    let (mut transport, home, data, calls) = harness(
        "read-log",
        vec![
            tool_call(
                "read_game_log",
                json!({ "session_id": session_id, "kinds": ["stderr"] }),
            ),
            ChatReply::text("崩在 Fabric loader 缺类：确认实例装的是 Fabric 而不是原版。"),
        ],
        json!({}),
    );
    fake_crashing_session(&data, session_id);

    transport
        .submit("我上次启动崩了，看看日志", &AgentRail::new())
        .expect("提交应成功（事件稍后回流）");
    let events = pump(&transport, |events| {
        has_tool_finished(events, "read_game_log", true) && !last_assistant_text(events).is_empty()
    });

    assert_eq!(
        calls.load(Ordering::Relaxed),
        2,
        "两轮补全：先要工具，拿到结果再说话"
    );
    let text = last_assistant_text(&events);
    assert!(
        text.contains("Fabric loader"),
        "模型的最终回答要进会话：{text}"
    );

    // 审计：`<data root>/agent/audit/audit-<date>.jsonl` 里应有这次工具调用的记录
    let audited = audit_lines(&home);
    let record = audited
        .iter()
        .find(|line| line["tool"] == "read_game_log")
        .unwrap_or_else(|| panic!("审计里没有 read_game_log：{audited:#?}"));
    assert_eq!(record["outcome"], "ok");
    assert!(
        record["agent_session_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("gui-")),
        "审计要记 agent 会话 id：{record:#?}"
    );
}

#[test]
fn a_gui_turn_writes_one_config_key_through_the_real_tool_kernel() {
    let (mut transport, home, data, _calls) = harness(
        "set-config",
        vec![
            tool_call(
                "set_config",
                json!({ "path": "download.concurrency", "value": 12 }),
            ),
            ChatReply::text("下载并发已经改成 12。"),
        ],
        json!({}),
    );

    transport
        .submit("把下载并发改成 12", &AgentRail::new())
        .expect("提交应成功");
    let events = pump(&transport, |events| {
        has_tool_finished(events, "set_config", true) && !last_assistant_text(events).is_empty()
    });
    assert!(
        has_tool_finished(&events, "set_config", true),
        "{events:#?}"
    );

    // 真落盘：临时数据根的 launcher.json 里这一项就是 12（低敏感键不问人）
    let ctx = Ctx::resolve_in(
        &unlucky_cli::cli::Globals {
            data_dir: Some(data.clone()),
            ..unlucky_cli::cli::Globals::default()
        },
        None,
    )
    .expect("数据根");
    let store = SettingStore::load(ctx.launcher_config(), launcher_schema());
    assert_eq!(
        store.doc().effective("download.concurrency"),
        Some(&json!(12)),
        "写配置要真的落盘（不是只在会话里说一句）"
    );

    let audited = audit_lines(&home);
    let record = audited
        .iter()
        .find(|line| line["tool"] == "set_config")
        .unwrap_or_else(|| panic!("审计里没有 set_config：{audited:#?}"));
    assert_eq!(record["outcome"], "ok");
    assert_eq!(record["yolo"], false, "非 YOLO 下要标 false");
}

#[test]
fn a_high_sensitivity_key_asks_the_user_before_it_writes() {
    let (mut transport, home, data, _calls) = harness(
        "confirm",
        vec![
            tool_call(
                "set_config",
                json!({ "path": "instances.mode", "value": "shared" }),
            ),
            ChatReply::text("默认隔离模式改成 shared 了。"),
        ],
        json!({}),
    );

    transport
        .submit("实例默认用 shared 模式", &AgentRail::new())
        .expect("提交应成功");
    // 先等到确认请求（这一步就是「敏感键要问人」）
    let events = pump(&transport, |events| {
        dto_events(events)
            .iter()
            .any(|event| matches!(event, DtoEvent::ConfirmRequested { .. }))
    });
    let request_id = dto_events(&events)
        .iter()
        .find_map(|event| match event {
            DtoEvent::ConfirmRequested { request } => Some(request.id.clone()),
            _ => None,
        })
        .expect("要有确认请求");

    // 用户还没点之前，写操作不该落地
    let ctx = Ctx::resolve_in(
        &unlucky_cli::cli::Globals {
            data_dir: Some(data.clone()),
            ..unlucky_cli::cli::Globals::default()
        },
        None,
    )
    .expect("数据根");
    let read_mode = || {
        let store = SettingStore::load(ctx.launcher_config(), launcher_schema());
        store.doc().effective("instances.mode").cloned()
    };
    assert_ne!(read_mode(), Some(json!("shared")), "没点允许之前不许写进去");

    transport.confirm(&request_id, true);
    let events = pump(&transport, |events| {
        has_tool_finished(events, "set_config", true)
    });
    assert!(
        has_tool_finished(&events, "set_config", true),
        "{events:#?}"
    );
    assert_eq!(read_mode(), Some(json!("shared")), "允许之后才落盘");

    // 审计里两条：awaiting_confirmation + ok(confirmed=true)
    let audited = audit_lines(&home);
    let outcomes: Vec<&str> = audited
        .iter()
        .filter(|line| line["tool"] == "set_config")
        .filter_map(|line| line["outcome"].as_str())
        .collect();
    assert!(
        outcomes.contains(&"awaiting_confirmation"),
        "「AI 想改 X」本身必须留痕：{audited:#?}"
    );
    let confirmed = audited
        .iter()
        .find(|line| line["tool"] == "set_config" && line["outcome"] == "ok")
        .expect("要有一条 ok");
    assert_eq!(confirmed["confirmed"], true, "用户亲手点的允许");
    assert_eq!(confirmed["yolo"], false);
}

#[test]
fn an_unreachable_endpoint_reports_a_named_failure_and_keeps_the_rail_usable() {
    // 真 provider（RigProvider）、真端点（一个必然连不上的回环端口）、真超时路径。
    // 这条用例**不碰模型**：它验的是「错误分类 → 人话横幅」这一段。
    let home = temp_dir("unreachable-home");
    let config = write_launcher(
        &home,
        json!({ "ai": { "enabled": true, "provider": "openai_compatible", "base_url": "http://127.0.0.1:1/v1", "model": "m" } }),
    );
    let plan = Plan::at(&config).expect("配置完整");
    let transport = RealTransport::spawn(
        plan,
        temp_ports(&temp_dir("unreachable-data")),
        RigProvider::factory(),
    );
    let mut transport = transport;

    transport
        .submit("在吗", &AgentRail::new())
        .expect("提交本身应成功（失败在事件里）");
    let events = pump(&transport, |events| {
        dto_events(events)
            .iter()
            .any(|event| matches!(event, DtoEvent::Failed { .. }))
    });

    let message = dto_events(&events)
        .iter()
        .find_map(|event| match event {
            DtoEvent::Failed { message } => Some(message.clone()),
            _ => None,
        })
        .expect("要有 Failed 事件");
    assert!(!message.trim().is_empty(), "错误不许是空串");
    // 会话仍可用：一轮失败不该把 rail 锁死（presenter 收敛流式标记）
    let mut rail = AgentRail::new();
    for event in dto_events(&events) {
        rail.ingest(event);
    }
    assert!(!rail.busy, "失败后 busy 必须收敛，否则输入框永远禁用");
    assert!(rail.error.is_some(), "横幅要有原因");
}
