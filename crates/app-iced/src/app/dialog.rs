//! 模态对话框（M187 接通 `DialogQueue`）+ 启动失败卡片。
//!
//! ## 为什么先补这一层
//!
//! 接线审计（M177）的结论很硬：`DialogQueue` / `DialogRequest` 在 app-iced 里**零引用**，
//! 所有弹窗都画不出来。于是「像 PCL2 / HMCL 那样，启动失败时弹一个带崩溃说明的框」这件事
//! 在此之前根本无处落脚。本模块是最小可用的一层：
//!
//! ```text
//! presenter AppState::dialogs（唯一队列）
//!         │ current()
//!         ▼
//! dialog::current(lang, request, card)  ← 纯视图模型（测试打这里）
//!         │
//!         ▼
//! dialog::view::layer(ui, base, modal)  ← 模态遮罩 + 卡片 + 主/次按钮
//! ```
//!
//! 交互：主按钮 / 次按钮 / 点遮罩 / `Esc` 都关当前条（`DialogQueue::close_current`）。
//!
//! ## 崩溃卡片额外带什么
//!
//! `DialogRequest`（presenter 的 DTO）只有标题/正文/按钮，而这次用户要的是**证据**：
//! 退出码、一句人话原因、**文件路径**、可展开的崩溃说明、以及三个按钮（导出 / 打开文件夹 /
//! 复制路径）。这些东西不进 `DialogRequest`——它是 presenter 的公共 DTO，塞进 UI 专用字段
//! 会把「谁拥有证据」搅浑。做法是：队列管**弹窗**，[`CrashCard`] 管**这次失败的事实**，
//! 两者按 `DialogId` 绑定，见 [`App::open_failure_dialog`]。
//!
//! 事实来源是 `unlucky_core::launch::crash_report`（启动线程收尾时登记的现场）。这里只读它，
//! 不重算：**界面不读盘、不猜原因**。

use unlucky_core::launch::crash_report::{CrashReport, Evidence};
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;
use unlucky_presenter::dialogs::{DialogId, DialogKind, DialogSource};

use crate::app::state::App;

pub(super) mod export;
#[cfg(test)]
mod tests;
pub(super) mod view;

/// 崩溃卡片：一次失败的**事实**（不含任何结论）。
#[derive(Debug, Clone)]
pub struct CrashCard {
    /// 现场。
    pub report: CrashReport,
    /// 「详细崩溃说明」是否展开。
    pub expanded: bool,
    /// 与哪一条队列请求绑定（关掉它时把卡片一起清掉）。
    pub dialog: DialogId,
    /// 上一次动作的结论（导出到哪 / 打不开文件夹的原因）。
    ///
    /// 为什么写在卡片里而不是只推 Toast：app-iced 的 Toast 通道**至今没有任何渲染点**
    /// （`state.toasts` 只被推入与过期，`view` 里一次都没读）。导出这种「必须看见落到哪」的
    /// 结果不能挂在一条不显示的通道上。
    pub outcome: Option<String>,
}

/// 弹窗消息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// 主按钮。
    Primary,
    /// 次按钮。
    Secondary,
    /// 点遮罩 / Esc。
    Dismiss,
    /// 展开 / 收起详细说明。
    Toggle,
    /// 打开日志文件夹。
    OpenFolder,
    /// 复制全部路径。
    CopyPaths,
    /// 复制第 `index` 条路径（`Modal::paths` 的下标）。
    CopyPath(usize),
}

/// 视图模型：**测试与渲染共用**的一份「这一屏该出现什么」。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct Modal {
    /// 标题。
    pub title: String,
    /// 正文（每行一段；都是运行时拼出来的 owned 字符串）。
    pub body: Vec<String>,
    /// 主按钮文字。
    pub primary: String,
    /// 次按钮文字（`None` = 不画）。
    pub secondary: Option<String>,
    /// 是否有次按钮（`Progress` 没有）。
    pub blocking_progress: bool,
    /// 崩溃卡片（普通弹窗为 `None`）。
    pub card: Option<CardView>,
    /// 动作结论（导出路径 / 失败原因）；`None` = 还没有动作。
    pub notice: Option<String>,
}

/// 崩溃卡片的展示内容。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct CardView {
    /// 展开 / 收起按钮的文字。
    pub toggle: String,
    /// 当前的展开态。
    pub expanded: bool,
    /// （标签, 路径）列表——**逐条可点**（点了复制该条）。
    pub paths: Vec<(String, String)>,
    /// 详细说明的小标题（`None` = 没有可展开的内容）。
    pub detail_title: Option<String>,
    /// 详细说明的正文行。
    pub detail_lines: Vec<String>,
    /// 三个按钮的文字。
    pub actions: [String; 3],
}

impl Modal {
    /// 正文全文（测试与「复制摘要」共用；换行拼起来）。
    #[must_use]
    #[cfg(test)]
    pub fn body_text(&self) -> String {
        self.body.join("\n")
    }
}

/// 从队列当前条与卡片造视图模型（**纯函数**：不读盘、不看 App）。
#[must_use]
pub(super) fn current(
    lang: Lang,
    request: &DialogRequestView<'_>,
    card: Option<&CrashCard>,
) -> Modal {
    let catalog = Catalog::of(lang);
    let mut modal = Modal {
        title: request.title.to_string(),
        body: vec![request.body.to_string()],
        primary: request.primary.to_string(),
        secondary: request.secondary.map(str::to_string),
        blocking_progress: request.blocking_progress,
        card: None,
        notice: None,
    };
    if let Some(card) = card {
        let detail = detail_of(&catalog, card);
        modal.body = failure_body(&catalog, &card.report);
        modal.primary = catalog.text(Key::CrashDialogExport).to_string();
        modal.secondary = Some(catalog.text(Key::CrashDialogClose).to_string());
        modal.card = Some(CardView {
            toggle: catalog
                .text(if card.expanded {
                    Key::CrashDialogHideDetail
                } else {
                    Key::CrashDialogShowDetail
                })
                .to_string(),
            expanded: card.expanded,
            paths: path_rows(&catalog, &card.report),
            detail_title: detail.0,
            detail_lines: detail.1,
            actions: [
                catalog.text(Key::CrashDialogExport).to_string(),
                catalog.text(Key::CrashDialogOpenFolder).to_string(),
                catalog.text(Key::CrashDialogCopyPaths).to_string(),
            ],
        });
        modal.notice = card.outcome.clone();
    }
    modal
}

/// 队列当前条的最小视图（避免把 `DialogRequest` 的细节漏进页面层）。
pub(super) struct DialogRequestView<'a> {
    /// 标题。
    pub title: &'a str,
    /// 正文。
    pub body: &'a str,
    /// 主按钮。
    pub primary: &'a str,
    /// 次按钮。
    pub secondary: Option<&'a str>,
    /// 进度框不可点主按钮。
    pub blocking_progress: bool,
}

impl<'a> DialogRequestView<'a> {
    /// 从 presenter 的请求取视图。
    pub(super) fn of(request: &'a unlucky_presenter::dialogs::DialogRequest) -> Self {
        Self {
            title: &request.title,
            body: &request.body,
            primary: request.kind.primary_label(),
            secondary: request.kind.secondary_label(),
            blocking_progress: request.kind.is_blocking_progress(),
        }
    }
}

/// 失败正文：退出码 + **一句人话原因** + 证据形态。
///
/// 三件事都必须有——用户投诉的原话是「找半天没找到崩溃日志」，所以第三句不是锦上添花：
/// 它要么指向游戏写的那份，要么明确说「这次没有，下面是捕获到的输出」。
fn failure_body(catalog: &Catalog, report: &CrashReport) -> Vec<String> {
    let mut body = Vec::new();
    if let Some(code) = report.exit_code {
        body.push(catalog.render(Key::CrashDialogExitCode, &[("code", &code.to_string())]));
    }
    if let Some(stage) = &report.stage {
        let label = stage_label(catalog, stage);
        body.push(catalog.render(Key::CrashDialogStageReason, &[("stage", label)]));
    } else if report.exit_code == Some(0) {
        body.push(catalog.text(Key::CrashDialogSilentFailure).to_string());
    } else {
        body.push(catalog.text(Key::CrashDialogStartedThenDied).to_string());
    }
    body.push(
        match report.evidence() {
            Evidence::GameCrashReport => catalog.text(Key::CrashDialogCrashReportFound),
            Evidence::CapturedOutput => catalog.text(Key::CrashDialogNoGameLog),
            Evidence::Nothing => catalog.text(Key::CrashDialogNoLogAtAll),
        }
        .to_string(),
    );
    body
}

/// 装配阶段 slug → 文案（**不**复用 core 的中文诊断串）。
fn stage_label(catalog: &Catalog, slug: &str) -> &'static str {
    let key = match slug {
        "version_manifest" => Key::LaunchStageVersionManifest,
        "inheritance" => Key::LaunchStageInheritance,
        "instance_config" => Key::LaunchStageInstanceConfig,
        "isolation" => Key::LaunchStageIsolation,
        "identity" => Key::LaunchStageIdentity,
        _ => Key::LaunchStageSpawn,
    };
    catalog.text(key)
}

/// 路径行（**启动器日志目录 + 该实例 latest.log + crash-reports 目录 + 会话 JSONL**）。
fn path_rows(catalog: &Catalog, report: &CrashReport) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if let Some(log) = &report.session_log {
        rows.push((
            catalog.text(Key::CrashDialogSessionLog).to_string(),
            log.display().to_string(),
        ));
    }
    if let Some(dir) = &report.launcher_logs {
        rows.push((
            catalog.text(Key::CrashDialogLauncherLogs).to_string(),
            dir.display().to_string(),
        ));
    }
    if let Some(path) = report.latest_log_path() {
        rows.push((
            catalog.text(Key::CrashDialogGameLog).to_string(),
            path.display().to_string(),
        ));
    }
    if let Some(dir) = report.crash_reports_dir() {
        rows.push((
            catalog.text(Key::CrashDialogCrashReports).to_string(),
            dir.display().to_string(),
        ));
    }
    rows
}

/// 详细说明：有崩溃报告贴 Description/栈片段，否则贴启动器捕获的尾巴。
fn detail_of(catalog: &Catalog, card: &CrashCard) -> (Option<String>, Vec<String>) {
    let report = &card.report;
    if let Some(artifact) = report
        .artifacts
        .iter()
        .find(|artifact| artifact.source == "crash_report")
    {
        let mut lines = Vec::new();
        if let Some(excerpt) = &artifact.excerpt {
            lines.extend(excerpt.lines().map(str::to_string));
        } else {
            lines.push(artifact.path.display().to_string());
        }
        let title = format!(
            "{} {}",
            catalog.text(Key::LogsDiagnosisTitle),
            artifact.path.display()
        );
        return (Some(title), lines);
    }
    if report.stderr_tail.is_empty() {
        return (None, Vec::new());
    }
    let title = catalog.render(
        Key::CrashDialogCapturedTail,
        &[("count", &report.stderr_tail.len().to_string())],
    );
    (Some(title), report.stderr_tail.clone())
}

// --------------------------------------------------------------- 与 App 的接线

impl App {
    /// 队列当前条 → 视图模型（`view()` 与测试共用同一个入口）。
    pub(super) fn dialog_modal(&self) -> Option<Modal> {
        let request = self.state.dialogs.current()?;
        let view = DialogRequestView::of(request);
        let card = self
            .dialog_card
            .as_ref()
            .filter(|card| card.dialog == request.id);
        Some(current(self.lang(), &view, card))
    }

    /// 收到一次启动收尾：失败就弹框（成功不打扰）。
    ///
    /// 只在**真的失败**时弹：`LogOutcome::Ok` 且没有阶段失败 ⇒ 什么都不做，与既有 Toast 口径一致。
    pub(super) fn open_failure_dialog(
        &mut self,
        finished: &unlucky_presenter::dto::LaunchFinished,
    ) {
        let failed = finished.outcome != unlucky_presenter::dto::LogOutcome::Ok
            || finished.failure.is_some();
        if !failed {
            return;
        }
        let Some(report) = unlucky_core::launch::crash_report::failing_launch() else {
            return;
        };
        if !finished.session.is_empty() && report.session != finished.session {
            // 登记的是另一场（并发启动）：不拿它去解释这一场。
            return;
        }
        let catalog = self.lang().catalog();
        let crashed = report.started();
        let title = catalog.text(if crashed {
            Key::CrashDialogTitleCrashed
        } else {
            Key::CrashDialogTitleFailed
        });
        let summary = self.dialog_summary(&report);
        let at = self.lang();
        let id = self.state.dialogs.push(
            DialogSource::UserAction,
            title,
            summary,
            DialogKind::Confirm {
                primary: Catalog::of(at).text(Key::CrashDialogExport).to_string(),
                secondary: Catalog::of(at).text(Key::CrashDialogClose).to_string(),
            },
        );
        self.dialog_card = Some(CrashCard {
            report,
            // 默认展开：正文已经说「下面是捕获的最后若干行」，收着就成了空口承诺。
            expanded: true,
            dialog: id,
            outcome: None,
        });
        self.view_dirty = true;
    }

    /// 队列标题下的那一行摘要（弹窗正文由 [`current`] 按卡片重算，这里给它一份独立可读的短句）。
    fn dialog_summary(&self, report: &CrashReport) -> String {
        let catalog = self.lang().catalog();
        match report.exit_code {
            Some(code) => catalog.render(Key::CrashDialogExitCode, &[("code", &code.to_string())]),
            None => catalog.text(Key::CrashDialogTitleFailed).to_string(),
        }
    }

    /// 关掉当前弹窗（主/次按钮、遮罩、`Esc` 共用）。
    pub(super) fn close_dialog(&mut self) {
        let closed = self.state.dialogs.current().map(|request| request.id);
        self.state.dialogs.close_current();
        if self
            .dialog_card
            .as_ref()
            .is_some_and(|card| closed == Some(card.dialog))
        {
            self.dialog_card = None;
        }
        self.view_dirty = true;
    }

    /// 展开 / 收起详细说明。
    pub(super) fn toggle_dialog_detail(&mut self) {
        if let Some(card) = &mut self.dialog_card {
            card.expanded = !card.expanded;
            self.view_dirty = true;
        }
    }
}
