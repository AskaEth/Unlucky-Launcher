//! 导出：格式选择 + **预检**（不写盘）。
//!
//! ## 预检与写盘是两次调用，同一套判据
//!
//! [`Preview`] 只扫描分类、不写盘；确认之后才是真导出。任务书要的
//! 「将内联 N 个文件 / 无法导出 M 项」正是 [`Preview::head`]。
//!
//! 三分类的口径（与 core 一致，**没有第四类**）：
//!
//! | 类 | 含义 | 预检里怎么呈现 |
//! |---|---|---|
//! | `Remote` | 能证明可被远程替代 | 一行计数 |
//! | `Inline` | 证明不了 ⇒ 原字节进 `overrides/` | 计数 + 字节 + 前几条样例 |
//! | `Skipped` | 压根没进包 | **逐条列出**（包不完整，不许合并成一句「已导出」） |
//!
//! `Skipped` 非零时 [`Preview::complete`] 为假，界面上会明确写「有文件不会进包」——
//! 这是 M112 那句「不认识的 mod 不能丢、不能编」的用户侧落点：丢了就要看见丢在哪。

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{
    human_bytes, ExportClassView, ExportFileView, ExportPreviewView, ExportReasonView,
    ExportRequestView, ExportState, PackFormatView,
};

use super::{PackPageState, PackTarget, Row};

/// 页内选择 → 导出请求（预检与写盘**共用同一份**）。
///
/// `target` 必须从这里进去（复审 r1 P1-1）：之前全仓唯一构造点硬编码 `target: None`，
/// 于是用户敲的「Save to」永远不进请求 —— 输入框是死的。
#[must_use]
pub fn request_of(page: &PackPageState, instance: &str) -> ExportRequestView {
    let name = page.export_name.trim();
    let version = page.export_version.trim();
    let output = page.output.trim();
    ExportRequestView {
        instance: instance.to_string(),
        format: page.format,
        name: if name.is_empty() {
            instance.to_string()
        } else {
            name.to_string()
        },
        version: if version.is_empty() {
            "1.0.0".to_string()
        } else {
            version.to_string()
        },
        target: (!output.is_empty()).then(|| std::path::PathBuf::from(output)),
    }
}

/// 预检里「内联文件样例」最多列几条（其余用一句「还有 N 个」收口）。
const INLINE_SAMPLE: usize = 6;

/// 预检块（**还没写盘**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// 一句话主结论：`将内联 12 个文件 · 无法导出 0 项`
    pub head: String,
    /// 远程引用行
    pub remote_line: String,
    /// 内联字节行
    pub inline_line: String,
    /// 落盘路径行
    pub target_line: String,
    /// 包会完整吗（`false` = 有文件压根没进包）
    pub complete: bool,
    /// 完整性那句话
    pub complete_line: String,
    /// 无法导出的项（逐条）
    pub skipped: Vec<Row>,
    /// 内联文件样例
    pub inline_sample: Vec<String>,
    /// `还有 N 个文件…`
    pub more_line: Option<String>,
    /// core 的 `notes`（已知损失）
    pub notes: Vec<String>,
}

/// 导出块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    /// 面板开着吗
    pub open: bool,
    /// 当前格式
    pub format: PackFormatView,
    /// 格式人读名
    pub format_label: &'static str,
    /// 可选的格式（只有两个：mrpack / CF）
    pub formats: Vec<(PackFormatView, &'static str)>,
    /// 目标实例候选
    pub targets: Vec<PackTarget>,
    /// 选中的实例
    pub instance: Option<unlucky_presenter::dto::InstanceId>,
    /// 包名
    pub name: String,
    /// 版本
    pub version: String,
    /// 落盘路径草稿（空 = 缺省）
    pub output: String,
    /// MultiMC/Prism 只导入不导出（如实写）
    pub format_note: &'static str,
    /// 预检在飞
    pub pending: bool,
    /// 预检的进行中说明
    pub pending_line: &'static str,
    /// 预检结果
    pub preview: Option<Preview>,
    /// 写盘完成行
    pub done: Option<String>,
    /// 失败原因
    pub failed: Option<String>,
}

/// 导出一块。
pub(super) fn export_of(
    state: &ExportState,
    page: &PackPageState,
    targets: Vec<PackTarget>,
    catalog: Catalog,
) -> Export {
    let instance = page
        .instance
        .clone()
        .or_else(|| targets.first().map(|target| target.id.clone()));
    let formats = PackFormatView::EXPORTABLE
        .iter()
        .map(|format| (*format, format_label(*format, catalog)))
        .collect();

    Export {
        open: page.export_open,
        format: page.format,
        format_label: format_label(page.format, catalog),
        formats,
        targets,
        instance,
        name: page.export_name.clone(),
        version: page.export_version.clone(),
        output: page.output.clone(),
        format_note: catalog.text(Key::PackExportMultimcNote),
        pending: state.pending,
        pending_line: catalog.text(Key::PackExportPending),
        preview: state
            .preview
            .as_ref()
            .map(|preview| preview_of(preview, catalog)),
        done: state.done.as_ref().map(|done| {
            catalog.render(
                Key::PackExportDone,
                &[
                    ("path", &done.path.display().to_string()),
                    ("bytes", &human_bytes(done.bytes)),
                ],
            )
        }),
        failed: state.failed.clone(),
    }
}

pub(super) fn preview_of(preview: &ExportPreviewView, catalog: Catalog) -> Preview {
    let skipped: Vec<Row> = preview
        .files
        .iter()
        .filter(|file| file.class == ExportClassView::Skipped)
        .map(|file| skipped_row(file, catalog))
        .collect();
    let inline: Vec<&ExportFileView> = preview
        .files
        .iter()
        .filter(|file| file.class == ExportClassView::Inline)
        .collect();
    let inline_sample = inline
        .iter()
        .take(INLINE_SAMPLE)
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let more_line = (inline.len() > inline_sample.len()).then(|| {
        catalog.render(
            Key::PackPreviewMore,
            &[("count", &(inline.len() - inline_sample.len()).to_string())],
        )
    });

    Preview {
        head: catalog.render(
            Key::PackPreviewHead,
            &[
                ("inline", &preview.inline_count().to_string()),
                ("skipped", &preview.skipped_count().to_string()),
            ],
        ),
        remote_line: catalog.render(
            Key::PackPreviewRemote,
            &[("count", &preview.remote_count().to_string())],
        ),
        inline_line: catalog.render(
            Key::PackPreviewInlineBytes,
            &[("bytes", &human_bytes(preview.inline_bytes()))],
        ),
        target_line: catalog.render(
            Key::PackPreviewTarget,
            &[("path", &preview.target.display().to_string())],
        ),
        complete: preview.is_complete(),
        complete_line: catalog
            .text(if preview.is_complete() {
                Key::PackPreviewComplete
            } else {
                Key::PackPreviewIncomplete
            })
            .to_owned(),
        skipped,
        inline_sample,
        more_line,
        notes: preview.notes.clone(),
    }
}

fn skipped_row(file: &ExportFileView, catalog: Catalog) -> Row {
    Row {
        path: file.path.clone(),
        note: reason_label(file.reason, catalog).to_owned(),
    }
}

/// 导出归类原因 → 人读（每条都写得能自己解释，不靠图标猜）。
#[must_use]
pub fn reason_label(reason: ExportReasonView, catalog: Catalog) -> &'static str {
    match reason {
        ExportReasonView::ModrinthSha1 => catalog.text(Key::PackReasonModrinthSha1),
        ExportReasonView::CurseForgeFingerprint => catalog.text(Key::PackReasonCurseFingerprint),
        ExportReasonView::NoRemoteMatch => catalog.text(Key::PackReasonNoRemoteMatch),
        ExportReasonView::CurseOnlyMatch => catalog.text(Key::PackReasonCurseOnly),
        ExportReasonView::ModrinthOnlyMatch => catalog.text(Key::PackReasonModrinthOnly),
        ExportReasonView::MissingSha512 => catalog.text(Key::PackReasonMissingSha512),
        ExportReasonView::MissingCurseIds => catalog.text(Key::PackReasonMissingCurseIds),
        ExportReasonView::DisabledNotExpressible => catalog.text(Key::PackReasonDisabled),
        ExportReasonView::SymbolicLink => catalog.text(Key::PackReasonSymlink),
        ExportReasonView::UnsafePath => catalog.text(Key::PackReasonUnsafePath),
        ExportReasonView::Unreadable => catalog.text(Key::PackReasonUnreadable),
    }
}

/// 格式人读名。
#[must_use]
pub fn format_label(format: PackFormatView, catalog: Catalog) -> &'static str {
    match format {
        PackFormatView::Curseforge => catalog.text(Key::PackFormatCurseforge),
        PackFormatView::Modrinth => catalog.text(Key::PackFormatModrinth),
        PackFormatView::Multimc => catalog.text(Key::PackFormatMultimc),
    }
}
