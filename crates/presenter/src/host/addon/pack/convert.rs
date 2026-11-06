//! core 报告 → DTO（M113）：**逐字段照搬，不重算任何结论**。
//!
//! 这一层的唯一职责是翻译：三态、五阶段、四种处置、三种冲突成因、十一种导出原因，
//! 全部一一对应。任何一个「顺手换算一下」都会让 UI 说的和管线干的不是一回事，
//! 所以这里没有一处 re-derive（唯一例外是把 `&'static str` 的稳定码换成人读串）。
//!
//! 翻译用的是 core 的稳定 slug（`as_str()`）+ DTO 的 `from_slug()`，
//! 于是「翻译表」本身就是一份可审计的清单：core 加了新变体，`from_slug` 返回 `None`
//! 时这里会落到一个有名字的兜底，而不是悄悄画成别的意思。

use std::path::{Path, PathBuf};

use unlucky_core::addon::export::{ExportReport, FileClass, FileReason};
use unlucky_core::addon::import::{
    ConflictChoice, ConflictFile, ConflictReason, FileAction, ImportError, ImportProgress,
    ImportReport, LoaderTarget, MissingReason,
};

use crate::dto::{
    ConflictChoiceView, ConflictFileView, ConflictReasonView, ExportClassView, ExportDoneView,
    ExportReasonView, FileActionView, ImportErrorView, ImportProgressView, ImportReportView,
    ImportStageView, ImportStatusView, ImportedFileView, MissingFileView, MissingReasonView,
    PackFormatView,
};

/// 一次导入结果 → DTO。
pub(super) fn report_view(report: &ImportReport, target: &Path) -> ImportReportView {
    ImportReportView {
        status: ImportStatusView::from_slug(report.status.as_str())
            .unwrap_or(ImportStatusView::Failed),
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
    }
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
        reason: match file.reason {
            ConflictReason::UserModified => ConflictReasonView::UserModified,
            ConflictReason::UserDeleted => ConflictReasonView::UserDeleted,
            ConflictReason::PreExisting => ConflictReasonView::PreExisting,
        },
        choice: match file.choice {
            ConflictChoice::KeepUser => ConflictChoiceView::KeepUser,
            ConflictChoice::Overwrite => ConflictChoiceView::Overwrite,
        },
        pack_sha1: file.pack_sha1.clone(),
        base_sha1: file.base_sha1.clone(),
        disk_sha1: file.disk_sha1.clone(),
        pack_size: file.pack_size,
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
        error: Some(ImportErrorView {
            code: code.to_string(),
            label: label.to_string(),
            message: message.to_string(),
            hint: None,
        }),
        registered: None,
        bytes_written: 0,
        rolled_back: true,
    }
}

/// 包文件名（`Better MC.mrpack` → `Better MC`）。
pub(super) fn pack_stem(pack: &Path) -> String {
    pack.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "imported-pack".to_string())
}

/// **实例目录名**：ASCII 化 + 必要时补一段内容哈希。
///
/// 判据（复审 r2 P2-2 之后与注释对齐）：名字里**只含** ASCII 字母数字、空格、`-`、`_` 时
/// 保持原样（空格折成 `-`，`Better MC` → `Better-MC`）；**一旦折过别的字符**（中文、`.`、
/// 括号……）就补 `-<hash8>`，用原名的哈希保住区分度 —— 否则「我的整合包」「另一个整合包」
/// 都会得到 `pack`，第二个中文名导入直接撞「directory already exists」。
pub(super) fn dir_name(name: &str) -> String {
    let base = slug(name);
    let faithful = !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == ' ');
    if faithful {
        base
    } else {
        format!("{base}-{}", hash8(name))
    }
}

/// FNV-1a 64 → 8 位十六进制（只要稳定、短、不需要密码学强度）。
fn hash8(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:08x}", hash as u32)
}

/// 目录/文件名用的 slug（只留 ASCII 字母数字与 `-` / `_`，其余折成 `-`）。
pub(super) fn slug(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "pack".to_string()
    } else {
        trimmed
    }
}
