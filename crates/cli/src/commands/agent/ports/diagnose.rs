//! 诊断端口：调用 M15 的规则引擎（**规则先跑，模型补位**，02 §3.2）。
//!
//! 工具层只把「规则结论 + 相关片段」交给模型，不把裸日志倒进去——这是 token 预算
//! 与结论质量的双重考虑，也是 analyse/02 §8.3 的分层思路。

use std::collections::BTreeMap;

use unlucky_agent::port::{
    CrashDiagnosis, DiagnosePort, DiagnosisCandidate, ExitSummary, FixAction, FixActionKind,
    PortResult,
};
use unlucky_core::diagnose::collectors::{ExitSignal, SignalOrigin};
use unlucky_core::diagnose::engine::{Diagnoser, DiagnosisInput, Finding};
use unlucky_core::diagnose::loader::RuleLoader;
use unlucky_core::diagnose::rules::FixActionKind as CoreFixKind;

use super::events::{tail_join, SessionSource};

/// 送进规则引擎的默认尾巴行数（与 CLI `diag` 同值，避免两条路看到不同证据）。
pub const DEFAULT_TAIL: usize = 2000;

/// M15 规则引擎的端口实现。
#[derive(Debug, Clone)]
pub struct CoreDiagnose {
    source: SessionSource,
    tail: usize,
}

impl CoreDiagnose {
    /// `root` = `<数据根>/.ULL/data/agent/sessions`。
    #[must_use]
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            source: SessionSource::new(root),
            tail: DEFAULT_TAIL,
        }
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

impl DiagnosePort for CoreDiagnose {
    fn diagnose(&self, session_id: &str) -> PortResult<CrashDiagnosis> {
        let entry = self.source.find(session_id)?;
        let events = self.source.events(&entry)?;
        let (stdout, stderr, error_lines) = SessionSource::split_text(&events);

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
            evidence: stderr.iter().rev().take(3).cloned().collect(),
            exit: ExitSummary {
                code: entry.exit_code,
                exit_type: entry.exit_type.clone(),
                silent_failure: entry.silent_failure,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn seed(root: &std::path::Path, stderr_line: &str) {
        let game = root.join("game");
        std::fs::create_dir_all(&game).expect("建目录");
        let entry = json!({
            "v": 1,
            "session_id": "20261029T124501-9f3c2a",
            "instance_id": "inst-1",
            "instance_name": "1.20.1 Forge",
            "game_version": "1.20.1",
            "started_at": "2026-10-29T12:45:01.123Z",
            "ended_at": "2026-10-29T12:45:05.123Z",
            "duration_ms": 4000,
            "exit_code": 1,
            "exit_type": "application_error",
            "silent_failure": false,
            "window_detected": false,
            "event_count": 1,
            "log_path": "20261029T124501-9f3c2a.jsonl",
            "crash_found": true,
            "diagnosis_top": null
        });
        std::fs::write(game.join("index.jsonl"), format!("{entry}\n")).expect("写索引");
        let event = json!({
            "seq": 0, "ts": "2026-10-29T12:45:01.123Z", "t_ms": 0,
            "kind": "stderr",
            "payload": { "line": stderr_line, "level": "error" }
        });
        std::fs::write(
            game.join("20261029T124501-9f3c2a.jsonl"),
            format!("{event}\n"),
        )
        .expect("写事件");
    }

    #[test]
    fn runs_the_rule_engine_over_a_session() {
        let root = std::env::temp_dir().join(format!("ull-mcp-diagnose-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        seed(
            &root,
            "java.lang.NoClassDefFoundError: cpw/mods/bootstraplauncher/BootstrapLauncher",
        );

        let port = CoreDiagnose::new(root.clone());
        let report = port.diagnose("20261029T124501-9f3c2a").expect("应能诊断");
        assert_eq!(report.session_id, "20261029T124501-9f3c2a");
        assert_eq!(report.source, "rules");
        assert_eq!(report.exit.code, Some(1));
        // 规则集里应有 JVM/完整性类结论命中这条 stderr
        assert!(!report.candidates.is_empty(), "应至少命中一条规则");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_session_is_a_readable_error() {
        let root =
            std::env::temp_dir().join(format!("ull-mcp-diagnose-none-{}", std::process::id()));
        let port = CoreDiagnose::new(root);
        let err = port.diagnose("nope").unwrap_err();
        assert!(err.contains("找不到会话"), "{err}");
    }
}
