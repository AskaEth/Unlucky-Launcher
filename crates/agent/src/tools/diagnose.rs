//! `diagnose_crash`（02 文档 §3.2 第三条）。
//!
//! 流水线是「**规则在前、模型补位**」（analyse/02 §8.3）：本工具只跑 M15 的规则引擎，
//! 把**结构化结论 + 关键片段**交给模型，绝不让模型读裸日志。规则未命中
//! （`candidates` 空 / `needs_more_info`）时，模型再去用 `read_game_log` 自己找证据。
//!
//! P0 与 M15 的桥：M15 的修复动作指向 `java.set` / `memory.set` 这些 **P1 才有的**语义工具 id。
//! P0 只有 `set_config`，所以这里按 [`FixActionKind`] 生成 `p0_next_steps`
//! （`adjust_memory` → `set_config {"path":"java.default_memory_mb"}`，`change_java` →
//! `set_config {"path":"java.selected_path"}`，见 [`MEMORY_KEY`] / [`JAVA_KEY`]），
//! 剩下的动作明确标成「P0 无工具，解释给用户」。

use std::sync::Arc;

use serde_json::{json, Value};

use crate::port::{CrashDiagnosis, DiagnosePort, DiagnosisCandidate, FixAction, FixActionKind};
use crate::redact::redact_text;
use crate::registry::schema::{integer, non_empty_string, object};
use crate::registry::{
    RegistryError, ToolCtx, ToolError, ToolFn, ToolOutput, ToolRegistry, ToolSpec,
};

/// 诊断工具名。
pub const DIAGNOSE_CRASH: &str = "diagnose_crash";
/// 单次返回的证据行上限（防 token 爆炸）。
pub const MAX_EVIDENCE_LINES: usize = 40;
/// 证据行默认条数。
pub const DEFAULT_EVIDENCE_LINES: usize = 20;
/// 单条证据最大字符数。
pub const MAX_EVIDENCE_CHARS: usize = 400;
/// 内存建议的配置键（M12 白名单里的键名，见 `setting-接口约定.md` §8）。
pub const MEMORY_KEY: &str = "java.default_memory_mb";
/// Java 路径的配置键。
pub const JAVA_KEY: &str = "java.selected_path";

/// 注册诊断工具。
///
/// # Errors
/// 工具名重复/schema 非法时返回 [`RegistryError`]。
pub fn register(
    registry: &mut ToolRegistry,
    port: Arc<dyn DiagnosePort>,
) -> Result<(), RegistryError> {
    let handler: ToolFn = Arc::new(move |_ctx: &ToolCtx, args: &Value| {
        let session_id = args
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::NotAllowed("缺少参数 `session_id`".to_string()))?;
        let max_lines = args
            .get("max_evidence_lines")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_EVIDENCE_LINES, |n| n as usize)
            .clamp(1, MAX_EVIDENCE_LINES);
        let report = port.diagnose(session_id).map_err(ToolError::Port)?;
        Ok(render(&report, max_lines))
    });

    registry.register(ToolSpec::read_only(
        DIAGNOSE_CRASH,
        "对某次游戏会话跑规则引擎（59 条崩溃规则）并返回结构化结论：主因、置信度、证据行、修复动作。规则未命中时会提示需要更多信息——此时再用 read_game_log 取 stderr/exit 片段。",
        object(
            &[
                ("session_id", non_empty_string("会话 id，来自 list_game_sessions")),
                (
                    "max_evidence_lines",
                    integer("返回多少条证据行", 1, MAX_EVIDENCE_LINES as i64),
                ),
            ],
            &["session_id"],
        ),
        handler,
    ))
}

/// 把规则引擎结论压成工具结果。
fn render(report: &CrashDiagnosis, max_evidence_lines: usize) -> ToolOutput {
    let candidates: Vec<Value> = report
        .candidates
        .iter()
        .map(|c| render_candidate(c, max_evidence_lines))
        .collect();
    let secondary: Vec<Value> = report
        .secondary
        .iter()
        .take(5)
        .map(|c| {
            json!({
                "id": c.id,
                "severity": c.severity,
                "category": c.category,
                "confidence": c.confidence,
                "message": message_of(c),
            })
        })
        .collect();
    let next_steps: Vec<Value> = report
        .candidates
        .iter()
        .flat_map(|c| c.fix_actions.iter())
        .map(bridge_to_p0)
        .collect();

    let content = json!({
        "session_id": report.session_id,
        "source": report.source,
        "needs_more_info": report.needs_more_info,
        "exit": report.exit,
        "top_confidence": report.top_confidence(),
        "candidates": candidates,
        "secondary": secondary,
        "evidence": cap_lines(&report.evidence, max_evidence_lines),
        "p0_next_steps": next_steps,
    });

    ToolOutput::ok(content, summarize(report))
}

fn render_candidate(candidate: &DiagnosisCandidate, max_evidence_lines: usize) -> Value {
    json!({
        "id": candidate.id,
        "severity": candidate.severity,
        "category": candidate.category,
        "confidence": candidate.confidence,
        "message": message_of(candidate),
        "detail": candidate.detail,
        "evidence": cap_lines(&candidate.evidence, max_evidence_lines),
        "fix_actions": candidate.fix_actions,
    })
}

/// `message` 为空时回落显示 i18n key（装配层没渲染时至少不丢信息）。
fn message_of(candidate: &DiagnosisCandidate) -> String {
    let text = candidate.message.trim();
    if text.is_empty() {
        candidate.message_key.clone()
    } else {
        text.to_string()
    }
}

fn summarize(report: &CrashDiagnosis) -> String {
    match report.top() {
        Some(top) => {
            let label = top
                .fix_actions
                .first()
                .map(action_label)
                .unwrap_or_else(|| "没有可执行的修复动作".to_string());
            format!(
                "规则引擎命中 {} 条候选，主因 `{}`（置信度 {:.2}）；建议：{label}",
                report.candidates.len(),
                top.id,
                top.confidence
            )
        }
        None if report.needs_more_info => {
            "规则引擎未命中，且需要更多信息（用 read_game_log 取 stderr/exit 片段后再判断）"
                .to_string()
        }
        None => "规则引擎未命中（59 条规则都不匹配），需要结合日志片段人工/AI 判断".to_string(),
    }
}

/// M15 的语义动作 → P0 现有工具。返回给模型看的"下一步"。
fn bridge_to_p0(action: &FixAction) -> Value {
    let label = action_label(action);
    match action.kind {
        FixActionKind::AdjustMemory => {
            let value = pick_memory_mb(action).map_or(Value::Null, |mb| json!(mb));
            json!({
                "action": action.tool,
                "kind": action.kind,
                "label": label,
                "suggested_tool": "set_config",
                "suggested_args": { "path": MEMORY_KEY, "value": value },
            })
        }
        FixActionKind::ChangeJava => {
            let value = pick_arg(action, &["path", "java_path", "binary", "value"])
                .map_or(Value::Null, |p| json!(p));
            json!({
                "action": action.tool,
                "kind": action.kind,
                "label": label,
                "suggested_tool": "set_config",
                "suggested_args": { "path": JAVA_KEY, "value": value },
            })
        }
        other => json!({
            "action": action.tool,
            "kind": other,
            "label": label,
            "suggested_tool": Value::Null,
            "unavailable_reason": "P0 没有对应工具（P1 落地），请把这个建议解释给用户",
        }),
    }
}

/// 动作的人话：优先 M15 给的 `label`，否则按 kind 兜底。
fn action_label(action: &FixAction) -> String {
    let label = action.label.trim();
    if !label.is_empty() {
        return label.to_string();
    }
    match action.kind {
        FixActionKind::ChangeJava => "换一个匹配的 Java 版本".to_string(),
        FixActionKind::AdjustMemory => "调整最大内存".to_string(),
        FixActionKind::ChangeGraphics => "调整图形设置/驱动".to_string(),
        FixActionKind::RemoveMod => "移除或替换冲突的 Mod".to_string(),
        FixActionKind::Reinstall => "重装/修复对应组件".to_string(),
        FixActionKind::SeekHelp => "导出诊断包并反馈".to_string(),
    }
}

/// 从动作参数里挑内存上限：先看常见键，再退化成"第一个带数字的值"。
fn pick_memory_mb(action: &FixAction) -> Option<i64> {
    if let Some(value) = pick_arg(
        action,
        &["value", "suggest", "mb", "max_mb", "xmx", "xmx_mb"],
    ) {
        if let Some(mb) = parse_mb(&value) {
            return Some(mb);
        }
    }
    action.args.values().find_map(|v| parse_mb(v))
}

fn pick_arg(action: &FixAction, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| action.args.get(*key).cloned())
        .filter(|v| !v.trim().is_empty())
}

/// `"4096"` / `"4G"` / `"6144M"` → MB。
fn parse_mb(raw: &str) -> Option<i64> {
    let text = raw.trim().to_ascii_lowercase();
    let (number, scale) = if let Some(rest) = text.strip_suffix('g') {
        (rest, 1024)
    } else if let Some(rest) = text.strip_suffix("gb") {
        (rest, 1024)
    } else if let Some(rest) = text.strip_suffix('m') {
        (rest, 1)
    } else if let Some(rest) = text.strip_suffix("mb") {
        (rest, 1)
    } else {
        (text.as_str(), 1)
    };
    number.trim().parse::<i64>().ok().map(|n| n * scale)
}

fn cap_lines(lines: &[String], max_lines: usize) -> Vec<String> {
    lines
        .iter()
        .take(max_lines)
        .map(|line| {
            let masked = redact_text(line);
            if masked.chars().count() <= MAX_EVIDENCE_CHARS {
                masked
            } else {
                let head: String = masked.chars().take(MAX_EVIDENCE_CHARS).collect();
                format!("{head}…")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockDiagnose;

    fn call(port: &MockDiagnose, session_id: &str) -> ToolOutput {
        let report = port.diagnose(session_id).expect("mock 应返回结论");
        render(&report, DEFAULT_EVIDENCE_LINES)
    }

    #[test]
    fn renders_candidates_with_p0_bridge_for_reinstall() {
        let out = call(&MockDiagnose::new(), "s-1");
        assert_eq!(out.content["candidates"].as_array().unwrap().len(), 1);
        let confidence = out.content["top_confidence"].as_f64().unwrap();
        assert!((confidence - 0.86).abs() < 1e-6, "{confidence}");
        let steps = out.content["p0_next_steps"].as_array().unwrap();
        assert_eq!(steps[0]["kind"], json!("reinstall"));
        assert_eq!(steps[0]["suggested_tool"], Value::Null);
        assert!(out.summary.contains("建议：重装该版本的 Forge 组件"));
    }

    #[test]
    fn bridges_memory_action_to_set_config() {
        let mut report = MockDiagnose::forge_missing_library();
        if let Some(candidate) = report.candidates.first_mut() {
            candidate.fix_actions = vec![FixAction {
                kind: FixActionKind::AdjustMemory,
                tool: "memory.set".to_string(),
                label: String::new(),
                args: [("suggest".to_string(), "4G".to_string())]
                    .into_iter()
                    .collect(),
            }];
        }
        let out = render(&report, 5);
        let steps = out.content["p0_next_steps"].as_array().unwrap();
        assert_eq!(steps[0]["suggested_tool"], json!("set_config"));
        assert_eq!(steps[0]["suggested_args"]["path"], json!(MEMORY_KEY));
        assert_eq!(steps[0]["suggested_args"]["value"], json!(4096));
        assert_eq!(steps[0]["label"], json!("调整最大内存"));
    }

    #[test]
    fn parses_memory_units() {
        assert_eq!(parse_mb("4096"), Some(4096));
        assert_eq!(parse_mb("4G"), Some(4096));
        assert_eq!(parse_mb("6144mb"), Some(6144));
        assert_eq!(parse_mb("abc"), None);
    }

    #[test]
    fn empty_candidates_says_need_more_info() {
        let report = CrashDiagnosis {
            session_id: "s-2".into(),
            source: "rules".into(),
            needs_more_info: true,
            candidates: vec![],
            secondary: vec![],
            evidence: vec![],
            exit: crate::port::ExitSummary::default(),
        };
        let out = render(&report, 5);
        assert!(out.summary.contains("需要更多信息"));
    }
}
