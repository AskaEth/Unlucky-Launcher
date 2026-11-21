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
//! CLI 那一壳的同一件事在 `crates/cli/src/commands/pack/lines.rs::warnings_lines`：**自 M342 起
//! 它与本文件读同一对 key**（`PackReportWarnings` / `PackReportNotes`，同样 `render(key, count)`），
//! 段标题（含计数与「标签：内容」排版）都不再由 CLI 自己拼 ⇒ 改目录那一句，两个壳一起变
//! （M342 的判据就是「改一个字，CLI 与界面两侧同时红」）。
//!
//! 共用止于**段标题**：逐条行以下 CLI 仍各排各的版。但「排版」与「文案」要分开读——四类里三类的
//! **措辞**已经并进目录（M344 收结局词、M346 收单步结局与登记行），CLI 只剩排版外壳与自己独有的
//! 字段；只有处理器汇总行仍由 CLI 自己拼整行。要查「CLI 到底读了哪些 key」，
//! `git grep -n 'Key::Pack' crates/cli/src` 比这张清单准（清单会过期，那道 grep 每次现取；M407
//! 跑过，退出码 0，输出 7 个文件）。
//!
//! **这里只写 `文件::符号`，不写行号**（M407）：行号每改一行就静默失效——这张表原先写的三处
//! `pack/lines.rs` 行号，M407 实测那个文件只有 327 行（`:349` / `:387` 早已不存在），而 `:149` 那行
//! 是个空行、`status_label` 根本不在那个文件里（M344 已把它搬去 `lines/rollback.rs`）。符号改名会
//! 让检索与编译当场报出来，行号不会。
//!
//! - 结局那一行：出词在 `crates/cli/src/commands/pack/lines/rollback.rs::status_label`（M344 从
//!   `pack/lines.rs` 拆出，那边贴着 400 行硬线），读 `PackStatusSuccess` / `PackStatusPartial` /
//!   `PackStatusFailed`（失败另两档 `PackStatusFailedNotStarted`、`PackStatusFailedIncomplete`）/
//!   `PackStatusCancelled` 与 `PackCancelIncomplete`（取消两档）。**CLI 手写的只剩「结果：」这个标签
//!   与文件计数四栏的排版**，在 `crates/cli/src/commands/pack.rs::outcome`。界面读同一批 key 的是
//!   `crates/app-iced/src/pages/content/packui/model/report.rs::status_title`；
//! - 单步结局：`crates/cli/src/commands/pack/wording.rs::step_body`（M346 把 `pack/lines.rs` 与
//!   `install` 两处各一份的手写中文并成这一份，唯一实现），读 `PackProcessorStepRan` /
//!   `PackProcessorStepSkipped`。调用点 4 处 / 3 个文件：`crates/cli/src/commands/pack/lines.rs::processor_lines`
//!   与 `crates/cli/src/commands/pack/lines.rs::step_line`、
//!   `crates/cli/src/commands/install/loader_processors.rs::step_sink`、
//!   `crates/cli/src/commands/install/report_loader/processors.rs::processor_lines`；
//!   界面读同一对 key：`crates/app-iced/src/pages/content/packui/model/report_processor.rs::step_row`；
//! - 处理器汇总行：**四类里唯一仍由 CLI 自己拼整行的**——`crates/cli/src/commands/pack/lines.rs::processor_lines`
//!   里那句 `处理器：执行 N 步 / 跳过 N 步（Java …，来源 …）`；只有括号里那五位来源读目录
//!   `PackProcessorJavaFrom*`（映射在 `crates/cli/src/catalog.rs::java_origin_key`）。界面读
//!   `PackProcessorSummary`（`crates/app-iced/src/pages/content/packui/model/report_processor.rs::section`），
//!   但它只管 `执行 N 步 · 跳过 N 步` 那截、排版与 CLI 那句**还没并**；
//! - 「已登记实例」四个出口共用一份实现：`crates/cli/src/commands/pack/wording.rs::registered_line`
//!   （M346），前缀读目录 `PackReportRegistered`，id / 版本 / 「复用已有档案」是 CLI 独有细节。
//!   四个调用点：`crates/cli/src/commands/pack.rs::outcome`、
//!   `crates/cli/src/commands/install/report.rs::human_lines`、
//!   `crates/cli/src/commands/install/report_loader.rs::human_lines`、
//!   `crates/cli/src/commands/instances/adopt.rs::run`；界面读同一句的是
//!   `crates/app-iced/src/pages/content/packui/model/report.rs::report_of`。

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
