//! 导入/导出区视图模型的守门（M113）：阶段映射、预检计数、三方哈希、容忍/致命分野。
//!
//! 这里**不测画法**（画法靠出图），测的是「决策」：哪些数字进了哪一行、哪些字段被截短、
//! 哪一态该出现哪一块。所有断言都能在没有图形环境的情况下跑。

mod cancel;
mod pick;
mod stage;

use std::path::PathBuf;

use unlucky_i18n::Lang;
use unlucky_presenter::dto::{
    ConflictChoiceView, ConflictFileView, ConflictReasonView, ExportClassView, ExportFileView,
    ExportPreviewView, ImportCancelView, ImportErrorView, ImportProgressView, ImportReportView,
    ImportStageView, ImportStatusView, InstallStage, MissingFileView, MissingReasonView,
    PackFormatView,
};

use super::progress::{panel_progress, panel_stage, stage_line};
use super::report::report_of;
use super::report::short_sha;
use super::{Pack, PackPageState};

fn catalog() -> unlucky_i18n::Catalog {
    Lang::En.catalog()
}

#[test]
fn stage_line_counts_the_five_steps_in_order() {
    let expected = [
        (ImportStageView::Probe, "1/5"),
        (ImportStageView::Extract, "2/5"),
        (ImportStageView::Fetch, "3/5"),
        (ImportStageView::Commit, "4/5"),
        (ImportStageView::Register, "5/5"),
    ];
    for (stage, position) in expected {
        let line = stage_line(stage, catalog());
        assert!(line.contains(position), "{stage:?} 的步号不对：{line}");
    }
    // 阶段名要真的是「那一档」，不能五个阶段都显示同一句
    let lines: Vec<String> = ImportStageView::ALL
        .iter()
        .map(|stage| stage_line(*stage, catalog()))
        .collect();
    for (index, line) in lines.iter().enumerate() {
        for other in lines.iter().skip(index + 1) {
            assert_ne!(line, other, "两个阶段的说明撞了：{line}");
        }
    }
}

#[test]
fn no_progress_stage_maps_to_a_terminal_install_stage() {
    // 进度面板只该画「在跑」的形态：映射到 Done/Failed/Cancelled 会让面板提前画成收尾/失败
    for stage in ImportStageView::ALL {
        let mapped = panel_stage(stage);
        assert!(
            !mapped.is_terminal(),
            "{stage:?} 落到了终态 {mapped:?}（面板会画成收尾）"
        );
    }
}

#[test]
fn panel_progress_carries_the_counts_and_the_pack_name() {
    let progress = ImportProgressView {
        stage: ImportStageView::Fetch,
        done: 41,
        total: 90,
        current: Some("mods/sodium.jar".to_string()),
        pack: "Better MC".to_string(),
    };
    let panel = panel_progress(&progress);
    assert_eq!(panel.files_done, 41);
    assert_eq!(panel.files_total, 90);
    assert_eq!(panel.stage, InstallStage::Downloading);
    // 面板第一行显示的就是包名（导入的对象是包，不是某个版本 id）
    assert_eq!(panel.version, "Better MC");
}

#[test]
fn unknown_counts_keep_the_bar_unknown() {
    // total == 0 时不允许画确定态进度条（那是「静止的假进度」的来源）
    let progress = ImportProgressView {
        stage: ImportStageView::Probe,
        done: 0,
        total: 0,
        current: None,
        pack: "Pack".to_string(),
    };
    let panel = panel_progress(&progress);
    assert_eq!(panel.files_total, 0);
    assert_eq!(panel.speed_bps, 0, "导入没有字节速率，不许编");
}

#[test]
fn sha_is_shortened_to_eight_chars() {
    assert_eq!(
        short_sha("9f2c41ab77de3350aa1b8c0d5e6f708192a3b4c5"),
        "9f2c41ab"
    );
    assert_eq!(short_sha("abc"), "abc");
    assert_eq!(short_sha(""), "");
}

fn preview(files: Vec<ExportFileView>) -> ExportPreviewView {
    ExportPreviewView {
        format: PackFormatView::Modrinth,
        instance: "survival".to_string(),
        name: "Survival".to_string(),
        version: "1.0.0".to_string(),
        target: PathBuf::from("C:/tmp/out.mrpack"),
        files,
        notes: Vec::new(),
    }
}

fn export_file(
    path: &str,
    class: ExportClassView,
    reason: unlucky_presenter::dto::ExportReasonView,
) -> ExportFileView {
    ExportFileView {
        path: path.to_string(),
        class,
        reason,
        bytes: 1000,
        disabled: false,
    }
}

#[test]
fn preview_counts_match_the_three_classes() {
    use unlucky_presenter::dto::ExportReasonView;
    let view = preview(vec![
        export_file(
            "a.jar",
            ExportClassView::Remote,
            ExportReasonView::ModrinthSha1,
        ),
        export_file(
            "b.jar",
            ExportClassView::Inline,
            ExportReasonView::NoRemoteMatch,
        ),
        export_file(
            "c.jar",
            ExportClassView::Inline,
            ExportReasonView::MissingSha512,
        ),
        export_file(
            "d.jar",
            ExportClassView::Skipped,
            ExportReasonView::SymbolicLink,
        ),
    ]);
    assert_eq!(view.remote_count(), 1);
    assert_eq!(view.inline_count(), 2);
    assert_eq!(view.skipped_count(), 1);
    assert_eq!(view.inline_bytes(), 2000);
    assert!(!view.is_complete(), "有跳过项 ⇒ 包不完整");
}

#[test]
fn the_head_line_carries_the_two_numbers_the_task_asks_for() {
    use unlucky_presenter::dto::ExportReasonView;
    let view = preview(vec![
        export_file(
            "b.jar",
            ExportClassView::Inline,
            ExportReasonView::NoRemoteMatch,
        ),
        export_file(
            "c.jar",
            ExportClassView::Inline,
            ExportReasonView::MissingSha512,
        ),
        export_file(
            "d.jar",
            ExportClassView::Skipped,
            ExportReasonView::UnsafePath,
        ),
    ]);
    let block = super::export::preview_of(&view, catalog());
    // 「将内联 N 个文件 · 无法导出 M 项」两个数字必须来自同一份 files，不许另算
    assert!(
        block.head.contains('2'),
        "内联数没进 headline：{}",
        block.head
    );
    assert!(
        block.head.contains('1'),
        "跳过数没进 headline：{}",
        block.head
    );
    assert_eq!(block.skipped.len(), 1);
    assert!(!block.complete);
}

#[test]
fn report_keeps_tolerable_and_fatal_apart() {
    let report = ImportReportView {
        status: ImportStatusView::Failed,
        format: Some(PackFormatView::Curseforge),
        pack_name: Some("Broken".to_string()),
        pack_version: None,
        game_version: None,
        loader: None,
        version_id: None,
        target: PathBuf::from("C:/tmp"),
        files: Vec::new(),
        conflicts: Vec::new(),
        missing: vec![MissingFileView {
            path: Some("mods/a.jar".to_string()),
            display: "mods/a.jar".to_string(),
            reason: MissingReasonView::Offline,
            detail: String::new(),
        }],
        rejected: Vec::new(),
        warnings: Vec::new(),
        error: Some(ImportErrorView {
            code: "pack_traversal".to_string(),
            label: "路径穿越".to_string(),
            message: "../etc/passwd".to_string(),
            hint: Some("换一个来源".to_string()),
        }),
        registered: None,
        bytes_written: 0,
        rolled_back: true,
        failure_stage: None,
        cancel: ImportCancelView::NotRequested,
    };
    let block = report_of(&report, &PackPageState::default(), catalog());
    assert!(block.fatal.is_some(), "致命块必须单独存在");
    assert_eq!(block.missing.len(), 1, "容忍域仍要逐条列出");
    assert!(!block.installed(), "Failed 不是「装上了」");
    assert!(!block.can_overwrite, "没有冲突时不给翻档动作");
}

#[test]
fn conflicts_default_to_keeping_the_user_copy() {
    let report = ImportReportView {
        status: ImportStatusView::Partial,
        format: Some(PackFormatView::Modrinth),
        pack_name: Some("Pack".to_string()),
        pack_version: None,
        game_version: None,
        loader: None,
        version_id: None,
        target: PathBuf::from("C:/tmp"),
        files: Vec::new(),
        conflicts: vec![ConflictFileView {
            path: "config/x.json".to_string(),
            reason: ConflictReasonView::UserModified,
            choice: ConflictChoiceView::KeepUser,
            pack_sha1: "fixture".to_string(),
            base_sha1: None,
            disk_sha1: Some("disk".to_string()),
            pack_size: 12,
        }],
        missing: Vec::new(),
        rejected: Vec::new(),
        warnings: Vec::new(),
        error: None,
        registered: None,
        bytes_written: 0,
        rolled_back: true,
        failure_stage: None,
        cancel: ImportCancelView::NotRequested,
    };
    // 「以包为准」只在导入到**已有实例**时给（复审 r1 P2-2），所以这里选一个实例
    let page = PackPageState {
        instance: Some("survival".to_string()),
        ..PackPageState::default()
    };
    let block = report_of(&report, &page, catalog());
    assert_eq!(block.conflicts.len(), 1);
    assert!(!block.conflicts[0].expanded, "默认收起");
    assert_eq!(
        block.conflicts[0].choice,
        catalog().text(unlucky_i18n::Key::PackChoiceKeepUser),
        "默认档必须是「保留我的」"
    );
    assert!(block.can_overwrite, "有冲突要能整体翻档");
    assert!(block.overwrite_note.is_some(), "不支持「两者都留」要如实写");

    // 展开的那一条才画三方哈希
    let expanded = PackPageState {
        expanded: Some("config/x.json".to_string()),
        ..page.clone()
    };
    let block = report_of(&report, &expanded, catalog());
    assert!(block.conflicts[0].expanded);
}

#[test]
fn an_idle_pack_has_nothing_to_draw_but_the_entry() {
    let import = unlucky_presenter::dto::ImportState::default();
    let export = unlucky_presenter::dto::ExportState::default();
    let model = Pack::of(&import, &export, &PackPageState::default(), &[], catalog());
    assert!(model.is_idle());
    assert!(model.running.is_none());
    assert!(model.report.is_none());
    // 入口说明永远在（用户得知道可以拖什么、为什么没有「选择文件」按钮）
    assert!(!model.drop_hint.is_empty());
    assert!(!model.no_dialog_note.is_empty());
}

#[test]
fn the_save_to_field_really_reaches_the_request() {
    // 复审 r1 P1-1：这个字段以前是死的（构造点硬编码 target: None）
    let page = PackPageState {
        output: "  D:/packs/my.mrpack  ".to_string(),
        ..PackPageState::default()
    };
    let request = super::export::request_of(&page, "survival");
    assert_eq!(
        request.target.as_deref(),
        Some(std::path::Path::new("D:/packs/my.mrpack")),
        "用户敲的路径必须真的进请求（去首尾空白）"
    );

    // 留空 ⇒ None（由 host 给缺省路径），不是空串路径
    let blank = PackPageState {
        output: "   ".to_string(),
        ..PackPageState::default()
    };
    assert!(super::export::request_of(&blank, "survival")
        .target
        .is_none());

    // 包名/版本留空时的缺省值也钉一下（预检与写盘共用同一份请求，不能各算一套）
    let request = super::export::request_of(&blank, "survival");
    assert_eq!(request.name, "survival");
    assert_eq!(request.version, "1.0.0");
}

#[test]
fn overwrite_is_offered_only_when_importing_into_an_existing_instance() {
    // 复审 r1 P2-2：新建实例那条路上再点「以包为准」必然撞「directory already exists」
    let report = unlucky_presenter::dto::ImportReportView {
        status: ImportStatusView::Partial,
        format: Some(PackFormatView::Modrinth),
        pack_name: Some("Pack".to_string()),
        pack_version: None,
        game_version: None,
        loader: None,
        version_id: None,
        target: std::path::PathBuf::from("C:/tmp"),
        files: Vec::new(),
        conflicts: vec![ConflictFileView {
            path: "config/x.json".to_string(),
            reason: ConflictReasonView::UserModified,
            choice: ConflictChoiceView::KeepUser,
            pack_sha1: "abc".to_string(),
            base_sha1: None,
            disk_sha1: None,
            pack_size: 1,
        }],
        missing: Vec::new(),
        rejected: Vec::new(),
        warnings: Vec::new(),
        error: None,
        registered: None,
        bytes_written: 0,
        rolled_back: true,
        failure_stage: None,
        cancel: ImportCancelView::NotRequested,
    };

    let new_instance = PackPageState::default();
    assert!(
        !super::report::report_of(&report, &new_instance, catalog()).can_overwrite,
        "新建实例流程下不该出现「以包为准」（那按钮必然失败）"
    );

    let existing = PackPageState {
        instance: Some("survival".to_string()),
        ..PackPageState::default()
    };
    assert!(
        super::report::report_of(&report, &existing, catalog()).can_overwrite,
        "导入到已有实例时才给翻档动作"
    );
}
