//! 取消的**三档真实结局**在界面上的分支（M195 r3）。
//!
//! 口径：一句「已停手并回滚」打天下是假话——取消可能停在提交前（已回滚）、可能停在回滚没收尾
//! （目标目录可能变了）、也可能**太晚**（管线已经跑完，实例保留、同名文件可能已被覆盖）。
//! 这一族用例钉的就是"三档各说各的话"，以及"占位文案不再替结局下结论"。

use std::path::PathBuf;

use unlucky_i18n::{Catalog, Key, Lang};
use unlucky_presenter::dto::{
    ImportCancelView, ImportReportView, ImportStatusView, PackFormatView,
};

use super::super::report::report_of;
use super::super::PackPageState;

fn catalog() -> Catalog {
    Lang::En.catalog()
}

fn report(
    status: ImportStatusView,
    cancel: ImportCancelView,
    rolled_back: bool,
) -> ImportReportView {
    ImportReportView {
        status,
        format: Some(PackFormatView::Multimc),
        pack_name: Some("Better MC".to_string()),
        pack_version: None,
        game_version: Some("1.20.1".to_string()),
        loader: None,
        version_id: Some("1.20.1".to_string()),
        target: PathBuf::from("C:/tmp/instance"),
        files: Vec::new(),
        conflicts: Vec::new(),
        missing: Vec::new(),
        rejected: Vec::new(),
        warnings: Vec::new(),
        error: None,
        registered: None,
        bytes_written: 0,
        rolled_back,
        failure_stage: None,
        cancel,
    }
}

#[test]
fn a_rolled_back_cancel_says_it_is_cancelled_not_partially_installed() {
    // 取消停在提交前：已回滚 ⇒ 标题是「已取消 —— 已回滚」，**不是**「部分成功」（那等于说装了一半），
    // 也不能出现「取消太晚」那句补充说明。
    let view = report(
        ImportStatusView::Cancelled,
        ImportCancelView::RolledBack,
        true,
    );
    let block = report_of(&view, &PackPageState::default(), catalog());

    assert_eq!(block.title, catalog().text(Key::PackStatusCancelled));
    assert_ne!(
        block.title,
        catalog().text(Key::PackStatusPartial),
        "已回滚的取消不许显示成「部分成功」"
    );
    assert!(!block.installed(), "取消不是「装上了」");
    assert!(block.cancel_note.is_none(), "已回滚不需要「太晚」那句");
}

#[test]
fn an_incomplete_cancel_rollback_says_the_directory_may_differ() {
    let view = report(
        ImportStatusView::Cancelled,
        ImportCancelView::RollbackIncomplete,
        false,
    );
    let block = report_of(&view, &PackPageState::default(), catalog());

    assert_eq!(block.title, catalog().text(Key::PackCancelIncomplete));
    assert_ne!(
        block.title,
        catalog().text(Key::PackStatusCancelled),
        "回滚没做完 ≠ 已回滚（这两句话不能混）"
    );
}

#[test]
fn a_too_late_cancel_says_the_import_already_finished() {
    // 取消来得太晚：状态是真实的 `Success`，另加一句「实例会保留、同名文件可能已被覆盖」。
    let view = report(ImportStatusView::Success, ImportCancelView::TooLate, false);
    let block = report_of(&view, &PackPageState::default(), catalog());

    assert_eq!(block.title, catalog().text(Key::PackStatusSuccess));
    assert_eq!(block.status, ImportStatusView::Success);
    assert_ne!(
        block.status,
        ImportStatusView::Cancelled,
        "太晚 ≠ 已回滚（状态是真的 Success/Partial）"
    );
    assert_eq!(
        block.cancel_note,
        Some(catalog().text(Key::PackCancelTooLate).to_owned()),
        "必须另外说清「取消没赶上」"
    );
}

/// 三档的标题**互不相同**（把任意两档并成一句，这里就红）。
#[test]
fn the_three_cancel_outcomes_have_three_different_titles() {
    let titles = [
        report_of(
            &report(
                ImportStatusView::Cancelled,
                ImportCancelView::RolledBack,
                true,
            ),
            &PackPageState::default(),
            catalog(),
        )
        .title,
        report_of(
            &report(
                ImportStatusView::Cancelled,
                ImportCancelView::RollbackIncomplete,
                false,
            ),
            &PackPageState::default(),
            catalog(),
        )
        .title,
        report_of(
            &report(ImportStatusView::Success, ImportCancelView::TooLate, false),
            &PackPageState::default(),
            catalog(),
        )
        .title,
    ];
    let unique: std::collections::BTreeSet<&String> = titles.iter().collect();
    assert_eq!(unique.len(), 3, "三档必须各说各的话：{titles:?}");
}
