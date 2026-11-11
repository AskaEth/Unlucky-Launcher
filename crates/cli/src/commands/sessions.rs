//! `sessions list` / `logs` / `diag`：读 M18 的会话索引与 JSONL 事件流。
//!
//! 数据源形状见 `DevDocs/SubAgentsTalk/launch-接口约定.md` §2/§5：
//! - `<sessions_root>/game/index.jsonl`：一会话一行摘要（同 id 取最后一行，倒序）；
//! - `<sessions_root>/game/<id>.jsonl`：事件流（`kind` + `payload`）。
//!
//! **M188 起 `sessions_root` = `<.ULL>/logs`**（旧家 `<.ULL>/data/agent/sessions` 只读兼容：
//! 两个索引合并、事件流先新根后旧家），`launcher.log` 就在同一层的 `logs/` 里。
//!
//! `diag` 的结论优先取会话里**落盘的那份**，老会话才把脱敏文本喂回 M15 的内嵌规则集
//! （M242，为什么与代价见 `sessions/diagnosis.rs` 的模块文档）。

mod diagnosis;

/// M242：`ull diag` 的结论来源（落盘那份优先）。拆文件守「单文件 ≤400 行」。
#[cfg(test)]
#[path = "sessions/m242_tests.rs"]
mod m242_tests;

use std::path::Path;

use serde_json::{json, Value};
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
    let entries = merged_entries(ctx)?;
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
    // M188：旧家的会话仍然列得出来，就得说清它们从哪来——否则用户会以为「新目录里怎么没有」。
    let legacy = ctx.legacy_session_index();
    if legacy.is_file() {
        human.push(format!(
            "  旧会话目录仍然只读兼容：{}（不搬不删）",
            legacy.display()
        ));
    }
    human.push(format!("  启动器日志：{}", ctx.logs_dir().display()));
    if shown.is_empty() {
        human.push("  （空）先用 `%prog% launch <实例>` 跑一局".to_string());
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

/// `ull diag <sid> [--tail N]`：给会话下诊断结论。
///
/// 结论来源**先是会话里落盘的 `diagnosis` 事件**（M242），没有它的老会话才在脱敏文本上重跑
/// ——为什么、代价如何见 `sessions/diagnosis.rs` 的模块文档。`--tail` 只影响重跑那条路。
///
/// `exit` 仍取自索引（launch 报的事实），不是 M15 的结论：换来源只换结论。
///
/// # Errors
///
/// 会话不存在、事件流读不了、结论序列化失败时返回错误。
pub fn diag(ctx: &Ctx, args: &DiagArgs) -> Result<Report, CliError> {
    let entry = find_entry(ctx, &args.session_id)?;
    let path = event_path(ctx, &entry);
    let events = read_events(&path)?;
    let found = diagnosis::conclusion(&entry, &events, args.tail)?;

    // 脱敏一次、两条输出共用：`--json` 是机器消费路径，不该比人类路径拿到更多原文。
    let human: Vec<String> = diagnosis::lines(&entry, &found)
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
        "conclusion_source": found.provenance.as_str(),
        "diagnosis": found.record,
    }));
    Ok(Report::new(payload, human))
}

fn find_entry(ctx: &Ctx, key: &str) -> Result<SessionIndexEntry, CliError> {
    let index = ctx.session_index();
    let entries = merged_entries(ctx)?;
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
    // M188：新会话在 `logs/game/`，旧会话在 `data/agent/sessions/game/`（只读兼容）。
    // 先看新根，再回落旧家；两处都没有就把新根路径交出去（错误信息据此说「该去哪找」）。
    // M211/D2：`log_path` 已经**相对会话根**（`game/<id>.jsonl`），解析交给 core 里唯一的
    // `session_index::event_file`——这里不许再自己拼 `game/`，否则又是一份第三真相。
    let primary = session_index::event_file(&ctx.sessions_root(), &entry.log_path);
    if primary.is_file() {
        return primary;
    }
    let legacy = session_index::event_file(&ctx.legacy_sessions_root(), &entry.log_path);
    if legacy.is_file() {
        return legacy;
    }
    primary
}

/// 两个索引合并后的条目（新根 `logs/` 在前，旧家只读兼容在后；同 id 以新根为准）。
fn merged_entries(ctx: &Ctx) -> Result<Vec<SessionIndexEntry>, CliError> {
    let mut indexes = vec![ctx.session_index()];
    let legacy = ctx.legacy_session_index();
    if legacy.is_file() {
        indexes.push(legacy);
    }
    let mut merged: Vec<SessionIndexEntry> = Vec::new();
    for index in indexes {
        for entry in session_index::read_all(&index)
            .map_err(|error| CliError::io("读会话索引", &index, &error))?
        {
            if merged
                .iter()
                .any(|item| item.session_id == entry.session_id)
            {
                continue;
            }
            merged.push(entry);
        }
    }
    merged.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    Ok(merged)
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

/// M211/D5：`ull launch` 也必须写 `<data>/logs/launcher.log`（与 GUI 同一份文件、同一行格式）。
///
/// 为什么这条用例在这个文件里：CLI 侧本 mission 只允许改 `commands/launch.rs` 与
/// `commands/sessions.rs`，而 `launch.rs` 已到 389 行、再加用例会破「文件 ≤400 行」——
/// 用例放在同一个 `<data>/logs` 层级的读取方（本文件打印「启动器日志：<dir>」）旁边。
///
/// 变异证明：删掉 `commands/launch.rs` 里的 `record_start(...)` ⇒ 本条红（文件根本不存在）；
/// 把 `set_dir` 的入参换成别的目录 ⇒ 本条红。
#[cfg(test)]
mod tests {
    /// 一个最小实例档案（`profiles.json`）：只要能让 `launch` 走到「准备启动」这一段。
    const PROFILES: &str = r#"{"schema_version":1,"profiles":[{"id":"11111111-2222-4333-8444-000000000001","name":"生存服","version_id":"1.20.1"}]}"#;

    #[test]
    fn a_launch_attempt_writes_the_launcher_log() {
        let root = std::env::temp_dir().join(format!("ull-m211-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".ULL/config")).expect("建配置目录");
        std::fs::write(root.join(".ULL/config/profiles.json"), PROFILES).expect("写 profiles");

        let args: Vec<String> = vec![
            "launch".to_string(),
            "生存服".to_string(),
            "--data-dir".to_string(),
            root.display().to_string(),
        ];
        let (mut out, mut err) = (Vec::new(), Vec::new());
        // 版本本体不存在 ⇒ 这条命令会失败，但「启动前」那一行必须已经落盘
        // （事故现场最需要的正是「它打算用什么起的」）。
        let _ = crate::run(&args, &mut out, &mut err);

        let log = std::fs::read_to_string(root.join(".ULL/logs/launcher.log"))
            .expect("`ull launch` 必须在 <data>/logs/launcher.log 里留下启动器日志");
        assert!(
            log.contains("[launch] instance=11111111-2222-4333-8444-000000000001"),
            "{log}"
        );
        assert!(
            log.contains("game_dir=") && log.contains("memory="),
            "{log}"
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
