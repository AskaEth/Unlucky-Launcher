//! host/logs.rs 的单测（拆文件守 400 行上限）。

use super::*;
use unlucky_core::launch::session_index;

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

// ---------------------------------------------------------------- M188：会话日志搬家

/// 一个临时数据根（`<root>/.ULL` + `<root>/.minecraft`）。
fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ull-m188-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// 索引条目夹具。`log_path` **必须是生产写入的形状**（相对会话根：`game/<id>.jsonl`）。
///
/// M211/D2：这里以前传不带 `game/` 的 `<id>.jsonl`，于是「读取侧多 join 一层 `game/`」的 bug
/// 在夹具里永远看不见——夹具和生产各自自洽，两边都是绿的。改夹具比改路径更重要。
fn sample_entry(id: &str, file: &str, started_at: &str) -> SessionIndexEntry {
    SessionIndexEntry {
        v: 1,
        session_id: id.to_string(),
        instance_id: "inst".to_string(),
        instance_name: "实例".to_string(),
        game_version: "1.20.1".to_string(),
        started_at: started_at.to_string(),
        ended_at: String::new(),
        duration_ms: 0,
        exit_code: Some(0),
        exit_type: Some("normal".to_string()),
        silent_failure: false,
        window_detected: true,
        event_count: 1,
        log_path: file.to_string(),
        crash_found: false,
        diagnosis_top: None,
    }
}

/// 守门①前半：**旧家的会话仍然列得出来、打得开**（只读兼容）。
///
/// 变异证明：
/// - 把 [`entries`] 改成只读 `root(env)`（新根）⇒ 列表里没有 `old-session`，本条红；
/// - 把 `event_path` 的新旧回落删掉（只认新根）⇒ `open` 拿到 0 行，本条红。
#[test]
fn legacy_sessions_are_still_listed_and_openable() {
    let root = temp_root("legacy");
    let env = HostEnv::at_root(&root);
    let legacy_game = env.legacy_sessions_root().join("game");
    std::fs::create_dir_all(&legacy_game).expect("建旧家目录");
    let entry = sample_entry(
        "old-session",
        "game/old-session.jsonl",
        "2026-11-01T10:00:00.000Z",
    );
    session_index::append(
        &session_index::index_path(&env.legacy_sessions_root()),
        &entry,
    )
    .expect("写旧索引");
    std::fs::write(
        legacy_game.join("old-session.jsonl"),
        "{\"kind\":\"stdout\",\"payload\":{\"line\":\"旧会话这一行\"}}\n",
    )
    .expect("写旧事件流");

    let listed = sessions(&env);
    assert!(
        listed.iter().any(|session| session.id == "old-session"),
        "旧家的会话没有出现在列表里：{listed:?}"
    );
    assert_eq!(
        event_path(&env, &entry),
        legacy_game.join("old-session.jsonl")
    );
    let (lines, _) = open(&env, "old-session");
    assert!(
        lines.iter().any(|line| line.text.contains("旧会话这一行")),
        "旧家的事件流没读出来：{lines:?}"
    );
    // 「兼容」不等于「复制一份到新根」：旧文件原地不动、新根里没有它。
    assert!(legacy_game.join("old-session.jsonl").is_file());
    assert!(!root.join(".ULL/logs/game/old-session.jsonl").is_file());
    std::fs::remove_dir_all(&root).ok();
}

/// 守门①后半：**新会话写在 `logs/game/`**，索引与事件流都在那儿。
///
/// 变异证明：把 `HostEnv::sessions_root` 改回 `layout.agent_sessions()` ⇒ 第一条断言红；
/// 把 [`entries`] 改成只读旧家 ⇒ 列表里没有 `fresh`，本条红。
#[test]
fn new_sessions_are_written_under_logs_game() {
    let root = temp_root("newhome");
    let env = HostEnv::at_root(&root);
    let index = session_index::index_path(&env.sessions_root());
    assert_eq!(index, root.join(".ULL/logs/game/index.jsonl"));

    let entry = sample_entry("fresh", "game/fresh.jsonl", "2026-11-08T09:00:00.000Z");
    session_index::append(&index, &entry).expect("写新索引");
    let file = env.sessions_root().join("game/fresh.jsonl");
    std::fs::write(
        &file,
        "{\"kind\":\"stdout\",\"payload\":{\"line\":\"新会话\"}}\n",
    )
    .expect("写新事件流");

    let listed = sessions(&env);
    assert_eq!(
        listed.first().map(|session| session.id.as_str()),
        Some("fresh"),
        "{listed:?}"
    );
    assert_eq!(event_path(&env, &entry), file);
    // 旧目录不许被凭空造出来（我们没搬东西，也没义务建一个旧家）。
    assert!(!env.legacy_sessions_root().exists());
    std::fs::remove_dir_all(&root).ok();
}

/// 同一个 id 同时出现在两个根时**新根赢**（用户在新位置又跑了一局同名会话）。
#[test]
fn the_new_root_wins_when_both_indexes_know_the_same_id() {
    let root = temp_root("both");
    let env = HostEnv::at_root(&root);
    let legacy_game = env.legacy_sessions_root().join("game");
    std::fs::create_dir_all(&legacy_game).expect("建旧家目录");
    let old = sample_entry("same", "game/same.jsonl", "2026-10-01T00:00:00.000Z");
    session_index::append(
        &session_index::index_path(&env.legacy_sessions_root()),
        &old,
    )
    .expect("写旧索引");
    std::fs::write(
        legacy_game.join("same.jsonl"),
        "{\"kind\":\"stdout\",\"payload\":{\"line\":\"旧\"}}\n",
    )
    .expect("写旧事件流");

    let fresh = sample_entry("same", "game/same.jsonl", "2026-11-08T00:00:00.000Z");
    session_index::append(&session_index::index_path(&env.sessions_root()), &fresh)
        .expect("写新索引");
    let new_file = env.sessions_root().join("game/same.jsonl");
    std::fs::write(
        &new_file,
        "{\"kind\":\"stdout\",\"payload\":{\"line\":\"新\"}}\n",
    )
    .expect("写新事件流");

    let entries = super::entries(&env);
    let hits: Vec<&SessionIndexEntry> = entries
        .iter()
        .filter(|entry| entry.session_id == "same")
        .collect();
    assert_eq!(hits.len(), 1, "同一个 id 只能出现一次：{hits:?}");
    assert_eq!(hits[0].started_at, fresh.started_at, "新根那条应当赢");
    assert_eq!(event_path(&env, &fresh), new_file);
    std::fs::remove_dir_all(&root).ok();
}

/// 落点记录：迁了什么（没有）、什么时候、旧路径还在不在。
#[test]
fn the_location_receipt_names_both_roots_and_the_policy() {
    let root = temp_root("receipt");
    let env = HostEnv::at_root(&root);
    let legacy_game = env.legacy_sessions_root().join("game");
    std::fs::create_dir_all(&legacy_game).expect("建旧家目录");
    std::fs::write(legacy_game.join("a.jsonl"), "{}\n").expect("写旧文件");

    let path = note_location(&env).expect("记录应当写得出来");
    assert_eq!(
        path,
        env.logs_dir().join(super::location::SESSION_LOCATION_FILE)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("读记录")).expect("记录是 JSON");
    assert_eq!(value["policy"], "read-only-compat");
    assert_eq!(value["moved"], 0, "我们一个文件都没搬");
    assert_eq!(value["legacy_present"], true);
    assert_eq!(value["legacy_files"], 1);
    assert_eq!(
        value["sessions_root"],
        env.sessions_root().display().to_string()
    );
    assert_eq!(
        value["legacy_root"],
        env.legacy_sessions_root().display().to_string()
    );
    std::fs::remove_dir_all(&root).ok();
}
