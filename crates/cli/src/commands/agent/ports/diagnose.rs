//! 诊断端口：调用 M15 的规则引擎（**规则先跑，模型补位**，02 §3.2）。
//!
//! 工具层只把「规则结论 + 相关片段」交给模型，不把裸日志倒进去——这是 token 预算
//! 与结论质量的双重考虑，也是 analyse/02 §8.3 的分层思路。
//!
//! ## M242：结论读**会话里落盘的那份**，不在脱敏文本上重跑
//!
//! 会话 JSONL 是写前脱敏的，而脱敏拿不准形状时是**有损**的：`≥64` 的不透明串、带点的票据串
//! 都会被换成占位符。M240 收窄了这条规则——**路径形状**的长串现在整条保留，所以 M239 实测的
//! 那条 `org/spongepowered/…/FabricMixinTransformerProxy` 今天能活下来。但那是**白名单**：
//! 形状一变（或又登记了别的 secret）就回到「抹掉」那一侧，而抹掉之后的重跑不是「少一点细节」，
//! 是**换一条结论** —— `fabric.loader_0_12`（critical）降级成 `integrity.no_class_def_found`
//! （warning），把「Fabric Loader 版本不配套」说成「库没下全」。M241 起写会话时已经在
//! **原始行**上算过一次结论并落成 `diagnosis` 事件，这里优先读它；没有该事件的老会话才回落到
//! 重跑（与今天的读法完全一致）。
//!
//! **隐私选择不变**：给模型的游戏日志仍是脱敏文本（`read_game_log` 与这里的 `evidence`
//! 片段都没动）。变的只是「结论从哪儿来」——盘上那份结论自己也是写入时过 `scrub_json` 的，
//! 所以既没把原文写进盘，也没把原文交给模型。

use std::collections::BTreeMap;

use serde_json::Value;
use unlucky_agent::port::{
    CrashDiagnosis, DiagnosePort, DiagnosisCandidate, ExitSummary, FixAction, FixActionKind,
    PortResult,
};
use unlucky_core::diagnose::collectors::{ExitSignal, SignalOrigin};
use unlucky_core::diagnose::engine::{Diagnoser, DiagnosisInput, Finding};
use unlucky_core::diagnose::loader::RuleLoader;
use unlucky_core::diagnose::rules::FixActionKind as CoreFixKind;
use unlucky_core::launch::session_index::SessionIndexEntry;

use super::events::{tail_join, SessionSource};

#[cfg(test)]
#[path = "diagnose/tests.rs"]
mod tests;

/// 送进规则引擎的默认尾巴行数（与 CLI `diag` 同值，避免两条路看到不同证据）。
pub const DEFAULT_TAIL: usize = 2000;

/// 会话级证据片段条数（与重跑路径同值：`stderr` 末尾 3 行）。
const EVIDENCE_LINES: usize = 3;

/// M15 规则引擎的端口实现。
#[derive(Debug, Clone)]
pub struct CoreDiagnose {
    source: SessionSource,
    tail: usize,
}

impl CoreDiagnose {
    /// `root` = `<数据根>/.ULL/logs`（M188 起；旧名是 `data/agent/sessions`）。
    #[must_use]
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            source: SessionSource::new(root),
            tail: DEFAULT_TAIL,
        }
    }

    /// 再挂一个只读兼容的旧根（`data/agent/sessions`）：旧会话也能诊断。
    #[must_use]
    pub fn with_legacy(mut self, legacy: impl Into<std::path::PathBuf>) -> Self {
        self.source = self.source.with_legacy(legacy);
        self
    }
}

fn map_kind(kind: CoreFixKind) -> FixActionKind {
    match kind {
        CoreFixKind::ChangeJava => FixActionKind::ChangeJava,
        CoreFixKind::AdjustMemory => FixActionKind::AdjustMemory,
        CoreFixKind::ChangeGraphics => FixActionKind::ChangeGraphics,
        CoreFixKind::RemoveMod => FixActionKind::RemoveMod,
        CoreFixKind::Reinstall => FixActionKind::Reinstall,
        CoreFixKind::SeekHelp => FixActionKind::SeekHelp,
    }
}

fn candidate_of(finding: &Finding) -> DiagnosisCandidate {
    let core_candidate = finding.to_candidate();
    DiagnosisCandidate {
        id: core_candidate.id,
        severity: core_candidate.severity.to_string(),
        category: core_candidate.category.to_string(),
        confidence: core_candidate.confidence,
        message_key: core_candidate.message_key,
        message: core_candidate.message,
        detail: core_candidate.detail.unwrap_or_default(),
        evidence: core_candidate.evidence,
        fix_actions: core_candidate
            .fix_actions
            .into_iter()
            .map(|action| FixAction {
                kind: map_kind(action.kind),
                tool: action.tool.to_string(),
                label: String::new(),
                args: action
                    .args
                    .into_iter()
                    .collect::<BTreeMap<String, String>>(),
            })
            .collect(),
    }
}

// ─────────────────── 落盘的结论（`diagnosis` 事件） ───────────────────
//
// 下面三个是 `unlucky_core::diagnose::finding::{DiagnosisRecord, Candidate,
// MaterializedFixAction}` 的**只读镜像**：core 侧那些类型只 derive `Serialize`（写盘用），
// agent 侧的类型也只 derive `Serialize`（回灌给模型用），所以「读回来」这一件事在这里落一次。

/// `diagnosis` 事件的 payload。
#[derive(Debug, Default, serde::Deserialize)]
struct StoredRecord {
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    needs_more_info: bool,
    #[serde(default)]
    candidates: Vec<StoredCandidate>,
    #[serde(default)]
    secondary: Vec<StoredCandidate>,
}

/// 一条落盘的候选结论。
#[derive(Debug, Default, serde::Deserialize)]
struct StoredCandidate {
    #[serde(default)]
    id: String,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    confidence: f32,
    #[serde(default)]
    message_key: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    evidence: Vec<String>,
    #[serde(default)]
    fix_actions: Vec<StoredAction>,
}

/// 一条落盘的修复动作（`kind` 是 `snake_case` 的六维之一）。
#[derive(Debug, Default, serde::Deserialize)]
struct StoredAction {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    tool: String,
    #[serde(default)]
    args: BTreeMap<String, String>,
}

impl StoredRecord {
    /// 事件流里最后一条**有主因**的 `diagnosis` 事件。
    ///
    /// 正常只有一条（挂钩写的或收尾补的，二选一）；真有多条取**最后一条**：收尾补的证据最新鲜。
    /// 没有候选的 payload 与「没结论」等价（`conclude` 也不会写它）→ 返回 `None`，让调用方回落重跑。
    fn find(events: &[Value]) -> Option<Self> {
        let payload = events
            .iter()
            .rev()
            .find(|event| event.get("kind").and_then(Value::as_str) == Some("diagnosis"))?
            .get("payload")?;
        let record: Self = serde_json::from_value(payload.clone()).ok()?;
        (!record.candidates.is_empty()).then_some(record)
    }

    /// 落盘结论 → 给模型的 [`CrashDiagnosis`]。
    ///
    /// `exit` 仍取自索引条目（launch 报的事实，与盘上那份结论的计算输入同源），
    /// `evidence` 仍是**脱敏**的 stderr 尾窗——隐私选择不变，只有结论换了来源。
    fn into_diagnosis(self, entry: &SessionIndexEntry, evidence: Vec<String>) -> CrashDiagnosis {
        CrashDiagnosis {
            session_id: entry.session_id.clone(),
            source: self.source.unwrap_or_else(|| "rules".to_string()),
            needs_more_info: self.needs_more_info,
            candidates: self
                .candidates
                .into_iter()
                .map(StoredCandidate::into_candidate)
                .collect(),
            secondary: self
                .secondary
                .into_iter()
                .map(StoredCandidate::into_candidate)
                .collect(),
            evidence,
            exit: ExitSummary {
                code: entry.exit_code,
                exit_type: entry.exit_type.clone(),
                silent_failure: entry.silent_failure,
            },
        }
    }
}

impl StoredCandidate {
    fn into_candidate(self) -> DiagnosisCandidate {
        DiagnosisCandidate {
            id: self.id,
            severity: self.severity,
            category: self.category,
            confidence: self.confidence,
            message_key: self.message_key,
            message: self.message,
            detail: self.detail.unwrap_or_default(),
            evidence: self.evidence,
            fix_actions: self
                .fix_actions
                .into_iter()
                .filter_map(StoredAction::into_action)
                .collect(),
        }
    }
}

impl StoredAction {
    /// 未知维度返回 `None`：宁可少一条建议，也不瞎猜成别的维度（六维口径在 core 一处）。
    fn into_action(self) -> Option<FixAction> {
        let kind = match self.kind.as_str() {
            "change_java" => FixActionKind::ChangeJava,
            "adjust_memory" => FixActionKind::AdjustMemory,
            "change_graphics" => FixActionKind::ChangeGraphics,
            "remove_mod" => FixActionKind::RemoveMod,
            "reinstall" => FixActionKind::Reinstall,
            "seek_help" => FixActionKind::SeekHelp,
            _ => return None,
        };
        Some(FixAction {
            kind,
            tool: self.tool,
            label: String::new(),
            args: self.args,
        })
    }
}

impl DiagnosePort for CoreDiagnose {
    fn diagnose(&self, session_id: &str) -> PortResult<CrashDiagnosis> {
        let entry = self.source.find(session_id)?;
        let events = self.source.events(&entry)?;
        let (stdout, stderr, error_lines) = SessionSource::split_text(&events);

        // 会话级证据片段：仍是**脱敏**文本（隐私选择不变）。
        let evidence: Vec<String> = stderr.iter().rev().take(EVIDENCE_LINES).cloned().collect();

        // 盘上有结论就用它——那是写会话时在原始行上算的，不是这份脱敏文本能重算出来的。
        if let Some(record) = StoredRecord::find(&events) {
            return Ok(record.into_diagnosis(&entry, evidence));
        }

        // 老会话（M241 之前的 JSONL）：回落到在脱敏文本上重跑。
        let exit = ExitSignal::classify(
            entry.exit_code,
            Some(entry.duration_ms),
            error_lines,
            &stderr.join("\n"),
        );
        let mut input = DiagnosisInput::new().with_exit(exit);
        let stdout_text = tail_join(&stdout, self.tail);
        if !stdout_text.is_empty() {
            input.push_text(SignalOrigin::Stdout, stdout_text);
        }
        let stderr_text = tail_join(&stderr, self.tail);
        if !stderr_text.is_empty() {
            input.push_text(SignalOrigin::Stderr, stderr_text);
        }

        let loader = RuleLoader::embedded();
        let diagnoser = Diagnoser::new(loader.rule_set().rules().to_vec());
        let report = diagnoser.analyze(&input);

        Ok(CrashDiagnosis {
            session_id: entry.session_id.clone(),
            source: "rules".to_string(),
            needs_more_info: report.needs_more_info(),
            candidates: report.findings.iter().map(candidate_of).collect(),
            secondary: report.secondary.iter().map(candidate_of).collect(),
            evidence,
            exit: ExitSummary {
                code: entry.exit_code,
                exit_type: entry.exit_type.clone(),
                silent_failure: entry.silent_failure,
            },
        })
    }
}
