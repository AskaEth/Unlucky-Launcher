//! `list_game_sessions` / `read_game_log`（02 文档 §3.2「能读取某次游戏崩溃日志」）。
//!
//! 数据源是 M18 的会话事件流（`DevDocs/SubAgentsTalk/launch-接口约定.md`）：
//! - 列表读 `index.jsonl`（每会话一行摘要，同 id 取最后一行）；
//! - 日志读 `<session-id>.jsonl`，**按 `seq` 切片 + 按 `kind` 过滤**，不是按行号。
//!
//! 两条防 token 爆炸的硬约束：
//! 1. 单次最多 [`MAX_LOG_EVENTS`] 条事件（默认 [`DEFAULT_LOG_EVENTS`]）；
//! 2. 单条 payload 里的字符串超过 [`MAX_PAYLOAD_CHARS`] 截断。
//!
//! 脱敏：launch 落盘前已经做过一遍（`<home>` / `<redacted:token>`），本工具再跑一遍
//! [`crate::redact::redact_text`] 作为第二道防线（幂等，旧会话/第三方 mod 输出可能漏网）。

use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::port::{GameSessionPort, LogEvent, LogPage, LogQuery};
use crate::redact::redact_text;
use crate::registry::schema::{enum_string_array, integer, non_empty_string, object, string};
use crate::registry::{
    RegistryError, ToolCtx, ToolError, ToolFn, ToolOutput, ToolRegistry, ToolSpec,
};

/// 列出游戏会话。
pub const LIST_GAME_SESSIONS: &str = "list_game_sessions";
/// 读一个会话的日志。
pub const READ_GAME_LOG: &str = "read_game_log";

/// 单次返回的事件上限（防 token 爆炸的硬闸）。
pub const MAX_LOG_EVENTS: usize = 500;
/// 默认返回条数。
pub const DEFAULT_LOG_EVENTS: usize = 100;
/// 单条 payload 字符串的展示上限（字符）。
pub const MAX_PAYLOAD_CHARS: usize = 800;
/// 列表默认/最大条数。
pub const DEFAULT_SESSION_LIMIT: usize = 10;
/// 列表最大条数。
pub const MAX_SESSION_LIMIT: usize = 50;

/// `launch-接口约定.md` §3 的事件类型（schema 里给模型做枚举；未知 kind 仍可透传）。
pub const KNOWN_KINDS: [&str; 13] = [
    "session_start",
    "java_selected",
    "cmd_built",
    "natives_extracted",
    "proc_start",
    "stdout",
    "stderr",
    "window_detected",
    "exit",
    "crash_found",
    "diagnosis",
    "killed",
    "session_end",
];

/// 注册两个日志工具。
///
/// # Errors
/// 工具名重复/schema 非法时返回 [`RegistryError`]。
pub fn register(
    registry: &mut ToolRegistry,
    port: Arc<dyn GameSessionPort>,
) -> Result<(), RegistryError> {
    registry.register(list_sessions_spec(Arc::clone(&port)))?;
    registry.register(read_log_spec(port))
}

fn list_sessions_spec(port: Arc<dyn GameSessionPort>) -> ToolSpec {
    let handler: ToolFn = Arc::new(move |_ctx: &ToolCtx, args: &Value| {
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_SESSION_LIMIT, |n| n as usize)
            .clamp(1, MAX_SESSION_LIMIT);
        let instance_id = args.get("instance_id").and_then(Value::as_str);
        let sessions = port
            .list_sessions(limit, instance_id)
            .map_err(ToolError::Port)?;

        if sessions.is_empty() {
            return Ok(ToolOutput::ok(
                json!({ "sessions": [], "count": 0 }),
                "没有游戏会话记录（还没启动过游戏，或数据目录不对）".to_string(),
            ));
        }

        let crashed = sessions
            .iter()
            .filter(|s| s.crash_found || s.silent_failure || s.exit_code.is_some_and(|c| c != 0))
            .count();
        let summary = format!(
            "最近 {} 个游戏会话，其中 {crashed} 个崩溃/异常退出；最近一次 {}（{}）",
            sessions.len(),
            sessions[0].session_id,
            sessions[0].started_at.as_deref().unwrap_or("启动时间未知")
        );
        Ok(ToolOutput::ok(
            json!({ "sessions": sessions, "count": sessions.len(), "problem_count": crashed }),
            summary,
        ))
    });

    ToolSpec::read_only(
        LIST_GAME_SESSIONS,
        "列出最近的游戏会话（启动时间、退出码、是否崩溃、规则引擎首选结论）。用于定位要读哪一次会话的日志。",
        object(
            &[
                ("limit", integer("返回条数", 1, MAX_SESSION_LIMIT as i64)),
                ("instance_id", string("只看某个实例的会话；省略则全部实例")),
            ],
            &[],
        ),
        handler,
    )
}

fn read_log_spec(port: Arc<dyn GameSessionPort>) -> ToolSpec {
    let handler: ToolFn = Arc::new(move |_ctx: &ToolCtx, args: &Value| {
        let session_id = args
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::NotAllowed("缺少参数 `session_id`".to_string()))?
            .to_string();

        let kinds: Vec<String> = args
            .get("kinds")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let max_events = args
            .get("max_events")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_LOG_EVENTS, |n| n as usize)
            .clamp(1, MAX_LOG_EVENTS);

        let query = LogQuery {
            from_seq: args.get("from_seq").and_then(Value::as_u64),
            to_seq: args.get("to_seq").and_then(Value::as_u64),
            kinds,
            max_events,
        };
        let page = port
            .read_log(&session_id, &query)
            .map_err(ToolError::Port)?;
        Ok(render_page(&page))
    });

    ToolSpec::read_only(
        READ_GAME_LOG,
        "读取某次游戏会话的事件流（stdout/stderr 按行、退出事件、crash-report 摘录）。可按 seq 区间和事件类型过滤；单次最多 500 条、单条最长 800 字符，优先用 kinds 过滤 stderr/exit/crash_found 而不是拉全量。",
        object(
            &[
                ("session_id", non_empty_string("会话 id，来自 list_game_sessions")),
                ("from_seq", integer("起始 seq（含）", 0, 9_007_199_254_740_991)),
                ("to_seq", integer("结束 seq（含）", 0, 9_007_199_254_740_991)),
                (
                    "kinds",
                    enum_string_array(&KNOWN_KINDS, "只要这些事件类型；省略则全部", 13),
                ),
                ("max_events", integer("最多返回多少条", 1, MAX_LOG_EVENTS as i64)),
            ],
            &["session_id"],
        ),
        handler,
    )
}

/// 把端口返回的一页日志压成工具结果（含摘要，模型据此决定要不要继续翻页）。
fn render_page(page: &LogPage) -> ToolOutput {
    let events: Vec<Value> = page.events.iter().map(render_event).collect();
    let kind_summary = page
        .kind_counts
        .iter()
        .map(|(kind, count)| format!("{kind}×{count}"))
        .collect::<Vec<_>>()
        .join(", ");

    let (content, summary) = if events.is_empty() {
        (
            json!({
                "session_id": page.session_id,
                "total_events": page.total_events,
                "matched_events": 0,
                "truncated": false,
                "kind_counts": page.kind_counts,
                "first_ts": page.first_ts,
                "last_ts": page.last_ts,
                "events": [],
                "hint": "没有符合过滤条件的事件；把 kinds/from_seq/to_seq 放宽再试",
            }),
            format!(
                "会话 {} 没有匹配的事件（共 {} 条）",
                page.session_id, page.total_events
            ),
        )
    } else {
        let truncated_note = if page.truncated {
            "（已截断）"
        } else {
            ""
        };
        (
            json!({
                "session_id": page.session_id,
                "total_events": page.total_events,
                "matched_events": page.matched_events,
                "truncated": page.truncated,
                "kind_counts": page.kind_counts,
                "first_ts": page.first_ts,
                "last_ts": page.last_ts,
                "events": events,
            }),
            format!(
                "会话 {}：共 {} 条事件，返回 {} 条{truncated_note}；类型分布 {kind_summary}",
                page.session_id,
                page.total_events,
                events.len(),
            ),
        )
    };
    ToolOutput::ok(content, summary)
}

fn render_event(event: &LogEvent) -> Value {
    json!({
        "seq": event.seq,
        "ts": event.ts,
        "t_ms": event.t_ms,
        "kind": event.kind,
        "payload": shrink(&event.payload),
    })
}

/// 递归脱敏 + 截断（第二道脱敏防线）。
fn shrink(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(cap(&redact_text(text))),
        Value::Array(items) => Value::Array(items.iter().map(shrink).collect()),
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            for (key, item) in map {
                out.insert(key.clone(), shrink(item));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn cap(text: &str) -> String {
    let length = text.chars().count();
    if length <= MAX_PAYLOAD_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(MAX_PAYLOAD_CHARS).collect();
    format!("{head}…（已截断 {} 字符）", length - MAX_PAYLOAD_CHARS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockSessions;
    use serde_json::json;

    const SESSION: &str = "20261029T124501-9f3c2a";

    fn mock() -> MockSessions {
        let (summary, events) = MockSessions::crashing_session(SESSION);
        let (older, older_events) = MockSessions::crashing_session("20261028T100000-aaaaaa");
        let mut sessions = MockSessions::new();
        sessions.add(summary, events);
        sessions.add(older, older_events);
        sessions
    }

    fn registry() -> ToolRegistry {
        let mut registry = ToolRegistry::new();
        register(&mut registry, Arc::new(mock())).expect("注册应成功");
        registry
    }

    /// 走**真实注册表 → 真实 handler**，不复制实现（否则测的是复制品）。
    fn call(tool: &str, args: Value) -> ToolOutput {
        let registry = registry();
        let spec = registry.get(tool).expect("工具应存在").clone();
        (spec.handler)(
            &ToolCtx {
                agent_session_id: "s1".to_string(),
                confirmed: false,
            },
            &args,
        )
        .expect("工具应成功")
    }

    #[test]
    fn lists_sessions_newest_first() {
        let out = call(LIST_GAME_SESSIONS, json!({ "limit": 5 }));
        assert_eq!(out.content["count"], json!(2));
        assert_eq!(out.content["sessions"][0]["session_id"], json!(SESSION));
        assert_eq!(out.content["problem_count"], json!(2));
        assert!(out.summary.contains("2 个崩溃/异常退出"));
    }

    #[test]
    fn list_respects_limit() {
        let out = call(LIST_GAME_SESSIONS, json!({ "limit": 1 }));
        assert_eq!(out.content["count"], json!(1));
    }

    #[test]
    fn filters_by_kind_and_keeps_summary() {
        let out = call(
            READ_GAME_LOG,
            json!({ "session_id": SESSION, "kinds": ["stderr"] }),
        );
        assert_eq!(out.content["events"].as_array().unwrap().len(), 1);
        assert_eq!(out.content["kind_counts"]["stdout"], json!(1));
        assert!(out.summary.contains("共 5 条事件"));
    }

    #[test]
    fn truncates_when_more_events_than_cap() {
        let out = call(
            READ_GAME_LOG,
            json!({ "session_id": SESSION, "max_events": 2 }),
        );
        assert_eq!(out.content["events"].as_array().unwrap().len(), 2);
        assert_eq!(out.content["truncated"], json!(true));
        assert_eq!(out.content["matched_events"], json!(5));
    }

    #[test]
    fn masks_user_dir_in_payload() {
        let out = call(
            READ_GAME_LOG,
            json!({ "session_id": SESSION, "kinds": ["crash_found"] }),
        );
        let payload = &out.content["events"][0]["payload"];
        assert_eq!(
            payload["path"],
            json!(r"C:\Users\***\.minecraft\crash-reports\crash-1.txt")
        );
    }

    #[test]
    fn reports_empty_result_with_hint() {
        let out = call(
            READ_GAME_LOG,
            json!({ "session_id": SESSION, "kinds": ["window_detected"] }),
        );
        assert_eq!(out.content["events"], json!([]));
        assert!(out.content["hint"].is_string());
    }

    #[test]
    fn caps_long_payload_text() {
        let long = "x".repeat(MAX_PAYLOAD_CHARS + 10);
        assert!(cap(&long).contains("已截断 10 字符"));
    }

    #[test]
    fn unknown_session_is_a_port_error() {
        let registry = registry();
        let spec = registry.get(READ_GAME_LOG).expect("工具应存在").clone();
        let err = (spec.handler)(
            &ToolCtx {
                agent_session_id: "s1".to_string(),
                confirmed: false,
            },
            &json!({ "session_id": "nope" }),
        )
        .unwrap_err();
        assert_eq!(err.code(), "port_error");
        assert!(err.message().contains("不存在"));
    }
}
