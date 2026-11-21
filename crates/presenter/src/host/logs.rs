//! 日志 / 诊断：会话索引 → 会话列表，事件 JSONL → 日志行，规则引擎 → 诊断卡片。
//!
//! 与 CLI `ull logs` / `ull diagnose` 同源（`launch::session_index` + `diagnose` 规则引擎）：
//! 「同一份 jsonl 的两种读法」只有这一处实现（M39 ports/events.rs 的设计）。
//! 实时滚动 = host 每拍尾随启动会话的 JSONL（只读新增字节），不额外开第二条事件通道。
//!
//! ## 会话日志在哪（M188）
//!
//! | 位置 | 角色 |
//! |---|---|
//! | `<.ULL>/logs/game/<session-id>.jsonl` + `index.jsonl` | **新会话写这里** |
//! | `<.ULL>/data/agent/sessions/game/…` | 旧家：**只读兼容**，不搬不删 |
//!
//! 两个根的合并、落点记录都在 [`location`] 里；本文件只做「读出来 → DTO」的翻译。

use std::path::{Path, PathBuf};

use serde_json::Value;
use unlucky_core::diagnose::collectors::{ExitSignal, SignalOrigin};
use unlucky_core::diagnose::engine::{Diagnoser, DiagnosisInput};
use unlucky_core::diagnose::loader::RuleLoader;
use unlucky_core::diagnose::rules::Severity;
use unlucky_core::launch::session_index::{self, SessionIndexEntry};

use crate::dto::{
    Diagnosis, DiagnosisReport, DiagnosisSeverity, LogLevel, LogLine, LogOutcome, LogSession,
    SessionId,
};

use super::env::HostEnv;

#[path = "logs/diagnosis.rs"]
mod diagnosis;
#[path = "logs/location.rs"]
mod location;
// M462：实时尾随器搬进自己的文件（本文件加了「整份重读」之后贴到 400 行）。
#[path = "logs/tail.rs"]
mod tail;

pub use location::{entries, legacy_root, note_location, root, sessions};
pub use tail::Tail;

/// 送进规则引擎的默认尾巴行数（与 CLI 同值，避免两条路看到不同证据）。
pub const DEFAULT_TAIL: usize = 2000;

/// 索引条目 → 左栏会话行。
pub fn session_of(entry: &SessionIndexEntry) -> LogSession {
    LogSession {
        id: entry.session_id.clone(),
        instance: if entry.instance_name.is_empty() {
            entry.instance_id.clone()
        } else {
            entry.instance_name.clone()
        },
        when: display_time(&entry.started_at),
        outcome: outcome_of(entry),
        duration_secs: entry.duration_ms / 1000,
    }
}

/// 结局：静默失败 > 崩溃（非 0 / crash_found） > 正常。
pub fn outcome_of(entry: &SessionIndexEntry) -> LogOutcome {
    if entry.silent_failure {
        LogOutcome::SilentFail
    } else if entry.crash_found
        || entry.exit_code.is_some_and(|code| code != 0)
        || (entry.exit_code.is_none() && !entry.ended_at.is_empty())
    {
        LogOutcome::Crashed
    } else {
        LogOutcome::Ok
    }
}

/// `2026-10-29T19:22:45.123Z` → `10-29 19:22`。
pub fn display_time(started_at: &str) -> String {
    let body = started_at.trim_end_matches('Z');
    let Some((date, time)) = body.split_once('T') else {
        return started_at.to_string();
    };
    let date = date.get(5..).unwrap_or(date);
    let time = time.get(..5).unwrap_or(time);
    format!("{date} {time}")
}

/// 事件流文件路径（`<logs>/<log_path>`；新根没有时回落到旧家）。
///
/// `log_path` 由写入侧按**相对会话根**的形状产出（`game/<id>.jsonl`，见
/// [`session_index::log_path_for`]），解析只走 [`session_index::event_file`] 一处——
/// CLI 与 GUI 两个读取点共用它，谁也不许再自己拼一层 `game/`（M211/D2）。
///
/// 两处都找不到就返回**新根**的路径：调用方据此说出「该去哪找」，而不是一个旧目录路径。
pub fn event_path(env: &HostEnv, entry: &SessionIndexEntry) -> PathBuf {
    let primary = session_index::event_file(&root(env), &entry.log_path);
    if primary.is_file() {
        return primary;
    }
    if let Some(legacy) = legacy_root(env) {
        let fallback = session_index::event_file(&legacy, &entry.log_path);
        if fallback.is_file() {
            return fallback;
        }
    }
    primary
}

/// 会话 id 或唯一前缀 → 索引条目（两个根的合并视图）。
fn find(env: &HostEnv, key: &str) -> Option<SessionIndexEntry> {
    let entries = entries(env);
    if let Some(exact) = entries.iter().find(|entry| entry.session_id == key) {
        return Some(exact.clone());
    }
    entries
        .iter()
        .find(|entry| entry.session_id.starts_with(key))
        .cloned()
}

/// 事件 JSON → 一行日志（`stdout`/`stderr` 的 `payload.line`）。
pub fn line_of(event: &Value) -> Option<LogLine> {
    let text = event.pointer("/payload/line").and_then(Value::as_str)?;
    let kind = event
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let level = match event
        .pointer("/payload/level")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "error" => LogLevel::Error,
        "warn" | "warning" => LogLevel::Warn,
        "debug" | "trace" => LogLevel::Debug,
        _ => {
            if kind == "stderr" {
                LogLevel::Warn
            } else {
                LogLevel::Info
            }
        }
    };
    Some(LogLine {
        text: text.to_string(),
        level,
    })
}

/// 整个事件流的原始 JSON 行（解析失败的行跳过，容忍被 kill 时截断的尾行）。
///
/// 一次读完所有事件：日志行与诊断结论都从这同一份读里取，不重复读盘。
pub fn read_events(path: &Path) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

/// 整个事件流 → 日志行（**展示面**：`payload.line`，即写盘前已脱敏的文本）。
pub fn read_lines(path: &Path) -> Vec<LogLine> {
    read_events(path).iter().filter_map(line_of).collect()
}

/// 打开一个会话：**索引里那一场的真 id** + 日志行 + 规则引擎结论。
///
/// 结论优先取会话里**落盘的那份**（写会话时用原始行算过，见 [`diagnosis`] 模块文档）；
/// 老会话没有它，才回落到「在脱敏文本上重跑规则」。
///
/// 头一个位置（M458）是 `None` 表示**这个 key 不在会话索引里**（`ImportCrashLog` 的
/// `file:<文件名>` 就是这种）：行与结论都为空，调用方也不许据此移动「选中项」——那个 id
/// 配不上 `log_sessions` 里的任何一行，移过去只会把选择器弄成「全部没选中」。
///
/// 它是 `find` 解析出来的**真 id**，不是调用方给的 `key`：`find` 接受唯一前缀，
/// 而选中项要拿去和 `log_sessions.id` 逐字比对。
pub fn open(
    env: &HostEnv,
    key: &str,
) -> (Option<SessionId>, Vec<LogLine>, Option<DiagnosisReport>) {
    let Some(entry) = find(env, key) else {
        return (None, Vec::new(), None);
    };
    let events = read_events(&event_path(env, &entry));
    let lines: Vec<LogLine> = events.iter().filter_map(line_of).collect();
    let diagnosis = diagnosis::stored(&entry, &events).or_else(|| diagnose_lines(&entry, &lines));
    (Some(entry.session_id), lines, diagnosis)
}

/// 在**脱敏文本**上重跑规则引擎（老会话的兜底路径）。
///
/// ⚠️ 这条路的证据是「写盘前脱敏过的」行：`≥64` 无点号长串（Sponge 类路径、长 hash）会被
/// 抹成 `<redacted:opaque>`，依赖那些字面量的规则（`fabric.loader_0_12`）在这里**不可能命中**。
/// 新会话通常不必走这条路——写会话时用原始行算过结论，见 [`diagnosis::stored`]。
pub fn diagnose_lines(entry: &SessionIndexEntry, lines: &[LogLine]) -> Option<DiagnosisReport> {
    let stderr_text: String = lines
        .iter()
        .filter(|line| line.level == LogLevel::Error || line.level == LogLevel::Warn)
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let body: String = lines
        .iter()
        .rev()
        .take(DEFAULT_TAIL)
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    if body.is_empty() {
        return None;
    }
    let error_lines = u32::try_from(
        lines
            .iter()
            .filter(|line| line.level == LogLevel::Error)
            .count(),
    )
    .unwrap_or(u32::MAX);
    let exit = ExitSignal::classify(
        entry.exit_code,
        Some(entry.duration_ms),
        error_lines,
        &stderr_text,
    );
    let mut input = DiagnosisInput::new().with_exit(exit);
    input.push_text(SignalOrigin::Stderr, body);
    let loader = RuleLoader::embedded();
    let report = Diagnoser::new(loader.rule_set().rules().to_vec()).analyze(&input);
    let finding = report
        .findings
        .first()
        .or_else(|| report.secondary.first())?;
    Some(DiagnosisReport {
        session: entry.session_id.clone(),
        diagnosis: Diagnosis {
            rule: finding.rule_id.clone(),
            cause_key: finding.message_key.clone(),
            severity: severity_of(finding.severity),
            confidence: confidence_percent(finding.confidence),
            groups: finding.groups.clone(),
            system_text: Some(finding.message.clone()),
            fix: finding
                .materialized_actions()
                .first()
                .map(|action| action.tool.to_string()),
        },
    })
}

/// 0.0~1.0 → 0~100（四舍五入）。
pub fn confidence_percent(confidence: f32) -> u8 {
    let percent = (confidence.clamp(0.0, 1.0) * 100.0).round();
    u8::try_from(percent as u32).unwrap_or(100)
}

/// core 的严重度 → DTO 的严重度（三值一一对应；`severity` 以前在半路丢了，页面只能拿置信度硬凑）。
pub fn severity_of(severity: Severity) -> DiagnosisSeverity {
    match severity {
        Severity::Info => DiagnosisSeverity::Info,
        Severity::Warning => DiagnosisSeverity::Warning,
        Severity::Critical => DiagnosisSeverity::Critical,
    }
}

/// 导入崩溃日志（`hs_err_pid*.log` / `crash-*.txt` / `latest.log`）做规则分析。
///
/// 文件名含 `hs_err` 时先走 `hs_err::parse`（结构化字段进规则引擎），否则整份文本当信号喂进去。
pub fn analyze_crash_file(path: &str) -> Result<(String, DiagnosisReport), String> {
    let path = Path::new(path);
    if !path.is_file() {
        return Err(format!("找不到崩溃日志：{}", path.display()));
    }
    let text = std::fs::read_to_string(path).map_err(|error| format!("读崩溃日志失败：{error}"))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "crash.log".to_string());
    let is_hs_err = name.contains("hs_err");
    let mut input = DiagnosisInput::new();
    if is_hs_err {
        input.push_hs_err(unlucky_core::diagnose::hs_err::parse(&text));
    }
    input.push_text(
        if is_hs_err {
            SignalOrigin::HsErr
        } else if name.contains("crash") {
            SignalOrigin::CrashReport
        } else {
            SignalOrigin::LatestLog
        },
        text,
    );
    let loader = RuleLoader::embedded();
    let report = Diagnoser::new(loader.rule_set().rules().to_vec()).analyze(&input);
    let finding = report
        .findings
        .first()
        .or_else(|| report.secondary.first())
        .ok_or_else(|| "规则引擎没有从这份日志里认出已知问题".to_string())?;
    Ok((
        format!("导入的日志：{name}"),
        DiagnosisReport {
            session: format!("file:{name}"),
            diagnosis: Diagnosis {
                rule: finding.rule_id.clone(),
                cause_key: finding.message_key.clone(),
                severity: severity_of(finding.severity),
                confidence: confidence_percent(finding.confidence),
                groups: finding.groups.clone(),
                system_text: Some(finding.message.clone()),
                fix: finding
                    .materialized_actions()
                    .first()
                    .map(|action| action.tool.to_string()),
            },
        },
    ))
}

#[cfg(test)]
#[path = "logs/m241_tests.rs"]
mod m241_tests;
#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;
