//! `sessions list` / `logs` / `diag`：读 M18 的会话索引与 JSONL 事件流。
//!
//! 数据源形状见 `DevDocs/SubAgentsTalk/launch-接口约定.md` §2/§5：
//! - `<sessions_root>/game/index.jsonl`：一会话一行摘要（同 id 取最后一行，倒序）；
//! - `<sessions_root>/game/<id>.jsonl`：事件流（`kind` + `payload`）。
//!
//! `diag` 把「退出信号 + stdout/stderr 文本」喂给 M15 的内嵌规则集，输出结构化结论。

use std::path::Path;

use serde_json::{json, Value};
use unlucky_core::diagnose::collectors::{ExitSignal, SignalOrigin};
use unlucky_core::diagnose::engine::{Diagnoser, DiagnosisInput};
use unlucky_core::diagnose::loader::RuleLoader;
use unlucky_core::launch::session_index::{self, SessionIndexEntry};

use crate::cli::{DiagArgs, LogsArgs};
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

/// `ull sessions list [--limit N]`。
///
/// # Errors
///
/// 索引文件读不了时返回用户错误（不存在 = 空列表）。
pub fn list(ctx: &Ctx, limit: usize) -> Result<Report, CliError> {
    let index = ctx.session_index();
    let entries = session_index::read_all(&index)
        .map_err(|error| CliError::io("读会话索引", &index, &error))?;
    let shown: Vec<&SessionIndexEntry> = entries.iter().take(limit).collect();

    let rows: Vec<Value> = shown
        .iter()
        .map(|entry| {
            json!({
                "session_id": entry.session_id,
                "instance_id": entry.instance_id,
                "instance_name": entry.instance_name,
                "game_version": entry.game_version,
                "started_at": entry.started_at,
                "ended_at": entry.ended_at,
                "duration_ms": entry.duration_ms,
                "exit_code": entry.exit_code,
                "exit_type": entry.exit_type,
                "silent_failure": entry.silent_failure,
                "window_detected": entry.window_detected,
                "crash_found": entry.crash_found,
                "diagnosis_top": entry.diagnosis_top,
            })
        })
        .collect();

    let mut human = vec![format!(
        "会话 {} 条（索引 {}）",
        shown.len(),
        index.display()
    )];
    if shown.is_empty() {
        human.push("  （空）先用 `ull launch <实例>` 跑一局".to_string());
    }
    for entry in &shown {
        human.push(format!(
            "  {}  {}  {}  {}  {}",
            entry.session_id,
            entry.instance_name,
            entry.game_version,
            entry.started_at,
            exit_label(entry),
        ));
    }

    Ok(Report::new(json!({ "sessions": rows }), human))
}

/// `ull logs <sid> [--tail N]`：打印事件流末尾 N 条。
///
/// # Errors
///
/// 会话不存在/前缀有歧义、事件流读不了时返回用户错误。
pub fn logs(ctx: &Ctx, args: &LogsArgs) -> Result<Report, CliError> {
    let entry = find_entry(ctx, &args.session_id)?;
    let path = event_path(ctx, &entry);
    let events = read_events(&path)?;
    let start = events.len().saturating_sub(args.tail.max(1));
    let shown = &events[start..];

    // 脱敏一次、两条输出共用：`--json` 是机器消费路径，不该比人类路径拿到更多原文。
    let shown: Vec<Value> = shown.iter().map(redact_value).collect();
    let mut human = vec![format!(
        "会话 {} · {} · 事件 {} 条（显示末尾 {}）",
        entry.session_id,
        entry.instance_name,
        events.len(),
        shown.len()
    )];
    human.extend(shown.iter().map(render_event));

    Ok(Report::new(
        json!({
            "session_id": entry.session_id,
            "path": path.display().to_string(),
            "total": events.len(),
            "events": shown,
        }),
        human,
    ))
}

/// `ull diag <sid> [--tail N]`：规则引擎诊断。
///
/// # Errors
///
/// 会话不存在、事件流读不了、结论序列化失败时返回错误。
pub fn diag(ctx: &Ctx, args: &DiagArgs) -> Result<Report, CliError> {
    let entry = find_entry(ctx, &args.session_id)?;
    let path = event_path(ctx, &entry);
    let events = read_events(&path)?;

    let mut stdout_lines: Vec<&str> = Vec::new();
    let mut stderr_lines: Vec<&str> = Vec::new();
    let mut error_lines: u32 = 0;
    for event in &events {
        let Some(line) = event.pointer("/payload/line").and_then(Value::as_str) else {
            continue;
        };
        match event.get("kind").and_then(Value::as_str) {
            Some("stdout") => stdout_lines.push(line),
            Some("stderr") => {
                stderr_lines.push(line);
                if event.pointer("/payload/level").and_then(Value::as_str) == Some("error") {
                    error_lines += 1;
                }
            }
            _ => {}
        }
    }
    let stdout_text = tail_join(&stdout_lines, args.tail);
    let stderr_text = tail_join(&stderr_lines, args.tail);

    let exit = ExitSignal::classify(
        entry.exit_code,
        Some(entry.duration_ms),
        error_lines,
        &stderr_text,
    );
    let mut input = DiagnosisInput::new().with_exit(exit);
    if !stdout_text.is_empty() {
        input.push_text(SignalOrigin::Stdout, stdout_text);
    }
    if !stderr_text.is_empty() {
        input.push_text(SignalOrigin::Stderr, stderr_text);
    }

    let loader = RuleLoader::embedded();
    let diagnoser = Diagnoser::new(loader.rule_set().rules().to_vec());
    let report = diagnoser.analyze(&input);
    let record = serde_json::to_value(report.to_record())
        .map_err(|error| CliError::internal("diagnosis_serialize", error.to_string()))?;

    let mut human = vec![format!(
        "会话 {} · {} · {} · {}",
        entry.session_id,
        entry.instance_name,
        entry.game_version,
        exit_label(&entry)
    )];
    match report.primary() {
        Some(finding) => {
            human.push(format!("主因: {}", finding.message));
            if let Some(detail) = &finding.detail {
                human.push(format!("说明: {detail}"));
            }
            for evidence in finding.evidence.iter().take(3) {
                human.push(format!("证据: {}", evidence.render()));
            }
        }
        None => human.push("规则引擎没有得出确定结论（needs_more_info）".to_string()),
    }
    human.push(format!(
        "规则评估 {} 条 · 结论 {} 条 · 次生 {} 条",
        report.rules_evaluated,
        report.findings.len(),
        report.secondary.len()
    ));
    let human: Vec<String> = human
        .iter()
        .map(|line| unlucky_agent::redact::redact_text(line))
        .collect();

    let payload = redact_value(&json!({
        "session_id": entry.session_id,
        "instance_id": entry.instance_id,
        "game_version": entry.game_version,
        "exit": {
            "code": entry.exit_code,
            "exit_type": entry.exit_type,
            "silent_failure": entry.silent_failure,
            "duration_ms": entry.duration_ms,
        },
        "diagnosis": record,
    }));
    Ok(Report::new(payload, human))
}

fn find_entry(ctx: &Ctx, key: &str) -> Result<SessionIndexEntry, CliError> {
    let index = ctx.session_index();
    let entries = session_index::read_all(&index)
        .map_err(|error| CliError::io("读会话索引", &index, &error))?;
    if let Some(entry) = entries.iter().find(|entry| entry.session_id == key) {
        return Ok(entry.clone());
    }
    let matches: Vec<&SessionIndexEntry> = entries
        .iter()
        .filter(|entry| entry.session_id.starts_with(key))
        .collect();
    match matches.as_slice() {
        [only] => Ok((*only).clone()),
        [] => Err(
            CliError::user("session_not_found", format!("找不到会话 `{key}`"))
                .with_hint(format!("索引文件：{}", index.display())),
        ),
        _ => Err(CliError::user(
            "session_ambiguous",
            format!("`{key}` 匹配到 {} 个会话，请给更长的前缀", matches.len()),
        )),
    }
}

fn event_path(ctx: &Ctx, entry: &SessionIndexEntry) -> std::path::PathBuf {
    ctx.sessions_root().join("game").join(&entry.log_path)
}

fn read_events(path: &Path) -> Result<Vec<Value>, CliError> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| CliError::io("读会话事件流", path, &error))?;
    Ok(text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect())
}

fn tail_join(lines: &[&str], tail: usize) -> String {
    let start = lines.len().saturating_sub(tail.max(1));
    let mut text = lines[start..].join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

fn render_event(event: &Value) -> String {
    let ts = event.get("ts").and_then(Value::as_str).unwrap_or("");
    let kind = event.get("kind").and_then(Value::as_str).unwrap_or("?");
    let detail = event
        .pointer("/payload/line")
        .and_then(Value::as_str)
        .or_else(|| event.pointer("/payload/summary").and_then(Value::as_str))
        .unwrap_or_default();
    format!("[{ts}] {kind} {detail}")
}

/// 递归脱敏任意 JSON（所有字符串过 `redact_text`：主目录用户名段 + Bearer 令牌）。
fn redact_value(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(unlucky_agent::redact::redact_text(text)),
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, child)| (key.clone(), redact_value(child)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn exit_label(entry: &SessionIndexEntry) -> String {
    let code = entry
        .exit_code
        .map_or_else(|| "无退出码".to_string(), |code| format!("退出码 {code}"));
    let exit_type = entry
        .exit_type
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let silent = if entry.silent_failure {
        " · 静默失败"
    } else {
        ""
    };
    format!("{code}/{exit_type}{silent}")
}
