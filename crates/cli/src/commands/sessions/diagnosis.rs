//! `ull diag` 的结论来源：**先读会话里落盘的那份**，老会话才重跑规则（M242）。
//!
//! ## 为什么不能每次都在会话文本上重跑
//!
//! 会话 JSONL 是**写前脱敏**的（`launch::redact`，落盘即脱敏），而脱敏拿不准形状时是**有损**的：
//! `≥64` 的不透明串、带点的票据串都会被换成占位符（`<redacted:opaque>` / `<redacted:token>`）。
//! M240 收窄了这条规则——**路径形状**的长串现在整条保留，所以 M239 实测的那条
//! `org/spongepowered/…/FabricMixinTransformerProxy` 今天能活下来。但那是**白名单**：形状一变
//! （或又登记了别的 secret）就回到「抹掉」那一侧，而抹掉之后的重跑不是「少一点细节」，是
//! **换一条结论** —— `fabric.loader_0_12`（critical）降级成 `integrity.no_class_def_found`
//! （warning：把「Fabric Loader 版本不配套」说成「库没下全」）。
//!
//! M241 起写会话时已经在**原始行**上算过一次结论（`SessionWriter::finish` →
//! `diagnose::session::conclude`），落成 `diagnosis` 事件；那份结论自己还要再走一遍
//! `Redactor::scrub_json`，所以盘上只有脱敏后的结论，没有原文。M242 换的就是**证据**，
//! 不是判据：规则集的 pattern 一个字符都没动。
//!
//! ## 回落
//!
//! 没有 `diagnosis` 事件的老会话仍走原来的重跑（行为与 M242 之前完全一致）：短字面量的规则
//! （`memory.out_of_memory` 之类）在脱敏文本上照样命中；只有「靠长串证据」那类规则会降级
//! —— 那是老数据本身的代价，输出里的「结论来源」一行会把它说清。

use serde_json::Value;
use unlucky_core::diagnose::collectors::{ExitSignal, SignalOrigin};
use unlucky_core::diagnose::engine::{Diagnoser, DiagnosisInput};
use unlucky_core::diagnose::loader::RuleLoader;
use unlucky_core::launch::session_index::SessionIndexEntry;

use crate::error::CliError;

use super::exit_label;

/// 结论从哪儿来（人类输出与 `--json` 都要说清，否则用户分不清「盘上的」与「刚算的」——
/// 这两者在长串证据类规则上给出的结论**可能不同**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// 会话里落盘的 `diagnosis` 事件（M241 起，写会话时在原始行上算的）。
    Stored,
    /// 老会话：在脱敏文本上重跑规则。
    Rerun,
}

impl Provenance {
    /// `--json` 里的值（`stored` / `rerun`）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stored => "stored",
            Self::Rerun => "rerun",
        }
    }

    /// 人类输出里的一行说明。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Stored => "会话落盘的 diagnosis 事件（写会话时在原始行上算的，已脱敏后落盘）",
            Self::Rerun => "在脱敏文本上重跑规则（老会话没有落盘结论，靠长串证据的规则可能降级）",
        }
    }
}

/// 一次诊断：结论本体 + 出处。
#[derive(Debug, Clone)]
pub struct Conclusion {
    /// `diagnosis` 记录（`DiagnosisRecord` 的 JSON），两条来源同形。
    pub record: Value,
    /// 从哪儿来。
    pub provenance: Provenance,
    /// 重跑时评估了多少条规则（落盘那份不记这个数，所以是 `Option`）。
    pub rules_evaluated: Option<usize>,
}

/// 取这个会话的诊断结论：优先落盘的那份，老会话才重跑。
///
/// # Errors
///
/// 重跑路径上结论序列化失败时返回内部错误。
pub fn conclusion(
    entry: &SessionIndexEntry,
    events: &[Value],
    tail: usize,
) -> Result<Conclusion, CliError> {
    if let Some(record) = stored(events) {
        return Ok(Conclusion {
            record,
            provenance: Provenance::Stored,
            rules_evaluated: None,
        });
    }
    let (record, rules_evaluated) = rerun(entry, events, tail)?;
    Ok(Conclusion {
        record,
        provenance: Provenance::Rerun,
        rules_evaluated: Some(rules_evaluated),
    })
}

/// 事件流里最后一条 `diagnosis` 事件的 payload。
///
/// 正常只有一条（挂钩写的或收尾补的，二选一）；真有多条取**最后一条**：收尾补的证据最新鲜。
/// 没有主因（`candidates` 空）的 payload 与「没结论」等价——`conclude` 也不会写它——此时
/// 回落到重跑，而不是拿一份空结论顶掉。
fn stored(events: &[Value]) -> Option<Value> {
    let payload = events
        .iter()
        .rev()
        .find(|event| event.get("kind").and_then(Value::as_str) == Some("diagnosis"))?
        .get("payload")?;
    let has_primary = payload
        .get("candidates")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty());
    has_primary.then(|| payload.clone())
}

/// 老会话的回落：在脱敏文本上重跑规则，返回 `(结论记录, 评估条数)`。
fn rerun(
    entry: &SessionIndexEntry,
    events: &[Value],
    tail: usize,
) -> Result<(Value, usize), CliError> {
    let mut stdout_lines: Vec<&str> = Vec::new();
    let mut stderr_lines: Vec<&str> = Vec::new();
    let mut error_lines: u32 = 0;
    for event in events {
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
    let stdout_text = tail_join(&stdout_lines, tail);
    let stderr_text = tail_join(&stderr_lines, tail);

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
    Ok((record, report.rules_evaluated))
}

/// 人类可读的几行（未脱敏——调用方统一过一次 `redact_text`）。
pub fn lines(entry: &SessionIndexEntry, found: &Conclusion) -> Vec<String> {
    let mut human = vec![
        format!(
            "会话 {} · {} · {} · {}",
            entry.session_id,
            entry.instance_name,
            entry.game_version,
            exit_label(entry)
        ),
        format!("结论来源: {}", found.provenance.label()),
    ];
    match found.record.get("candidates").and_then(Value::as_array) {
        Some(items) if !items.is_empty() => {
            let primary = &items[0];
            if let Some(message) = text_at(primary, "message") {
                human.push(format!("主因: {message}"));
            }
            if let Some(detail) = text_at(primary, "detail") {
                human.push(format!("说明: {detail}"));
            }
            for evidence in evidence_of(primary, 3) {
                human.push(format!("证据: {evidence}"));
            }
        }
        _ => human.push("规则引擎没有得出确定结论（needs_more_info）".to_string()),
    }
    human.push(counts_line(&found.record, found.rules_evaluated));
    human
}

/// `规则评估 N 条 · 结论 N 条 · 次生 N 条`（落盘那份没有「评估条数」，省掉第一段）。
fn counts_line(record: &Value, rules_evaluated: Option<usize>) -> String {
    let count = |key: &str| {
        record
            .get(key)
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    };
    let mut line = String::new();
    if let Some(evaluated) = rules_evaluated {
        line.push_str(&format!("规则评估 {evaluated} 条 · "));
    }
    line.push_str(&format!(
        "结论 {} 条 · 次生 {} 条",
        count("candidates"),
        count("secondary")
    ));
    line
}

fn text_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn evidence_of(candidate: &Value, limit: usize) -> Vec<String> {
    candidate
        .get("evidence")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(limit)
        .map(str::to_string)
        .collect()
}

/// 取末尾 `tail` 行并拼成文本（与重跑路径的老行为逐字一致）。
fn tail_join(lines: &[&str], tail: usize) -> String {
    let start = lines.len().saturating_sub(tail.max(1));
    let mut text = lines[start..].join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}
