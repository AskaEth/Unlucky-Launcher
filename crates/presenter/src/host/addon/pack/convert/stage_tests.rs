//! M161 复审 P2-1：`failure_stage` 是⑥三档的**唯一**判定点，此前零覆盖（把它改成恒
//! `RolledBack`，presenter 254 条全绿）。这里直接喂 core 的机器位，断言的是**判定**，
//! 不是渲染（渲染在 app-iced 的 `packui/model/tests/stage.rs`）。
//!
//! 抓得住：`None if report.rolled_back` 被写反（预检失败 → 「已回滚」，正是⑥要消灭的谎）；
//! `is_complete()` 被丢掉（回滚没做完也说成零残留）；`status != Failed` 也发阶段。

use std::path::PathBuf;

use unlucky_core::addon::import::{ImportError, ImportReport, ImportStatus, RollbackOutcome};

use super::*;

/// 一份「失败」的空报告（还没决定回滚结果）。
fn failed() -> ImportReport {
    let mut report = ImportReport::empty(PathBuf::from("C:/tmp/.minecraft"));
    report.fail(ImportError::integrity("sha1 mismatch"));
    report
}

/// 预检就失败（**从未写过目标目录**）⇒ `NotStarted`。
///
/// core 的 `abandoned()` 就是这一形：`rollback == None` + `rolled_back == true`。
#[test]
fn preflight_failure_is_not_started() {
    let mut report = failed();
    report.rolled_back = true;
    assert_eq!(
        failure_stage(&report),
        Some(ImportFailureStageView::NotStarted),
        "没写过目标目录就是「根本没开始」，不许说成「已回滚」"
    );
    assert_eq!(failure_stage(&report).unwrap().slug(), "not_started");
    // 渲染层同一份判据（`failed_report_with` 走的是同一条）。
    assert_eq!(
        failed_report_with(
            PathBuf::from("C:/tmp"),
            std::path::Path::new("Better MC.mrpack"),
            ImportErrorView {
                code: "pack_traversal".to_string(),
                label: "Path traversal".to_string(),
                message: "../etc/passwd".to_string(),
                hint: None,
            },
        )
        .failure_stage,
        Some(ImportFailureStageView::NotStarted),
        "宿主侧失败（目标实例不可用）也是「没开始写」，不是「已回滚」"
    );
}

/// 跑过回滚且完整 ⇒ `RolledBack`。
#[test]
fn a_complete_rollback_is_rolled_back() {
    let mut report = failed();
    report.rolled_back = true;
    report.rollback = Some(RollbackOutcome {
        attempted: 3,
        failed: Vec::new(),
    });
    assert_eq!(
        failure_stage(&report),
        Some(ImportFailureStageView::RolledBack)
    );
    assert_eq!(failure_stage(&report).unwrap().slug(), "rolled_back");
}

/// 跑过回滚但有失败项 ⇒ `RollbackIncomplete`（目标目录可能与导入前不同）。
#[test]
fn a_partial_rollback_is_incomplete() {
    let mut report = failed();
    report.rolled_back = false;
    report.rollback = Some(RollbackOutcome {
        attempted: 3,
        failed: vec!["mods/a.jar: 占用中".to_string()],
    });
    assert_eq!(
        failure_stage(&report),
        Some(ImportFailureStageView::RollbackIncomplete)
    );
    assert_eq!(
        failure_stage(&report).unwrap().slug(),
        "rollback_incomplete"
    );
}

/// 既没回滚也没宣称干净（core 的 `failed()` 单独一支）⇒ 只能如实说「可能不一致」。
#[test]
fn a_failure_without_any_claim_is_incomplete_not_not_started() {
    let mut report = failed();
    report.rolled_back = false;
    report.rollback = None;
    assert_eq!(
        failure_stage(&report),
        Some(ImportFailureStageView::RollbackIncomplete),
        "没跑回滚也不宣称干净：不许说「没开始写」"
    );
}

/// 不是致命失败 ⇒ 没有「失败阶段」可言（成功/部分成功都不该发这一段）。
#[test]
fn a_non_fatal_report_has_no_stage() {
    let mut report = ImportReport::empty(PathBuf::from("C:/tmp"));
    report.rolled_back = false;
    assert_eq!(failure_stage(&report), None, "Success 不该有失败阶段");
    report.status = ImportStatus::Partial;
    assert_eq!(failure_stage(&report), None, "Partial 不该有失败阶段");
}
