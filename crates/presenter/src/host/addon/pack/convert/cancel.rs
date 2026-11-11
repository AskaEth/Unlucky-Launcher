//! 「取消的**真实结局**」→ DTO 三档（M195 r3）。
//!
//! 从父模块拆出来只为行数（父文件已到 400 硬限附近），判据与文案口径原样。

use unlucky_core::addon::import::report::ImportStatus;
use unlucky_core::addon::import::ImportReport;

use crate::dto::ImportCancelView;

/// core 的「取消真实结局」→ DTO 的三档（M195 r3：**不许**把三档混成一句）。
///
/// - core 说 `Cancelled` ⇒ 管线真的停手了；回滚完整与否看 `rolled_back`（两档分开说）；
/// - core 说 `Success`/`Partial` 而这次被点过取消 ⇒ **取消太晚**：报告是真结局，但它不是
///   "停下来了"，界面必须照实说「实例会保留、文件可能已被覆盖」；
/// - core 说 `Failed` ⇒ **一律 `NotRequested`**（复审 r3 P2-1）：失败卡自己已经把真相说全了
///   （`failure_stage` 分「没写过 / 回滚干净 / 回滚没做完」），再贴一句「取消太晚、已经跑完、
///   文件可能被覆盖」就是与红卡**直接对立**的第二句假话——失败的导入没有"跑完"这一说。
pub(super) fn cancel_view(report: &ImportReport, cancel_requested: bool) -> ImportCancelView {
    match report.status {
        ImportStatus::Cancelled => {
            if report.rolled_back {
                ImportCancelView::RolledBack
            } else {
                ImportCancelView::RollbackIncomplete
            }
        }
        ImportStatus::Success | ImportStatus::Partial if cancel_requested => {
            ImportCancelView::TooLate
        }
        ImportStatus::Success | ImportStatus::Partial | ImportStatus::Failed => {
            ImportCancelView::NotRequested
        }
    }
}

#[cfg(test)]
mod tests {
    //! 三档的**推导**（不是"给定这一档、文案怎么画"）：什么输入会推出哪一档。
    //!
    //! 这一族用例是复审 r3 P2-1/P2-2 的直接产物：此前只有下半段（文案）有用例，把推导删掉
    //! 也全绿。

    use unlucky_core::addon::import::report::ImportStatus;
    use unlucky_core::addon::import::ImportReport;

    use crate::dto::ImportCancelView;

    use super::super::report_view;

    fn core_report(status: ImportStatus, rolled_back: bool) -> ImportReport {
        let mut report = ImportReport::empty(std::path::PathBuf::from("instance"));
        report.status = status;
        report.rolled_back = rolled_back;
        report
    }

    fn cancel_of(
        status: ImportStatus,
        rolled_back: bool,
        cancel_requested: bool,
    ) -> ImportCancelView {
        report_view(
            &core_report(status, rolled_back),
            std::path::Path::new("instance"),
            cancel_requested,
        )
        .cancel
    }

    #[test]
    fn an_incomplete_rollback_is_its_own_bucket() {
        // 推导：`Cancelled` + `rolled_back == false` ⇒ **回滚没做完**（不是"已回滚"）。
        // 把这条推导删掉（一律按已回滚）这里就红——那正是复审 r3 P2-2 说的零覆盖。
        assert_eq!(
            cancel_of(ImportStatus::Cancelled, false, true),
            ImportCancelView::RollbackIncomplete
        );
        // 对照：回滚完整才是「已回滚」——两档不许并成一个。
        assert_eq!(
            cancel_of(ImportStatus::Cancelled, true, true),
            ImportCancelView::RolledBack
        );
    }

    #[test]
    fn a_failed_run_is_never_dressed_up_as_a_late_cancel() {
        // P2-1：失败 + 点过取消（例如"点取消的同时摘要不符"）⇒ 失败卡自己说全了真相，
        // 不许再贴「取消太晚、已经跑完、文件可能被覆盖」——那两句直接对立。
        assert_eq!(
            cancel_of(ImportStatus::Failed, false, true),
            ImportCancelView::NotRequested,
            "失败的导入没有「跑完」这一说"
        );
        assert_eq!(
            cancel_of(ImportStatus::Failed, true, true),
            ImportCancelView::NotRequested
        );
        // 对照：成功/部分成功 + 点过取消，才是「太晚」。
        for status in [ImportStatus::Success, ImportStatus::Partial] {
            assert_eq!(
                cancel_of(status, false, true),
                ImportCancelView::TooLate,
                "{status:?} + 取消 ⇒ 太晚"
            );
        }
    }

    #[test]
    fn an_untouched_run_carries_no_cancel_verdict() {
        for status in [
            ImportStatus::Success,
            ImportStatus::Partial,
            ImportStatus::Failed,
        ] {
            assert_eq!(
                cancel_of(status, false, false),
                ImportCancelView::NotRequested,
                "没点过取消就不该有取消结论"
            );
        }
    }
}
