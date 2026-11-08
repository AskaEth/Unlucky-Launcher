//! 测试替身：五个端口的 **mock 实现**（无盘、无网络、无时间依赖）。
//!
//! 放在库内（而不是只在 `#[cfg(test)]`）是刻意的：`crates/app` 装配时也要能给
//! 自己的单测造一套「无盘」环境，且 P1/M39 的 MCP 契约测试要复用同一批假数据。
//!
//! 按端口拆文件（`port.rs` 挨 400 行上限的同款理由）：`settings` 配置、`sessions`
//! 会话日志在下面，`game` 实例/Java/账户与 `diagnose` 诊断各一个子模块。
//! 对外名字不变，`unlucky_agent::testing::MockSettings` 之类照旧可用（走 re-export）。

pub mod diagnose;
pub mod game;
pub mod settings;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::port::{GameSessionPort, GameSessionSummary, LogEvent, LogPage, LogQuery, PortResult};

pub use diagnose::MockDiagnose;
pub use game::{MockAccounts, MockGame};
pub use settings::MockSettings;

/// 内存会话端口。
#[derive(Debug, Default)]
pub struct MockSessions {
    sessions: Vec<GameSessionSummary>,
    events: BTreeMap<String, Vec<LogEvent>>,
}

impl MockSessions {
    /// 空数据。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 加一个会话及其事件流。
    pub fn add(&mut self, summary: GameSessionSummary, events: Vec<LogEvent>) {
        self.events.insert(summary.session_id.clone(), events);
        self.sessions.push(summary);
    }

    /// 便捷：造一个崩溃会话（`exit_code=1` + stderr + crash_found）。
    #[must_use]
    pub fn crashing_session(session_id: &str) -> (GameSessionSummary, Vec<LogEvent>) {
        let summary = GameSessionSummary {
            session_id: session_id.to_string(),
            instance_id: Some("inst-1".to_string()),
            instance_name: Some("1.20.1 Forge".to_string()),
            game_version: Some("1.20.1".to_string()),
            started_at: Some("2026-10-29T12:45:01.123Z".to_string()),
            ended_at: Some("2026-10-29T12:46:31.123Z".to_string()),
            duration_ms: 90_000,
            exit_code: Some(1),
            exit_type: Some("application_error".to_string()),
            silent_failure: false,
            window_detected: true,
            event_count: 5,
            crash_found: true,
            diagnosis_top: Some("forge_missing_library".to_string()),
        };
        let events = vec![
            event(
                0,
                "stdout",
                json!({ "line": "Starting Minecraft 1.20.1", "level": "info" }),
            ),
            event(
                1,
                "stderr",
                json!({ "line": "java.lang.NoClassDefFoundError: cpw/mods/bootstraplauncher/BootstrapLauncher", "level": "error" }),
            ),
            event(
                2,
                "crash_found",
                json!({ "source": "crash_report", "path": r"C:\Users\aska\.minecraft\crash-reports\crash-1.txt", "excerpt": "Description: Mod loading failure" }),
            ),
            event(
                3,
                "exit",
                json!({ "code": 1, "exit_type": "application_error", "silent_failure": false }),
            ),
            event(
                4,
                "session_end",
                json!({ "reason": "exit", "duration_ms": 90000 }),
            ),
        ];
        (summary, events)
    }
}

fn event(seq: u64, kind: &str, payload: Value) -> LogEvent {
    LogEvent {
        seq,
        ts: format!("2026-10-29T12:45:0{seq}.000Z"),
        t_ms: seq * 1_000,
        kind: kind.to_string(),
        payload,
    }
}

impl GameSessionPort for MockSessions {
    fn list_sessions(
        &self,
        limit: usize,
        instance_id: Option<&str>,
    ) -> PortResult<Vec<GameSessionSummary>> {
        let mut out: Vec<GameSessionSummary> = self
            .sessions
            .iter()
            .filter(|s| instance_id.is_none_or(|id| s.instance_id.as_deref() == Some(id)))
            .cloned()
            .collect();
        out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        out.truncate(limit);
        Ok(out)
    }

    fn read_log(&self, session_id: &str, query: &LogQuery) -> PortResult<LogPage> {
        let events = self
            .events
            .get(session_id)
            .ok_or_else(|| format!("会话 `{session_id}` 不存在"))?;
        let mut kind_counts = BTreeMap::new();
        for event in events {
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
            session_id: session_id.to_string(),
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

/// 一次性造好全部端口的 [`crate::port::Ports`]（含一个崩溃会话，够跑通对话与日志工具）。
#[must_use]
pub fn mock_ports() -> (crate::port::Ports, std::sync::Arc<MockSettings>) {
    use std::sync::Arc;
    let settings = Arc::new(MockSettings::new());
    let (summary, events) = MockSessions::crashing_session("20261029T124501-9f3c2a");
    let mut sessions = MockSessions::new();
    sessions.add(summary, events);
    let ports = crate::port::Ports {
        setting: settings.clone(),
        sessions: Arc::new(sessions),
        diagnose: Arc::new(MockDiagnose::new()),
        game: Arc::new(MockGame::new()),
        accounts: Arc::new(MockAccounts::new()),
    };
    (ports, settings)
}

/// 测试里常用的「全部工具名」集合（契约测试与 MCP 投影比对用）。
#[must_use]
pub fn expected_v1_tool_names() -> BTreeSet<&'static str> {
    [
        "get_config",
        "get_config_schema",
        "set_config",
        "list_game_sessions",
        "read_game_log",
        "diagnose_crash",
        "list_instances",
        "list_versions",
        "get_java_status",
        "list_accounts",
    ]
    .into_iter()
    .collect()
}
