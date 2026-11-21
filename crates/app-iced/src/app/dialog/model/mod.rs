//! 弹窗的**视图模型**：队列当前条 + 崩溃卡片 → 「这一屏该出现什么」（M298 从 `dialog.rs` 拆出）。
//!
//! 纯函数、不读盘、不看 `App`：测试与渲染共用同一份（`crate::app::dialog::{current, Modal}`）。
//! 拆分的直接原因只有行数（`dialog.rs` 当时 390 行、贴着 400 行硬线），**没有行为变化**：
//! 每个函数体逐行搬移，符号经父模块 `pub use` 保路径。

use unlucky_core::launch::crash_report::{CrashReport, Evidence};
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;

use unlucky_presenter::dto::JavaGuardDirection;

use super::{CrashCard, JavaGuardCard, Message as DialogMessage};

mod guard;

use guard::install_state_text;
pub(crate) use guard::{guard_body, guard_title};

/// 视图模型：**测试与渲染共用**的一份「这一屏该出现什么」。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Modal {
    /// 标题。
    pub title: String,
    /// 正文（每行一段；都是运行时拼出来的 owned 字符串）。
    pub body: Vec<String>,
    /// 主按钮文字。
    pub primary: String,
    /// 次按钮文字（`None` = 不画）。
    pub secondary: Option<String>,
    /// 第三枚按钮（M334：只有「Java 不符」那个框有，`None` = 不画）。
    pub tertiary: Option<String>,
    /// 主按钮（蓝底 filled、最右）派哪条动作（`None` = 默认的 `DialogMessage::Primary`）。
    ///
    /// M349 r2 / 复审 P2-3：建议框的主按钮是「下载 Java 8」，而那条出口的动作是 `Tertiary`。
    /// **动作跟着按钮走、不跟着位置猜**——否则点了 filled 那一枚会跳去设置页。
    pub primary_action: Option<super::Message>,
    /// 第三枚按钮（outlined、最左）派哪条动作（`None` = 默认的 `DialogMessage::Tertiary`）。
    pub tertiary_action: Option<super::Message>,
    /// 是否有次按钮（`Progress` 没有）。
    pub blocking_progress: bool,
    /// 崩溃卡片（普通弹窗为 `None`）。
    pub card: Option<CardView>,
    /// 动作结论（导出路径 / 失败原因）；`None` = 还没有动作。
    pub notice: Option<String>,
}

/// 崩溃卡片的展示内容。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CardView {
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
pub(crate) fn current(
    lang: Lang,
    request: &DialogRequestView<'_>,
    card: Option<&CrashCard>,
    guard: Option<&JavaGuardCard>,
) -> Modal {
    let catalog = Catalog::of(lang);
    let mut modal = Modal {
        title: request.title.to_string(),
        body: vec![request.body.to_string()],
        primary: request.primary.to_string(),
        secondary: request.secondary.map(str::to_string),
        tertiary: None,
        primary_action: None,
        tertiary_action: None,
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
    if let Some(guard) = guard {
        // M331：**启动前**的框。四段正文的顺序是有意的——先说是哪一条实例（用户在列表里
        // 可能点了好几下），再说「需要什么 / 现在是什么」（这两个数字是全框的价值），
        // 最后一句说清后果（用户点「仍要启动」之前必须知道大概率会崩）。
        // M349：`Advisory` 那一支走**另一份**标题与正文（它不是「不符」，只是先问一句）。
        modal.title = guard_title(&catalog, &guard.view).to_string();
        modal.body = guard_body(&catalog, &guard.view);
        match guard.view.direction {
            // 硬拦：本仓的主按钮 = **推荐动作**，而这里推荐的是「换一个 Java」。
            // 三枚按钮的档位与位置一个字节都不动（M331/M334 复审过的那一版）。
            JavaGuardDirection::TooOld | JavaGuardDirection::TooNew => {
                // M334：第三个出口——装一个对版本的 Java（手里只有 17/21 时，「去设置」变不出来）。
                let (label, line) = install_state_text(&catalog, guard, guard.view.install_major);
                if let Some(line) = line {
                    modal.body.push(line);
                }
                modal.primary = catalog.text(Key::JavaGuardOpenSettings).to_string();
                modal.secondary = Some(catalog.text(Key::JavaGuardLaunchAnyway).to_string());
                modal.tertiary = Some(label);
            }
            // 建议（M349 r2 / 复审 P2-3）：正文推荐的就是「装 Java 8」，主按钮得是它。
            // 于是三枚按钮**换位**：filled = 下载 Java 8、text = 仍要启动、outlined = 去设置改 Java。
            // 动作跟着图标走（`Modal::*_action`），不是跟着位置猜——点 filled 那一枚派的是
            // `DialogMessage::Tertiary`（装机/装好后重试），点 outlined 那一枚才去设置页。
            JavaGuardDirection::Advisory(_) => match guard.view.install_major {
                Some(major) => {
                    let (label, line) = install_state_text(&catalog, guard, Some(major));
                    if let Some(line) = line {
                        modal.body.push(line);
                    }
                    modal.primary = label;
                    modal.primary_action = Some(DialogMessage::Tertiary);
                    modal.secondary = Some(catalog.text(Key::JavaGuardLaunchAnyway).to_string());
                    modal.tertiary = Some(catalog.text(Key::JavaGuardOpenSettings).to_string());
                    modal.tertiary_action = Some(DialogMessage::Primary);
                }
                // 钉过实例 Java（M349 r3 / 复审 r2 P2-1）：装机出口**注定被影子掉**
                // （钉死那份优先，托管目录里装什么都挑不中），所以不给它。剩下两枚都能落地：
                // filled = 仍要启动（这一次放行）、outlined = 去**实例**的启动参数
                // （全局设置页对这条实例无效——实例层优先级更高）。
                None => {
                    modal.primary = catalog.text(Key::JavaGuardLaunchAnyway).to_string();
                    modal.primary_action = Some(DialogMessage::Secondary);
                    // 中间那一格要**清掉**：队列请求自带的次按钮文案就是「仍要启动」，
                    // 不清的话它会跟上面的主按钮重复出现两次。
                    modal.secondary = None;
                    modal.tertiary = Some(catalog.text(Key::JavaGuardInstanceArgs).to_string());
                    modal.tertiary_action = Some(DialogMessage::InstanceLaunchArgs);
                }
            },
        }
        modal.card = None;
    }
    modal
}

/// 队列当前条的最小视图（避免把 `DialogRequest` 的细节漏进页面层）。
pub(crate) struct DialogRequestView<'a> {
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
    pub(crate) fn of(request: &'a unlucky_presenter::dialogs::DialogRequest) -> Self {
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
pub(crate) fn path_rows(catalog: &Catalog, report: &CrashReport) -> Vec<(String, String)> {
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
