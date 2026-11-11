//! 会话端口：索引 → `list_game_sessions`，事件流切片 → `read_game_log`。

use serde_json::{json, Value};

use unlucky_agent::port::{
    GameSessionPort, GameSessionSummary, LogEvent, LogPage, LogQuery, PortResult,
};

use super::events::SessionSource;

/// M18 的会话索引 + 事件流。
#[derive(Debug, Clone)]
pub struct CoreSessions {
    source: SessionSource,
}

impl CoreSessions {
    /// `root` = `<数据根>/.ULL/logs`（M188 起；`LauncherConfig` 在其下建 `game/`）。
    #[must_use]
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            source: SessionSource::new(root),
        }
    }

    /// 再挂一个只读兼容的旧根（`<数据根>/.ULL/data/agent/sessions`，M188）。
    #[must_use]
    pub fn with_legacy(mut self, legacy: impl Into<std::path::PathBuf>) -> Self {
        self.source = self.source.with_legacy(legacy);
        self
    }
}

fn summary_of(
    entry: &unlucky_core::launch::session_index::SessionIndexEntry,
) -> GameSessionSummary {
    GameSessionSummary {
        session_id: entry.session_id.clone(),
        instance_id: Some(entry.instance_id.clone()).filter(|id| !id.is_empty()),
        instance_name: Some(entry.instance_name.clone()).filter(|name| !name.is_empty()),
        game_version: Some(entry.game_version.clone()).filter(|v| !v.is_empty()),
        started_at: Some(entry.started_at.clone()).filter(|v| !v.is_empty()),
        ended_at: Some(entry.ended_at.clone()).filter(|v| !v.is_empty()),
        duration_ms: entry.duration_ms,
        exit_code: entry.exit_code,
        exit_type: entry.exit_type.clone(),
        silent_failure: entry.silent_failure,
        window_detected: entry.window_detected,
        event_count: entry.event_count,
        crash_found: entry.crash_found,
        diagnosis_top: entry.diagnosis_top.clone(),
    }
}

fn to_log_event(event: &Value, fallback_seq: u64) -> LogEvent {
    LogEvent {
        seq: event
            .get("seq")
            .and_then(Value::as_u64)
            .unwrap_or(fallback_seq),
        ts: event
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        t_ms: event.get("t_ms").and_then(Value::as_u64).unwrap_or(0),
        kind: event
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        payload: event.get("payload").cloned().unwrap_or(Value::Null),
    }
}

impl GameSessionPort for CoreSessions {
    fn list_sessions(
        &self,
        limit: usize,
        instance_id: Option<&str>,
    ) -> PortResult<Vec<GameSessionSummary>> {
        let mut entries = self.source.entries()?;
        if let Some(instance) = instance_id {
            entries.retain(|entry| entry.instance_id == instance);
        }
        entries.truncate(limit);
        Ok(entries.iter().map(summary_of).collect())
    }

    fn read_log(&self, session_id: &str, query: &LogQuery) -> PortResult<LogPage> {
        let entry = self.source.find(session_id)?;
        let raw = self.source.events(&entry)?;
        let events: Vec<LogEvent> = raw
            .iter()
            .enumerate()
            .map(|(index, event)| to_log_event(event, index as u64))
            .collect();

        let mut kind_counts = std::collections::BTreeMap::new();
        for event in &events {
            *kind_counts.entry(event.kind.clone()).or_insert(0u64) += 1;
        }
        let matched: Vec<&LogEvent> = events
            .iter()
            .filter(|e| query.from_seq.is_none_or(|from| e.seq >= from))
            .filter(|e| query.to_seq.is_none_or(|to| e.seq <= to))
            .filter(|e| query.kinds.is_empty() || query.kinds.contains(&e.kind))
            .collect();
        let max = if query.max_events == 0 {
            matched.len()
        } else {
            query.max_events
        };
        let truncated = matched.len() > max;

        Ok(LogPage {
            session_id: entry.session_id,
            total_events: events.len() as u64,
            matched_events: matched.len() as u64,
            truncated,
            kind_counts,
            first_ts: events.first().map(|e| e.ts.clone()),
            last_ts: events.last().map(|e| e.ts.clone()),
            events: matched.into_iter().take(max).cloned().collect(),
        })
    }
}

/// 会话摘要 + 事件流 → JSON 里用得到的稳定形状（`read_game_log` 的 payload 直接透传）。
#[must_use]
pub fn log_event_json(event: &LogEvent) -> Value {
    json!({
        "seq": event.seq,
        "ts": event.ts,
        "t_ms": event.t_ms,
        "kind": event.kind,
        "payload": event.payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use unlucky_core::launch::session_index;

    /// 生产写入侧真会写出的形状（M212/D2）：`log_path` 相对**会话根**，事件流真落点
    /// `<根>/game/<id>.jsonl`。夹具以前写裸文件名、把事件流也放在 `game/` 下，
    /// 与生产只差一层 `game/` —— 正是这一层让「读取侧多 join 一层」永远测不出来。
    fn seed_session(root: &Path) {
        let log_path = session_index::log_path_for("20261029T124501-9f3c2a");
        let index = session_index::index_path(root);
        std::fs::create_dir_all(index.parent().expect("索引父目录")).expect("建目录");
        let entry = json!({
            "v": 1,
            "session_id": "20261029T124501-9f3c2a",
            "instance_id": "inst-1",
            "instance_name": "1.20.1 Forge",
            "game_version": "1.20.1",
            "started_at": "2026-10-29T12:45:01.123Z",
            "ended_at": "2026-10-29T12:46:31.123Z",
            "duration_ms": 90000,
            "exit_code": 1,
            "exit_type": "application_error",
            "silent_failure": false,
            "window_detected": true,
            "event_count": 2,
            "log_path": log_path.clone(),
            "crash_found": true,
            "diagnosis_top": "integrity.forge_missing_library"
        });
        std::fs::write(index, format!("{entry}\n")).expect("写索引");
        let events = [
            json!({"seq":0,"ts":"2026-10-29T12:45:01.123Z","t_ms":0,"kind":"stdout","payload":{"line":"hi","level":"info"}}),
            json!({"seq":1,"ts":"2026-10-29T12:45:02.123Z","t_ms":1000,"kind":"stderr","payload":{"line":"boom","level":"error"}}),
        ];
        let text: String = events
            .iter()
            .map(|event| format!("{event}\n"))
            .collect::<String>();
        std::fs::write(session_index::event_file(root, &log_path), text).expect("写事件");
    }

    #[test]
    fn lists_and_slices_a_session() {
        let root = std::env::temp_dir().join(format!("ull-mcp-sessions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        seed_session(&root);

        let port = CoreSessions::new(root.clone());
        let sessions = port.list_sessions(10, None).expect("应能列出");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].exit_code, Some(1));
        assert!(sessions[0].crash_found);

        let page = port
            .read_log(
                "20261029T124501",
                &LogQuery {
                    max_events: 1,
                    kinds: vec!["stderr".to_string()],
                    ..LogQuery::default()
                },
            )
            .expect("应能读");
        assert_eq!(page.total_events, 2);
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].kind, "stderr");
        assert!(page.kind_counts.contains_key("stdout"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
