//! host/logs.rs 的单测（拆文件守 400 行上限）。

use super::*;

#[test]
fn time_display_and_outcomes() {
    assert_eq!(display_time("2026-10-29T19:22:45.123Z"), "10-29 19:22");
    assert_eq!(display_time("nope"), "nope");
    let mut entry = SessionIndexEntry {
        v: 1,
        session_id: "s".to_string(),
        instance_id: String::new(),
        instance_name: String::new(),
        game_version: String::new(),
        started_at: String::new(),
        ended_at: "2026-10-29T19:22:45Z".to_string(),
        duration_ms: 1000,
        exit_code: Some(0),
        exit_type: Some("normal".to_string()),
        silent_failure: false,
        window_detected: true,
        event_count: 1,
        log_path: "s.jsonl".to_string(),
        crash_found: false,
        diagnosis_top: None,
    };
    assert_eq!(outcome_of(&entry), LogOutcome::Ok);
    entry.exit_code = Some(1);
    assert_eq!(outcome_of(&entry), LogOutcome::Crashed);
    entry.silent_failure = true;
    assert_eq!(outcome_of(&entry), LogOutcome::SilentFail);
}

#[test]
fn lines_map_levels_and_skip_non_line_events() {
    let stdout = serde_json::json!({"kind":"stdout","payload":{"line":"hi","level":"info"}});
    assert_eq!(line_of(&stdout).unwrap().level, LogLevel::Info);
    let stderr = serde_json::json!({"kind":"stderr","payload":{"line":"boom","level":"error"}});
    assert_eq!(line_of(&stderr).unwrap().level, LogLevel::Error);
    let other = serde_json::json!({"kind":"cmd_built","payload":{"argv":[]}});
    assert!(line_of(&other).is_none());
}

#[test]
fn tail_only_consumes_complete_lines() {
    let dir = std::env::temp_dir().join(format!("ull-host-tail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("目录");
    let path = dir.join("s.jsonl");
    std::fs::write(
        &path,
        "{\"kind\":\"stdout\",\"payload\":{\"line\":\"one\"}}\n",
    )
    .expect("写");
    let mut tail = Tail::default();
    tail.attach(path.clone(), "s");
    let (session, lines) = tail.poll().expect("首拍有新行");
    assert_eq!(session, "s");
    assert_eq!(lines.len(), 1);
    assert!(tail.poll().is_none(), "没有新增就不该再产出");

    // 半行不消费
    let mut text = std::fs::read_to_string(&path).expect("读");
    text.push_str("{\"kind\":\"stdout\",\"payload\":{\"line\":\"tw");
    std::fs::write(&path, text).expect("写");
    assert!(tail.poll().is_none(), "半个 JSON 行不能解析");
    let mut text = std::fs::read_to_string(&path).expect("读");
    text.push_str("o\"}}\n");
    std::fs::write(&path, text).expect("写");
    let (_, lines) = tail.poll().expect("补全后应产出");
    assert_eq!(lines[0].text, "two");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn crash_import_reports_missing_file() {
    let err = analyze_crash_file("D:/definitely/missing/hs_err_pid1.log").unwrap_err();
    assert!(err.contains("找不到崩溃日志"), "{err}");
}

#[test]
fn confidence_percent_is_rounded_and_clamped() {
    assert_eq!(confidence_percent(0.923), 92);
    assert_eq!(confidence_percent(-1.0), 0);
    assert_eq!(confidence_percent(2.0), 100);
}
