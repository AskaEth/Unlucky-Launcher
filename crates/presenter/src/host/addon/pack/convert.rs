//! core 报告 → DTO（M113）：**逐字段照搬，不重算任何结论**。
//!
//! 这一层的唯一职责是翻译：三态、五阶段、四种处置、三种冲突成因、十一种导出原因，
//! 全部一一对应。任何一个「顺手换算一下」都会让 UI 说的和管线干的不是一回事，
//! 所以这里没有一处 re-derive（唯一例外是把 `&'static str` 的稳定码换成人读串）。
//!
//! 翻译用的是 core 的稳定 slug（`as_str()`）+ DTO 的 `from_slug()`，
//! 于是「翻译表」本身就是一份可审计的清单：core 加了新变体，`from_slug` 返回 `None`
//! 时这里会落到一个有名字的兜底，而不是悄悄画成别的意思。

mod cancel;

use std::path::{Path, PathBuf};

use unlucky_core::addon::export::{ExportReport, FileClass, FileReason};
use unlucky_core::addon::import::report::ImportStatus;
use unlucky_core::addon::import::{
    ConflictChoice, ConflictDecisions, ConflictFile, ConflictReason, FileAction, FileSource,
    ImportError, ImportProgress, ImportReport, ImportScan, LoaderTarget, MissingReason,
};

use crate::dto::{
    ConflictChoiceView, ConflictDecisionsView, ConflictFileView, ConflictReasonView,
    ConflictSourceView, ExportClassView, ExportDoneView, ExportReasonView, FileActionView,
    ImportConflictView, ImportErrorView, ImportFailureStageView, ImportProgressView,
    ImportReportView, ImportScanView, ImportStageView, ImportStatusView, ImportedFileView,
    MissingFileView, MissingReasonView, PackFormatView,
};

/// 一次导入**预检** → DTO（M160：冲突清单）。
pub(super) fn scan_view(scan: &ImportScan) -> ImportScanView {
    ImportScanView {
        target: scan.target.clone(),
        format: scan.format.as_deref().and_then(PackFormatView::from_slug),
        pack_name: scan.pack_name.clone(),
        pack_version: scan.pack_version.clone(),
        game_version: scan.game_version.clone(),
        conflicts: scan
            .conflicts
            .iter()
            .map(|conflict| ImportConflictView {
                path: conflict.path.clone(),
                source: source_view(conflict.source),
                reason: reason_of(conflict.reason),
                disabled: conflict.disabled,
                pack_sha1: conflict.pack_sha1.clone(),
                pack_size: conflict.pack_size,
                disk_sha1: conflict.disk_sha1.clone(),
                disk_size: conflict.disk_size,
                base_sha1: conflict.base_sha1.clone(),
            })
            .collect(),
        unknown_remote: scan.unknown_remote,
        lost_bytes: scan.lost_bytes(),
        incoming_bytes: scan.incoming_bytes(),
        notes: scan
            .notes
            .iter()
            .map(|issue| issue.message.clone())
            .collect(),
        error: scan.error.as_ref().map(error_view),
    }
}

/// DTO 的决策 → core 的决策表（**形状一换，语义不换**）。
///
/// 三条入口一一对应；没点名的路径在 core 侧落到"保留"（`ConflictDecisions` 的缺省档），
/// 所以"页面少传一条"永远不会变成"覆盖"。
pub(super) fn decisions_of(view: &ConflictDecisionsView) -> ConflictDecisions {
    let mut decisions = match view.default_choice() {
        ConflictChoiceView::KeepUser => ConflictDecisions::keep_all(),
        ConflictChoiceView::Overwrite => ConflictDecisions::overwrite_all(),
    };
    for item in view.items() {
        decisions.set(item.path.clone(), choice_of(item.choice));
    }
    decisions
}

const fn choice_of(choice: ConflictChoiceView) -> ConflictChoice {
    match choice {
        ConflictChoiceView::KeepUser => ConflictChoice::KeepUser,
        ConflictChoiceView::Overwrite => ConflictChoice::Overwrite,
    }
}

const fn source_view(source: FileSource) -> ConflictSourceView {
    match source {
        FileSource::Overrides => ConflictSourceView::Overrides,
        FileSource::ClientOverrides => ConflictSourceView::ClientOverrides,
        FileSource::CurseForge => ConflictSourceView::CurseForge,
        FileSource::Modrinth => ConflictSourceView::Modrinth,
    }
}

const fn reason_of(reason: ConflictReason) -> ConflictReasonView {
    match reason {
        ConflictReason::UserModified => ConflictReasonView::UserModified,
        ConflictReason::UserDeleted => ConflictReasonView::UserDeleted,
        ConflictReason::PreExisting => ConflictReasonView::PreExisting,
    }
}

/// 一次导入结果 → DTO。
///
/// `cancel_requested` = 这次运行**被点过取消**（presenter 手里的那个开关）。它只用来区分
/// "取消太晚"那一档：管线已经跑完时报告是 `Success`/`Partial`，但那不是一次正常导入——
/// 用户的取消没赶上，实例会保留、同名文件可能已被覆盖（M195 r3）。
pub(super) fn report_view(
    report: &ImportReport,
    target: &Path,
    cancel_requested: bool,
) -> ImportReportView {
    ImportReportView {
        status: ImportStatusView::from_slug(report.status.as_str())
            .unwrap_or(ImportStatusView::Failed),
        cancel: cancel::cancel_view(report, cancel_requested),
        format: report.format.as_deref().and_then(PackFormatView::from_slug),
        pack_name: report.pack_name.clone(),
        pack_version: report.pack_version.clone(),
        game_version: report.game_version.clone(),
        loader: report.loader.as_ref().map(loader_label),
        version_id: report.version_id.clone(),
        target: target.to_path_buf(),
        files: report
            .files
            .iter()
            .map(|file| ImportedFileView {
                path: file.path.clone(),
                action: action_view(file.action),
                bytes: file.bytes,
            })
            .collect(),
        conflicts: report.conflicts.iter().map(conflict_view).collect(),
        missing: report
            .missing
            .iter()
            .map(|file| MissingFileView {
                path: file.path.clone(),
                display: file.display.clone(),
                reason: missing_view(file.reason),
                detail: file.detail.clone(),
            })
            .collect(),
        rejected: report
            .rejected_components
            .iter()
            .map(|component| match &component.version {
                Some(version) => format!("{} {version}", component.uid),
                None => component.uid.clone(),
            })
            .collect(),
        warnings: report
            .warnings
            .iter()
            .map(|issue| issue.message.clone())
            .collect(),
        error: report.error.as_ref().map(error_view),
        registered: report
            .registered
            .as_ref()
            .map(|registered| registered.name.clone()),
        bytes_written: report.bytes_written,
        rolled_back: report.rolled_back,
        failure_stage: failure_stage(report),
    }
}

/// 致命失败发生在哪一步（M161）。
///
/// 判据只用 core 的两个机器位，**不看 warnings 的措辞**：
/// - `rollback: Some(outcome)` ⇒ 真的跑过回滚（写过了），完整与否看 `is_complete()`；
/// - `rollback: None` ⇒ 没跑过回滚。core 的约定是「失败发生在碰目标目录之前」（`abandoned`），
///   此时 `rolled_back == true`；若 `rolled_back` 也是 `false`（既没回滚也没宣称干净），
///   那就如实报「可能不一致」，不替它圆场。
fn failure_stage(report: &ImportReport) -> Option<ImportFailureStageView> {
    if report.status != ImportStatus::Failed {
        return None;
    }
    Some(match &report.rollback {
        Some(outcome) if outcome.is_complete() => ImportFailureStageView::RolledBack,
        Some(_) => ImportFailureStageView::RollbackIncomplete,
        None if report.rolled_back => ImportFailureStageView::NotStarted,
        None => ImportFailureStageView::RollbackIncomplete,
    })
}

/// 一次导出结果 → DTO。
pub(super) fn done_view(report: &ExportReport) -> ExportDoneView {
    ExportDoneView {
        path: report.path.clone(),
        bytes: report.bytes,
        remote: u32::try_from(report.remote_count()).unwrap_or(u32::MAX),
        inline: u32::try_from(report.inline_count()).unwrap_or(u32::MAX),
        skipped: u32::try_from(report.skipped_count()).unwrap_or(u32::MAX),
    }
}

/// 一条导入进度 → DTO。
pub(super) fn progress_view(progress: &ImportProgress, pack: &str) -> ImportProgressView {
    ImportProgressView {
        stage: ImportStageView::from_slug(progress.stage.as_str())
            .unwrap_or(ImportStageView::Probe),
        done: u32::try_from(progress.done).unwrap_or(u32::MAX),
        total: u32::try_from(progress.total).unwrap_or(u32::MAX),
        current: progress.current.clone(),
        pack: pack.to_string(),
    }
}

/// 致命失败 → DTO（稳定码原样带走，日志与 `--json` 用的是同一串）。
fn error_view(error: &ImportError) -> ImportErrorView {
    ImportErrorView {
        code: error.kind().as_str().to_string(),
        label: error.kind().label().to_string(),
        message: error.message().to_string(),
        hint: error.hint().map(str::to_string),
    }
}

fn conflict_view(file: &ConflictFile) -> ConflictFileView {
    ConflictFileView {
        path: file.path.clone(),
        reason: reason_of(file.reason),
        choice: choice_view(file.choice),
        pack_sha1: file.pack_sha1.clone(),
        base_sha1: file.base_sha1.clone(),
        disk_sha1: file.disk_sha1.clone(),
        pack_size: file.pack_size,
    }
}

/// 一条 `ConflictChoice` → DTO（预检与事后账共用一张翻译表）。
const fn choice_view(choice: ConflictChoice) -> ConflictChoiceView {
    match choice {
        ConflictChoice::KeepUser => ConflictChoiceView::KeepUser,
        ConflictChoice::Overwrite => ConflictChoiceView::Overwrite,
    }
}

const fn action_view(action: FileAction) -> FileActionView {
    match action {
        FileAction::Installed => FileActionView::Installed,
        FileAction::Replaced => FileActionView::Replaced,
        FileAction::SkippedIdentical => FileActionView::SkippedIdentical,
        FileAction::KeptUser => FileActionView::KeptUser,
    }
}

const fn missing_view(reason: MissingReason) -> MissingReasonView {
    match reason {
        MissingReason::Offline => MissingReasonView::Offline,
        MissingReason::NoSource => MissingReasonView::NoSource,
        MissingReason::FetchFailed => MissingReasonView::FetchFailed,
    }
}

pub(super) fn reason_view(reason: FileReason) -> ExportReasonView {
    ExportReasonView::from_slug(reason.as_str()).unwrap_or(ExportReasonView::Unreadable)
}

pub(super) fn class_view(class: FileClass) -> ExportClassView {
    match class {
        FileClass::Remote => ExportClassView::Remote,
        FileClass::Inline => ExportClassView::Inline,
        FileClass::Skipped => ExportClassView::Skipped,
    }
}

/// 加载器 → 人读串（`fabric-loader 0.15.11` / `vanilla 1.20.1`）。
fn loader_label(target: &LoaderTarget) -> String {
    format!("{} {}", target.kind.as_str(), target.loader_version)
}

/// 「命令跑起来了，但目标实例不可用」这类**host 侧**失败的报告。
///
/// 仍然走三态里的 `Failed`：界面只有一种失败形状，用户不用学第二套。
/// `target` 是**目标实例目录**（不知道就给空路径），不是包路径（复审 r1 P2-9 顺手纠的 DTO 语义）。
pub(super) fn failed_report(
    target: PathBuf,
    pack: &Path,
    code: &str,
    label: &str,
    message: &str,
) -> ImportReportView {
    failed_report_with(
        target,
        pack,
        ImportErrorView {
            code: code.to_string(),
            label: label.to_string(),
            message: message.to_string(),
            hint: None,
        },
    )
}

/// 一条 core 的 [`ImportError`] → 失败报告（**逐字段照搬**，含 `hint`）。
///
/// 预检失败（`scan.error`）走它：那些错本来就是 core 判出来的（`pack_traversal` /
/// `pack_limits` / `pack_metadata` …），没必要在宿主层再解释一遍，更不能把 `hint`
/// （"先装好游戏再导出"这种可执行下一步）丢掉——丢掉的话用户看到的就是"失败了，然后呢"。
pub(super) fn failed_report_of(
    target: PathBuf,
    pack: &Path,
    error: &ImportError,
) -> ImportReportView {
    failed_report_with(target, pack, error_view(error))
}

/// 组装：形状只此一处，文案由调用方给。
fn failed_report_with(target: PathBuf, pack: &Path, error: ImportErrorView) -> ImportReportView {
    ImportReportView {
        status: ImportStatusView::Failed,
        format: None,
        pack_name: Some(pack_stem(pack)),
        pack_version: None,
        game_version: None,
        loader: None,
        version_id: None,
        target,
        files: Vec::new(),
        conflicts: Vec::new(),
        missing: Vec::new(),
        rejected: Vec::new(),
        warnings: Vec::new(),
        error: Some(error),
        registered: None,
        bytes_written: 0,
        rolled_back: true,
        // 预检/装配阶段就失败了：目标目录一个字节都没写（不是「写完又回滚」）。
        failure_stage: Some(ImportFailureStageView::NotStarted),
        // 这一路根本没进管线，谈不上取消。
        cancel: crate::dto::ImportCancelView::NotRequested,
    }
}

/// 包文件名（`Better MC.mrpack` → `Better MC`）。
///
/// **M216 ②**：函数体一行委托 core（`game::install::pack_dir`）——CLI 调的是同一个函数，
/// 于是「CLI 缺省落点」与「GUI 落点」不只目录相同，连目录名归一化也只有一份实现。
pub(super) fn pack_stem(pack: &Path) -> String {
    unlucky_core::game::install::pack_dir::pack_stem(pack)
}

/// **实例目录名**（判据见 core 的 [`pack_dir::instance_dir_name`]）：本函数只有一行委托。
///
/// **只剩 presenter 自己的用例在调它**（生产侧落点已改成 `pack_dir::instance_dir`，见 M216
/// P3-2），所以挂 `cfg(test)`：它是那组既有判据（空格折 `-`、中文补 `-<hash8>`、两个中文名
/// 不塌成同一个目录）继续钉住归一化规则的入口，生产构建里不需要它。
///
/// [`pack_dir::instance_dir_name`]: unlucky_core::game::install::pack_dir::instance_dir_name
#[cfg(test)]
pub(super) fn dir_name(name: &str) -> String {
    unlucky_core::game::install::pack_dir::instance_dir_name(name)
}

/// 目录/文件名用的 slug（导出用的文件名也走它；实现同样收在 core `pack_dir`）。
pub(super) fn slug(text: &str) -> String {
    unlucky_core::game::install::pack_dir::slug(text)
}

#[cfg(test)]
#[path = "convert/stage_tests.rs"]
mod stage_tests;
