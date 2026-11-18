//! 一档注记：**按 core 给的级别**切出「告警」或「注记」那一段（M339）。
//!
//! 从 `model/report.rs` 搬出来只为守 400 行硬线；它与 `report_processor.rs` 同一族——都是
//! 「报告里的一段怎么从数据层切出来」的纯函数。
//!
//! # M341：两屏共用这一个分档
//!
//! 预检那一屏（[`super::pick`]）要画的是**同一批 core `ImportIssue`**（`ImportScanView::notes`
//! 与 `ImportReportView::warnings` 只是两个阶段的两次搬运）。所以这里不再绑 `ImportReportView`，
//! 改成吃 `&[ImportIssueView]`：报告传 `report.warnings`、预检传 `scan.notes`。
//!
//! **GUI 这一壳里「按 level 分组」只有这一处**（`crates/app-iced` 内 `grep 'issue.level =='` 只命中
//! 本文件，报告与预检两处都是调它）：两屏各写一份 `filter(|issue| issue.level == ...)` 必然演化成
//! 「预检说只是告知、报告说要处理」。
//!
//! CLI 那一壳的同一件事在 `crates/cli/src/commands/pack/lines.rs` 的 `warnings_lines`：**自 M342 起
//! 它与本文件读同一对 key**（`PackReportWarnings` / `PackReportNotes`，同样 `render(key, count)`），
//! 段标题（含计数与「标签：内容」排版）都不再由 CLI 自己拼 ⇒ 改目录那一句，两个壳一起变
//! （M342 的判据就是「改一个字，CLI 与界面两侧同时红」）。
//!
//! 共用止于**段标题**。逐条行以下 CLI 仍各排各的版，目录改了它不跟——以下四类仍是 CLI 手写
//! （清单本身会过期，`git grep -n 'Key::Pack' crates/cli/src` 才是准的）：
//!
//! - 状态词与「结果：」整行：`pack/lines.rs:149` 的 `status_label`（成功 / 部分成功 / 失败三档 /
//!   已取消两档）与 `pack.rs:213` 的排版；界面读 `PackStatusSuccess` / `PackStatusPartial` /
//!   `PackStatusFailed*` / `PackStatusCancelled` / `PackCancelIncomplete`；
//! - 单步结局：`pack/lines.rs:387` 的 `step_body`（`执行完成` / `跳过（产物已就绪）`），
//!   `install/loader_processors.rs:140` 与 `install/report_loader.rs:298` 同句；界面读
//!   `PackProcessorStepRan` / `PackProcessorStepSkipped`；
//! - 处理器汇总行：`pack/lines.rs:349`（`处理器：执行 N 步 / 跳过 N 步（Java …）`）；界面读
//!   `PackProcessorSummary`（Java 来源那五位 CLI 已经在读目录的 `PackProcessorJavaFrom*`）；
//! - 「已登记实例」四处：`pack.rs:251`、`install/report.rs:166`、`install/report_loader.rs:244`、
//!   `instances/adopt.rs:42`；界面读 `PackReportRegistered`。

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{ImportIssueLevelView, ImportIssueView};

/// 某一档（[`ImportIssueLevelView::Warning`] / [`ImportIssueLevelView::Note`]）的标题与逐条原文。
///
/// **标题里的计数从逐条行数导出**：写「告警 3 条」而下面列了 2 行，就是本仓反复收的那种自相矛盾
/// （`ImportReportView::warnings` 与 `ImportScanView::notes` 都是**混装**通道，不按级别切就必然
/// 要猜）。两个数字只可能来自同一处，于是不可能对不上。
///
/// `None` = 这一档一条都没有 ⇒ 调用方整段不画（空标题比不画更糟：那是本仓「空值不画标签」的旧例）。
pub(super) fn section(
    issues: &[ImportIssueView],
    level: ImportIssueLevelView,
    key: Key,
    catalog: Catalog,
) -> (Option<String>, Vec<String>) {
    let lines: Vec<String> = issues
        .iter()
        .filter(|issue| issue.level == level)
        .map(|issue| issue.message.clone())
        .collect();
    let title =
        (!lines.is_empty()).then(|| catalog.render(key, &[("count", &lines.len().to_string())]));
    (title, lines)
}
