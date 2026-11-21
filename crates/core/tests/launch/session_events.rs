//! 会话事件流（JSONL）与索引测试。
//!
//! 校验 `DevDocs/SubAgentsTalk/launch-接口约定.md` 的硬约定：
//! `seq` 单调、写前脱敏、索引同 id 取最后一行。

use std::sync::Arc;

use serde_json::json;
use unlucky_core::launch::pump::LineSink;

use crate::prelude::*;

fn command_with_token() -> AssembledCommand {
    let (mut request, ctx) = legacy_forge_request();
    request.manifest.legacy_arguments =
        Some("--accessToken ${auth_session} --username ${auth_player_name}".to_string());
    assemble(&request, &ctx).expect("组装应成功")
}

#[test]
fn session_stream_records_all_event_kinds() {
    let root = temp_sessions_root("events");
    let ctx = PlatformCtx::new(OsFamily::Windows, "x86_64");
    let meta = SessionMeta::new("inst-1", "1.12.2-Forge", "1.12.2")
        .with_launcher("UnluckyLauncher", "0.1.0")
        .with_platform(&ctx);
    let redactor = Redactor::new().with_secret("SECRET-TOKEN-123456");
    let mut writer = SessionWriter::create_with_id(&root, "20261029T120000-abcdef", meta, redactor)
        .expect("创建会话");

    writer.session_start().expect("session_start");
    writer.cmd_built(&command_with_token()).expect("cmd_built");
    writer.proc_start(4242).expect("proc_start");
    writer
        .line(&LogLine::new(Stream::Stdout, "LWJGL Version: 3.3.1"))
        .expect("stdout");
    writer
        .line(&LogLine::new(
            Stream::Stderr,
            "java.lang.IllegalStateException: boom",
        ))
        .expect("stderr");
    writer
        .window_detected(&unlucky_core::launch::window_detect::Detection {
            heuristic: "lwjgl_version".to_string(),
            took_ms: 820,
            evidence: "LWJGL Version: 3.3.1".to_string(),
        })
        .expect("window_detected");

    let lines = vec![LogLine::new(Stream::Stderr, "java.lang.Exception: boom")];
    let analysis = exit_analysis::analyze(&exit_analysis::ExitSignals::new(1, 40_000, &lines));
    writer.exit(&analysis).expect("exit");
    writer
        .diagnosis(json!({
            "source": "rules",
            "needs_more_info": false,
            "candidates": [{ "id": "mixin_apply_failed", "confidence": 0.9 }],
        }))
        .expect("diagnosis");

    let entry = writer.finish(EndReason::Exit).expect("session_end");
    assert!(writer.is_finished());
    assert_eq!(entry.session_id, "20261029T120000-abcdef");
    assert_eq!(entry.exit_code, Some(1));
    assert_eq!(entry.exit_type.as_deref(), Some("application_error"));
    assert!(entry.window_detected);
    assert_eq!(entry.diagnosis_top.as_deref(), Some("mixin_apply_failed"));
    assert_eq!(entry.log_path, "game/20261029T120000-abcdef.jsonl");

    let events = read_events(writer.path());
    let kinds: Vec<&str> = events
        .iter()
        .filter_map(|event| event.get("kind").and_then(|value| value.as_str()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            "session_start",
            "cmd_built",
            "proc_start",
            "stdout",
            "stderr",
            "window_detected",
            "exit",
            "diagnosis",
            "session_end",
        ]
    );

    // seq 从 0 单调递增，v/ts/t_ms/session_id 齐全。
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event["v"], 1);
        assert_eq!(event["seq"], index as u64);
        assert_eq!(event["session_id"], "20261029T120000-abcdef");
        assert!(event["ts"].as_str().unwrap_or_default().ends_with('Z'));
        assert!(event["t_ms"].is_u64());
    }
    assert_eq!(entry.event_count, events.len() as u64);

    // 索引落盘且可读回。
    let index = session_index::index_path(&root);
    let listed = session_index::read_all(&index).expect("读索引");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].session_id, entry.session_id);
}

#[test]
fn cmd_built_argv_is_redacted_before_writing() {
    let root = temp_sessions_root("redact");
    let meta = SessionMeta::new("inst-1", "1.12.2-Forge", "1.12.2");
    let redactor = Redactor::new().with_secret("SECRET-TOKEN-123456");
    let mut writer =
        SessionWriter::create_with_id(&root, "redact-1", meta, redactor).expect("创建会话");
    writer.cmd_built(&command_with_token()).expect("cmd_built");

    let raw = std::fs::read_to_string(writer.path()).expect("读文件");
    assert!(!raw.contains("SECRET-TOKEN-123456"), "命令行必须脱敏后落盘");
    assert!(raw.contains(TOKEN_PLACEHOLDER));

    let events = read_events(writer.path());
    let argv = events[0]["payload"]["argv"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(argv
        .iter()
        .any(|arg| arg.as_str() == Some(TOKEN_PLACEHOLDER)));
}

#[test]
fn diagnosis_payload_is_redacted_before_writing() {
    let root = temp_sessions_root("diagnosis-redact");
    let meta = SessionMeta::new("inst-1", "1.20.1", "1.20.1");
    let redactor = Redactor::new()
        .with_secret("SECRET-TOKEN-123456")
        .with_home_str("C:/Users/aska");
    let mut writer =
        SessionWriter::create_with_id(&root, "diag-redact-1", meta, redactor).expect("创建会话");
    writer
        .diagnosis(json!({
            "source": "rules",
            "needs_more_info": false,
            "candidates": [{
                "id": "mixin_failed",
                "confidence": 0.9,
                "message_key": "launch.crash.reason.mixin_failed",
                "evidence": [
                    "hs_err:C:/Users/aska/.minecraft/hs_err_pid123.log: java.lang.Exception",
                    "token=SECRET-TOKEN-123456",
                ],
            }],
        }))
        .expect("写 diagnosis");

    let raw = std::fs::read_to_string(writer.path()).expect("读文件");
    assert!(!raw.contains("C:/Users/aska"), "诊断证据里的主目录必须脱敏");
    assert!(
        !raw.contains("SECRET-TOKEN-123456"),
        "诊断证据里的凭据必须脱敏"
    );
    assert!(raw.contains("<home>"));
    assert!(raw.contains("<redacted:token>"), "同一个值内也允许混合脱敏");

    // 字段名/数值/布尔原样保留（launch 只脱敏、不改写字段）。
    let events = read_events(writer.path());
    let payload = &events[0]["payload"];
    assert_eq!(payload["needs_more_info"], json!(false));
    assert_eq!(payload["candidates"][0]["id"], json!("mixin_failed"));
    assert_eq!(payload["candidates"][0]["confidence"], json!(0.9));
    assert_eq!(
        payload["candidates"][0]["message_key"],
        json!("launch.crash.reason.mixin_failed")
    );
    assert_eq!(payload["source"], json!("rules"));
    // diagnosis_top 仍能从脱敏后的 candidates 取出。
    assert_eq!(
        writer
            .finish(EndReason::Exit)
            .expect("结束会话")
            .diagnosis_top
            .as_deref(),
        Some("mixin_failed")
    );
}

#[test]
fn index_keeps_last_line_per_session_id() {
    let root = temp_sessions_root("index");
    let mut entry = session_index::SessionIndexEntry {
        v: 1,
        session_id: "same-id".to_string(),
        instance_id: "inst-1".to_string(),
        instance_name: "one".to_string(),
        game_version: "1.20.1".to_string(),
        started_at: "2026-10-29T12:00:00.000Z".to_string(),
        ended_at: "2026-10-29T12:01:00.000Z".to_string(),
        duration_ms: 60_000,
        exit_code: Some(0),
        exit_type: Some("normal".to_string()),
        ..session_index::SessionIndexEntry::default()
    };
    let index = session_index::index_path(&root);
    session_index::append(&index, &entry).expect("第一次写索引");

    // 崩溃重启后重写同一会话的索引行。
    entry.exit_code = Some(1);
    entry.exit_type = Some("application_error".to_string());
    entry.duration_ms = 30_000;
    session_index::append(&index, &entry).expect("第二次写索引");

    let listed = session_index::read_all(&index).expect("读索引");
    assert_eq!(listed.len(), 1, "同 id 只保留一条");
    assert_eq!(listed[0].exit_code, Some(1));
    assert_eq!(listed[0].duration_ms, 30_000);

    // 追加另一实例的会话，按实例过滤生效。
    let mut other = listed[0].clone();
    other.session_id = "other-id".to_string();
    other.instance_id = "inst-2".to_string();
    other.started_at = "2026-10-30T12:00:00.000Z".to_string();
    session_index::append(&index, &other).expect("第三次写索引");
    let only = session_index::list(&index, 10, Some("inst-2")).expect("过滤");
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].session_id, "other-id");
    // 倒序：最新的在前。
    let all = session_index::list(&index, 10, None).expect("全部");
    assert_eq!(
        all.first().map(|item| item.session_id.as_str()),
        Some("other-id")
    );
}

#[test]
fn session_sink_writes_lines_and_detects_window() {
    let root = temp_sessions_root("sink");
    let meta = SessionMeta::new("inst-1", "1.20.1", "1.20.1");
    let writer =
        SessionWriter::create_with_id(&root, "sink-1", meta, Redactor::new()).expect("创建会话");
    let path = writer.path().to_path_buf();
    let sink = Arc::new(SessionSink::new(writer));

    sink.on_line(&LogLine::new(Stream::Stdout, "preparing"));
    sink.on_line(&LogLine::new(Stream::Stdout, "LWJGL Version: 3.3.1"));
    sink.on_line(&LogLine::new(Stream::Stderr, "warn: something"));

    let kinds: Vec<String> = read_events(&path)
        .iter()
        .filter_map(|event| event["kind"].as_str().map(str::to_string))
        .collect();
    assert!(
        kinds.contains(&"window_detected".to_string()),
        "sink 应自动检测窗口"
    );
    assert_eq!(kinds.iter().filter(|kind| *kind == "stdout").count(), 2);
    assert_eq!(kinds.iter().filter(|kind| *kind == "stderr").count(), 1);
}
