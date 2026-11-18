//! 取消报告的**三档诚实口径**：`ImportReport::cancel` 的直接用例（不需要跑管线）。
//!
//! 从 `cancel_tests.rs` 拆出来只为行数：那边装下五个端到端用例就够满了。

use crate::addon::import::report::{FileAction, FileSource, ImportReport, ImportedFile};
use crate::addon::import::stage::RollbackOutcome;
use crate::addon::import::ImportStatus;

fn base() -> ImportReport {
    ImportReport::empty(std::path::PathBuf::from("instance"))
}

fn notes(report: &ImportReport) -> Vec<&str> {
    report
        .warnings
        .iter()
        .map(|note| note.message.as_str())
        .collect()
}

#[test]
fn a_cancel_before_writing_never_claims_the_target_was_touched() {
    let mut report = base();
    report.cancel(RollbackOutcome::default());

    assert_eq!(report.status, ImportStatus::Cancelled);
    assert!(report.error.is_none(), "取消不许带错误");
    assert!(report.rolled_back);
    assert!(report.rollback.is_none(), "没做过回滚就不该有回滚记录");
    let notes = notes(&report);
    assert!(
        notes.iter().any(|line| line.contains("未被写入")),
        "{notes:?}"
    );
    assert!(
        !notes.iter().any(|line| line.contains("恢复为导入前")),
        "{notes:?}"
    );
}

#[test]
fn a_cancel_after_commit_says_it_rolled_back_exactly_once() {
    let mut report = base();
    report.cancel(RollbackOutcome {
        attempted: 2,
        failed: Vec::new(),
    });

    assert_eq!(report.status, ImportStatus::Cancelled);
    assert!(report.rolled_back);
    assert_eq!(
        report.rollback.as_ref().map(RollbackOutcome::failure_count),
        Some(0)
    );
    let notes = notes(&report);
    assert!(
        notes.iter().any(|line| line.contains("恢复为导入前")),
        "{notes:?}"
    );
    assert!(
        !notes.iter().any(|line| line.contains("未被写入")),
        "{notes:?}"
    );
}

#[test]
fn an_incomplete_cancel_rollback_never_claims_consistency() {
    let mut report = base();
    report.cancel(RollbackOutcome {
        attempted: 3,
        failed: vec!["删除 config/a.toml 失败：拒绝访问".to_string()],
    });

    assert_eq!(report.status, ImportStatus::Cancelled);
    assert!(!report.rolled_back, "回滚不完整时不许宣称一致");
    let notes = notes(&report);
    assert!(
        notes
            .iter()
            .any(|line| line.contains("未能回滚") || line.contains("未完成")),
        "{notes:?}"
    );
    assert!(
        !notes
            .iter()
            .any(|line| line.contains("恢复为导入前的样子") || line.contains("未被写入")),
        "{notes:?}"
    );
}

#[test]
fn cancelling_clears_the_per_file_ledger_it_never_landed() {
    // 取消前的 `files` 是"处理过"而不是"落地过"：留着就是假账（UI 会照它画"已装 N 个"）。
    let mut report = base();
    report.bytes_written = 4096;
    report.files.push(ImportedFile {
        path: "mods/a.jar".to_string(),
        source: FileSource::Overrides,
        sha1: None,
        bytes: 4096,
        disabled: false,
        action: FileAction::Installed,
        note: None,
    });

    report.cancel(RollbackOutcome::default());

    assert!(report.files.is_empty(), "取消后不许留下「已落地」的账");
    assert_eq!(report.bytes_written, 0);
    assert_eq!(report.installed(), 0);
}
