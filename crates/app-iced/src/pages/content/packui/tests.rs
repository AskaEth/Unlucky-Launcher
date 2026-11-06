//! 导入/导出区的守门（M113）：四种出图态都装得满、入口文案都在。
//!
//! 出图（D30）靠这几条夹具：任何一态装不满，截图就会是一块空容器，而不是「有话说」的界面。
//! 视图模型的判定在 `model/tests.rs`，这里只管**夹具本身**。

use unlucky_i18n::Lang;

use super::demo::pack_in;
use super::model::Pack;

/// D30 要求的四态（导入中 / 部分成功 / 冲突选择 / 导出预检）。
const STATES: [&str; 4] = ["importing", "partial", "conflict", "export"];

#[test]
fn every_screenshot_state_fills_something() {
    for state in STATES {
        let (model, _page) = pack_in(state, Lang::En);
        assert!(
            !model.is_idle(),
            "{state} 这一态什么都没画（截图会是一块空容器）"
        );
        assert_eq!(model.targets.len(), 2, "{state} 的实例候选不足");
        assert!(!model.drop_hint.is_empty());
        assert!(
            !model.no_dialog_note.is_empty(),
            "没有系统文件框这件事必须写在界面上"
        );
    }
}

#[test]
fn importing_shows_the_shared_install_panel() {
    let (model, _page) = pack_in("importing", Lang::En);
    let running = model.running.expect("导入中要有进度");
    // 进度形状来自下载页那块面板（`Download.install`），不是本区另造的
    assert!(
        running
            .panel
            .as_ref()
            .is_some_and(|panel| panel.install.is_some()),
        "进度必须走下载页的安装面板视图模型"
    );
    assert!(
        running.stage_line.contains("2/5"),
        "解包阶段是第 2/5 步：{}",
        running.stage_line
    );
    assert!(model.report.is_none(), "导入中不该同时有结果");
}

#[test]
fn partial_keeps_the_tolerable_rows_one_by_one() {
    let (model, _page) = pack_in("partial", Lang::En);
    let report = model.report.expect("部分成功要有报告");
    assert!(report.installed(), "Partial 仍是「装上了」");
    assert!(report.fatal.is_none(), "部分成功不是致命失败");
    assert_eq!(report.missing.len(), 3, "缺文件要逐条列出");
    assert!(report.missing_title.is_some());
    assert!(!report.rejected.is_empty(), "被拒 component 也要逐条");
}

#[test]
fn conflict_state_defaults_to_keeping_the_user_copy_and_expands_one_row() {
    let (model, page) = pack_in("conflict", Lang::En);
    let report = model.report.expect("冲突态要有报告");
    assert_eq!(report.conflicts.len(), 3);
    assert!(
        report.conflicts.iter().all(|row| !row.expanded)
            || report.conflicts.iter().filter(|row| row.expanded).count() == 1,
        "只展开用户点过的那一条"
    );
    assert_eq!(
        report.conflicts.iter().filter(|row| row.expanded).count(),
        1,
        "夹具要展开一条，截图才看得见三方哈希"
    );
    assert!(page.expanded.is_some());
    assert!(report.can_overwrite);
    assert!(report.overwrite_note.is_some());
}

#[test]
fn export_state_has_a_preview_but_has_not_written_anything() {
    let (model, _page) = pack_in("export", Lang::En);
    assert!(model.export.open);
    let preview = model.export.preview.expect("预检要有结果");
    assert!(
        preview.head.contains('2'),
        "内联数要进主结论：{}",
        preview.head
    );
    assert!(!preview.skipped.is_empty(), "无法导出的项要逐条");
    assert!(!preview.complete, "有跳过项 ⇒ 包不完整");
    assert!(!preview.notes.is_empty(), "已知损失要列出来");
    assert!(
        model.export.done.is_none(),
        "预检阶段不许出现「已写出」（那是写盘之后的事）"
    );
}

#[test]
fn default_state_offers_the_entry_with_nothing_else() {
    let (model, page): (Pack, _) = pack_in("", Lang::En);
    assert!(model.is_idle());
    assert!(!model.export.open);
    assert!(page.draft.is_empty());
}
