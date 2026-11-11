//! 导入结果的视图模型：**三态 + 容忍域逐条 + 致命块**。
//!
//! ## 为什么容忍域与致命域连字段类型都不同
//!
//! 任务书：「失败原因逐条展示（能容忍的缺文件与致命错误必须视觉可区分）」。
//! 只在颜色上区分不够——颜色一关掉（截图/高对比/色弱）就没了。所以：
//!
//! | 域 | 类型 | 画法 |
//! |---|---|---|
//! | 容忍（缺文件 / 被拒 component / 注记） | [`Row`]（路径 + 原因） | 中性容器 + `Info` 图标 + 逐条列表 |
//! | 致命（已回滚） | [`Fatal`]（大类 + 原文 + 建议 + 稳定码） | `ErrorContainer` + `Error` 图标 + 标题换措辞 |
//!
//! 一个 `Option<Fatal>` 与一个 `Vec<Row>` 之间没有「误合并」的写法，这比任何样式约定都硬。

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{
    ConflictChoiceView, ConflictFileView, ConflictReasonView, ImportCancelView, ImportErrorView,
    ImportFailureStageView, ImportReportView, ImportStatusView, MissingFileView, MissingReasonView,
};

use super::PackPageState;

/// 报告里的一行（容忍域：缺文件 / 被拒 component）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// 左列（路径 / 组件名）
    pub path: String,
    /// 右列（原因的人读文案）
    pub note: String,
}

/// 一条冲突（三方哈希 + 本次处置）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictRow {
    /// 实例内路径
    pub path: String,
    /// 成因（`你改过` / `你删过` / `本来就在`）
    pub reason: String,
    /// 本次实际处置（`保留我的` / `用包里的`）
    pub choice: String,
    /// 包内 SHA-1（截短）
    pub pack_sha1: String,
    /// 基线 SHA-1（`None` = 没有基线）
    pub base_sha1: Option<String>,
    /// 磁盘 SHA-1（`None` = 文件不在）
    pub disk_sha1: Option<String>,
    /// 展开中（展开才画三方哈希那行）
    pub expanded: bool,
}

/// 致命失败块（**两种措辞，不共用**）。
///
/// 为什么不是一句「失败」：`NotStarted`（预检就没过，一个字节都没写）与 `RolledBack`
/// （写进去了，又整包回滚干净）对用户是不同的消息——前者「去修包再试」，后者「刚才动过你的目录，
/// 已经复原」。M161 之前两者共用一句「已回滚 —— 目标目录零残留」，预检失败时那句话是假的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatalKind {
    /// 预检/装配阶段失败：目标目录**一个字节都没写**
    NotStarted,
    /// 写过目标目录，回滚**完整**完成
    RolledBack,
    /// 写过，但回滚有失败项（目标目录可能与导入前不同）
    RollbackIncomplete,
}

impl FatalKind {
    /// 图标（与 [`Fatal::title`] 各说一遍：颜色/图标关掉时标题仍然分得清）。
    #[must_use]
    pub const fn icon(self) -> unlucky_design::icons::IconKind {
        match self {
            // 预检没过 = 「还没开始」，用警示而不是「已出错」：目录是干净的
            Self::NotStarted => unlucky_design::icons::IconKind::Warn,
            Self::RolledBack => unlucky_design::icons::IconKind::Restore,
            Self::RollbackIncomplete => unlucky_design::icons::IconKind::Error,
        }
    }
}

/// 致命失败块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fatal {
    /// 三种阶段之一（决定标题/图标/底色）
    pub kind: FatalKind,
    /// 标题（预检未通过 / 已回滚 / 回滚不完整）
    pub title: String,
    /// 大类名（`路径穿越` / `清单损坏` …）
    pub label: String,
    /// 具体是什么
    pub message: String,
    /// 稳定码
    pub code: String,
    /// 补救建议
    pub hint: Option<String>,
}

/// 导入结果块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// 三态
    pub status: ImportStatusView,
    /// 标题（已导入 / 有缺口 / 已回滚）
    pub title: String,
    /// 副标题（`包名 · 格式 · 游戏版本 · 加载器`）
    pub subtitle: String,
    /// 写盘行（`12 个文件已写盘 · 3.4 MB`）
    pub written_line: String,
    /// 致命失败（有它就**没有**下面的缺口区块）
    pub fatal: Option<Fatal>,
    /// 缺文件标题
    pub missing_title: Option<String>,
    /// 缺文件逐条（容忍域）
    pub missing: Vec<Row>,
    /// 冲突标题
    pub conflicts_title: Option<String>,
    /// 冲突逐条
    pub conflicts: Vec<ConflictRow>,
    /// 被拒 component 的标题（带计数，与下面逐条行数一致）
    pub rejected_title: Option<String>,
    /// 被拒 component 逐条
    pub rejected: Vec<Row>,
    /// 注记逐条
    pub warnings: Vec<String>,
    /// 能不能给「以包为准重新导入」（有冲突、且实例可用时才有意义）
    pub can_overwrite: bool,
    /// 「两者都留改名」为何不支持（有冲突时如实写）
    pub overwrite_note: Option<String>,
    /// 这次运行与取消的关系（M195 r3：标题与图标按它分档，不许一句打天下）
    pub cancel: ImportCancelView,
    /// 取消的**补充说明**：只有「取消太晚」有——那种情况标题照常是"已导入/部分成功"，
    /// 必须另起一句说清"你的取消没赶上，实例会保留、同名文件可能已被覆盖"。
    pub cancel_note: Option<String>,
}

impl Report {
    /// 实例可用吗（`Success` / `Partial` 都是装上了）。
    pub const fn installed(&self) -> bool {
        self.status.installed()
    }
}

/// 一次导入结果 → 视图模型。
pub(super) fn report_of(
    report: &ImportReportView,
    page: &PackPageState,
    catalog: Catalog,
) -> Report {
    let missing_title = (!report.missing.is_empty()).then(|| {
        catalog.render(
            Key::PackReportMissing,
            &[("count", &report.missing.len().to_string())],
        )
    });
    let conflicts_title = (!report.conflicts.is_empty()).then(|| {
        catalog.render(
            Key::PackReportConflicts,
            &[("count", &report.conflicts.len().to_string())],
        )
    });

    Report {
        status: report.status,
        title: status_title(report, catalog).to_owned(),
        subtitle: subtitle(report, catalog),
        written_line: catalog.render(
            Key::PackReportWritten,
            &[
                ("count", &report.written().to_string()),
                ("bytes", &human(report)),
            ],
        ),
        fatal: report
            .error
            .as_ref()
            .map(|error| fatal_of(error, report.failure_stage, catalog)),
        missing_title,
        missing: report
            .missing
            .iter()
            .map(|file| missing_row(file, catalog))
            .collect(),
        conflicts_title,
        conflicts: report
            .conflicts
            .iter()
            .map(|file| conflict_row(file, page.expanded.as_deref(), catalog))
            .collect(),
        rejected_title: (!report.rejected.is_empty()).then(|| {
            catalog.render(
                Key::PackReportRejected,
                &[("count", &report.rejected.len().to_string())],
            )
        }),
        rejected: report
            .rejected
            .iter()
            .map(|component| Row {
                path: component.clone(),
                note: catalog.text(Key::PackReportRejectedNote).to_owned(),
            })
            .collect(),
        warnings: report.warnings.clone(),
        // 「以包为准重新导入」只在**导入到已有实例**时有意义（复审 r1 P2-2）：新建实例那条路上
        // 目标目录已经建好，再点一次必然撞「directory already exists」——一个必然失败的按钮
        // 等于死按钮，所以那种流程下不显示。
        can_overwrite: !report.conflicts.is_empty()
            && report.status == ImportStatusView::Partial
            && page.instance.is_some(),
        overwrite_note: (!report.conflicts.is_empty())
            .then(|| catalog.text(Key::PackOverwriteNote).to_owned()),
        cancel: report.cancel,
        // 「取消太晚」那句只说给**真的跑完**的导入（复审 r3 P2-1）：推导那边已经保证
        // `Failed` 不会落到 `TooLate`，这里再看一眼状态是双保险——万一推导回归，也不会让
        // 红卡（"零残留"）与"已经跑完、文件可能被覆盖"同时出现在一屏上。
        cancel_note: (report.cancel == ImportCancelView::TooLate
            && matches!(
                report.status,
                ImportStatusView::Success | ImportStatusView::Partial
            ))
        .then(|| catalog.text(Key::PackCancelTooLate).to_owned()),
    }
}

/// 字节数的人读串。
fn human(report: &ImportReportView) -> String {
    unlucky_presenter::dto::human_bytes(report.bytes_written)
}

/// 标题：三态 + **失败阶段**（预检未通过 ≠ 已回滚，M161）。
fn status_title(report: &ImportReportView, catalog: Catalog) -> &'static str {
    match report.status {
        ImportStatusView::Success => catalog.text(Key::PackStatusSuccess),
        ImportStatusView::Partial => catalog.text(Key::PackStatusPartial),
        // 三档取消各有各的话：已回滚（什么都没留下）/ 回滚没做完（要对一眼目录）/
        // 取消太晚（标题是真实状态，另有一句 cancel_note 说明"没赶上"）。
        ImportStatusView::Cancelled => match report.cancel {
            ImportCancelView::RollbackIncomplete => catalog.text(Key::PackCancelIncomplete),
            _ => catalog.text(Key::PackStatusCancelled),
        },
        ImportStatusView::Failed => match report.failure_stage {
            Some(ImportFailureStageView::NotStarted) => {
                catalog.text(Key::PackStatusFailedNotStarted)
            }
            Some(ImportFailureStageView::RollbackIncomplete) => {
                catalog.text(Key::PackStatusFailedIncomplete)
            }
            // `None` 不该出现（失败必有阶段）；退回「已回滚」这一句是旧行为，
            // 只有拿不到阶段时才会走到（例如未来的旧版对端）。
            _ => catalog.text(Key::PackStatusFailed),
        },
    }
}

/// 副标题：包名 · 格式 · 游戏版本 · 加载器（缺哪个就不写哪个，不拼「未知」）。
fn subtitle(report: &ImportReportView, catalog: Catalog) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(format) = report.format {
        parts.push(super::format_label(format, catalog).to_owned());
    }
    if let Some(version) = &report.game_version {
        parts.push(version.clone());
    }
    if let Some(loader) = &report.loader {
        parts.push(loader.clone());
    }
    if let Some(name) = &report.pack_name {
        parts.insert(0, name.clone());
    }
    parts.join(" · ")
}

fn fatal_of(
    error: &ImportErrorView,
    stage: Option<ImportFailureStageView>,
    catalog: Catalog,
) -> Fatal {
    let kind = match stage {
        Some(ImportFailureStageView::NotStarted) => FatalKind::NotStarted,
        Some(ImportFailureStageView::RollbackIncomplete) => FatalKind::RollbackIncomplete,
        _ => FatalKind::RolledBack,
    };
    let title = match kind {
        FatalKind::NotStarted => catalog.text(Key::PackFatalTitleNotStarted),
        FatalKind::RolledBack => catalog.text(Key::PackFatalTitle),
        FatalKind::RollbackIncomplete => catalog.text(Key::PackFatalTitleIncomplete),
    };
    Fatal {
        kind,
        title: title.to_owned(),
        label: error.label.clone(),
        message: error.message.clone(),
        code: error.code.clone(),
        hint: error.hint.clone(),
    }
}

fn missing_row(file: &MissingFileView, catalog: Catalog) -> Row {
    Row {
        path: file.path.clone().unwrap_or_else(|| file.display.clone()),
        note: match file.reason {
            MissingReasonView::Offline => catalog.text(Key::PackMissingOffline).to_owned(),
            MissingReasonView::NoSource => catalog.text(Key::PackMissingNoSource).to_owned(),
            MissingReasonView::FetchFailed => catalog.render(
                Key::PackMissingFetchFailed,
                &[("detail", file.detail.as_str())],
            ),
        },
    }
}

fn conflict_row(file: &ConflictFileView, expanded: Option<&str>, catalog: Catalog) -> ConflictRow {
    ConflictRow {
        path: file.path.clone(),
        reason: match file.reason {
            ConflictReasonView::UserModified => catalog.text(Key::PackConflictUserModified),
            ConflictReasonView::UserDeleted => catalog.text(Key::PackConflictUserDeleted),
            ConflictReasonView::PreExisting => catalog.text(Key::PackConflictPreExisting),
        }
        .to_owned(),
        choice: match file.choice {
            ConflictChoiceView::KeepUser => catalog.text(Key::PackChoiceKeepUser),
            ConflictChoiceView::Overwrite => catalog.text(Key::PackChoiceOverwrite),
        }
        .to_owned(),
        pack_sha1: short_sha(&file.pack_sha1),
        base_sha1: file.base_sha1.as_deref().map(short_sha),
        disk_sha1: file.disk_sha1.as_deref().map(short_sha),
        expanded: expanded == Some(file.path.as_str()),
    }
}

/// SHA-1 截短（前 8 位够人眼区分；全 40 位会把一行撑爆）。
#[must_use]
pub fn short_sha(sha: &str) -> String {
    sha.chars().take(8).collect()
}
