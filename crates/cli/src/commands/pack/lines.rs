//! `pack import` 的**人读行**（M321 从 `pack.rs` 搬出来：那边贴着 400 行硬线，而本轮要在
//! 人读输出里加「处理器」与「告警」两段）。
//!
//! 搬过来的是 M161 的三档回滚判据与几张清单的渲染，**行为逐字未变**；新增的是
//! [`warnings_lines`] / [`processor_lines`] / [`step_line`] 与进度回调工厂 [`progress_sink`]，
//! 理由见各自的文档。
//!
//! # 结局出词那一组在 `lines/rollback.rs`（M344）
//!
//! [`status_label`] / [`rollback_line`] / `rollback_stage` 与那套三档枚举搬去了 [`rollback`]
//! 子模块：本文件当时 397 行贴着 400 行硬线，而这一组是内聚的（搬移 72 行**逐字未变**）。
//! 路径由下面的 `pub(crate) use` 保持不变，调用点一字未改；「三档为什么不并进 core」那段
//! 解释跟着代码一起搬了过去。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use unlucky_core::addon::import::{
    ConflictChoice, ConflictReason, FileAction, ImportProgress, ImportProgressSink, ImportReport,
    ImportStage, MissingFile, MissingReason,
};
use unlucky_core::addon::loader::processors::StepOutcome;
use unlucky_i18n::Key;

use crate::catalog::{java_origin_text, CATALOG};

use super::wording::step_body;

#[path = "lines/rollback.rs"]
mod rollback;

// 结局出词那一组搬去了 `lines/rollback.rs`（M344 为腾行数），路径靠这句保持不变：
// `pack.rs` 与 `pack/tests.rs` 仍按 `lines::{status_label, rollback_line, rollback_stage, RollbackStage}`
// 取用，调用点一字未改。
pub(crate) use rollback::{rollback_line, status_label};

/// 这两个名字 **在本模块之外只有 `pack/tests.rs` 的用例**用得到：三档判据本身在生产路径上
/// 由 `rollback.rs::status_key` 自己用（不是测试专用代码），所以 `cfg(test)` 只该挂在这句
/// **再导出**上——否则 `-D warnings` 会因为「导入了没人用的名字」把构建判红。
#[cfg(test)]
pub(crate) use rollback::{rollback_stage, RollbackStage};

/// 缺失成因 → 目录 key。**穷尽匹配**：core 哪天多一档 `MissingReason`，这里编译不过。
const fn missing_key(reason: MissingReason) -> Key {
    match reason {
        MissingReason::Offline => Key::PackMissingOffline,
        MissingReason::NoSource => Key::PackMissingNoSource,
        MissingReason::FetchFailed => Key::PackMissingFetchFailed,
    }
}

/// 冲突成因 → 目录 key（同上，穷尽匹配）。
const fn conflict_key(reason: ConflictReason) -> Key {
    match reason {
        ConflictReason::UserModified => Key::PackConflictUserModified,
        ConflictReason::UserDeleted => Key::PackConflictUserDeleted,
        ConflictReason::PreExisting => Key::PackConflictPreExisting,
    }
}

/// 冲突处置 → 目录 key（GUI 的导入取舍清单用的是同两条）。
const fn choice_key(choice: ConflictChoice) -> Key {
    match choice {
        ConflictChoice::KeepUser => Key::PackChoiceKeepUser,
        ConflictChoice::Overwrite => Key::PackChoiceOverwrite,
    }
}

/// 导入阶段 → 目录 key（同上，穷尽匹配：core 多一档 `ImportStage`，这里编译不过）。
///
/// 六条文案与 GUI 进度行用的是同一批（`PackStageProbe/Extract/Fetch/Loader/Commit/Register`），
/// 所以 `阶段：probe → extract → …` 里那些内部 id 不再漏进中文句子。**没有新增文案**。
const fn stage_key(stage: ImportStage) -> Key {
    match stage {
        ImportStage::Probe => Key::PackStageProbe,
        ImportStage::Extract => Key::PackStageExtract,
        ImportStage::Fetch => Key::PackStageFetch,
        ImportStage::Loader => Key::PackStageLoader,
        ImportStage::Commit => Key::PackStageCommit,
        ImportStage::Register => Key::PackStageRegister,
    }
}

/// 装载器段每处理多少个文件印一行（与 `commands::install::progress` 同一档）。
///
/// 原版 + assets 有几千个文件，逐条印会刷屏；而这一段的 `ImportProgress` 只带
/// 「已处理 / 总数 / 当前文件」，**不带下载/跳过的结局**，所以措辞也不能说"下载"——
/// 只报事实（处理到第几个）。M210/D11 的教训：进度里的数字必须是真的发生过的计数。
const PROGRESS_EVERY: usize = 32;

/// 走到过的阶段（**按发射顺序、去重**）→ 一行人读：`阶段：读取清单 → 解包覆盖内容 → …`。
///
/// 一个阶段都没走到时返回 `None`（失败的导入可能连 `Probe` 都没发就返回了）。
///
/// # 为什么不用 `PackStageLine`（M324）
///
/// 那条模板是 `步骤 {index}/{total} · {stage}`，序号取的是 GUI 进度条上的**计划位次**；而 CLI
/// 这行只列**真的发射过**的阶段（取消 / 中途失败会提前收车），把 `{index}/{total}` 填进去就成了
/// 「给没跑的步骤编号」。所以取六条单条文案自己拼——词仍是目录里那六句，两个壳说同一套阶段名。
pub(super) fn stages_line(stages: &[ImportStage]) -> Option<String> {
    if stages.is_empty() {
        return None;
    }
    let labels = stages
        .iter()
        .map(|stage| CATALOG.text(stage_key(*stage)))
        .collect::<Vec<_>>();
    Some(format!("阶段：{}", labels.join(" → ")))
}

/// `--json` 的 `stages`：仍是 **core 的稳定 id**（`crates/cli/tests/pack_test.rs` 断言过
/// `probe` / `commit`）。机器位说 id、人读位说目录文案，同一份记录两副面孔。
pub(super) fn stage_ids(stages: &[ImportStage]) -> Vec<&'static str> {
    stages.iter().map(|stage| stage.as_str()).collect()
}

/// 导入进度回调（记阶段 + 装载器段的逐文件行）。
///
/// 为什么是工厂而不是在 `run()` 里就地写闭包：`run()` 那份闭包只能往真 stderr 打，
/// 「喂一条 `Loader` 进度事件 ⇒ 真的产出一行」就没法离线断言。把输出口做成参数
/// （生产传 [`progress_line`](crate::output::progress_line)，用例传记录器），这条判据才钉得住。
///
/// 记录的是 [`ImportStage`] 本身（`Copy` 的小枚举）而不是 `as_str()` 的 `&'static str`：阶段名
/// 要到收尾时才渲染成人读行，存原值就不必在两种字符串之间倒手。**去重口径不变**——只在阶段
/// 变化时追加一条，几千个文件也只留六档。
pub(super) fn progress_sink(
    stages: Arc<Mutex<Vec<ImportStage>>>,
    out: Arc<dyn Fn(&str) + Send + Sync>,
) -> ImportProgressSink {
    let printed = AtomicUsize::new(0);
    Arc::new(move |progress: &ImportProgress| {
        if let Ok(mut seen) = stages.lock() {
            if seen.last() != Some(&progress.stage) {
                seen.push(progress.stage);
            }
        }
        let Some(line) = loader_file_line(progress) else {
            return;
        };
        let seq = printed.fetch_add(1, Ordering::Relaxed) + 1;
        // 第一行、每 PROGRESS_EVERY 个、以及最后一行（`done == total` 收尾）。
        if seq == 1 || seq.is_multiple_of(PROGRESS_EVERY) || progress.done == progress.total {
            out(&line);
        }
    })
}

/// 装载器段的逐文件行（其它阶段、或没有当前条目时返回 `None`）。
///
/// 措辞只说「处理到第几个」：`ImportProgress` 不带下载/跳过的结局，说"下载"就是编
/// （重装同版本时一个字节都没下，进度却会走完）。
pub(super) fn loader_file_line(progress: &ImportProgress) -> Option<String> {
    if progress.stage != ImportStage::Loader {
        return None;
    }
    let current = progress.current.as_deref()?;
    Some(if progress.total > 0 {
        format!("装载器 {}/{} {current}", progress.done, progress.total)
    } else {
        format!("装载器 {current}")
    })
}

/// 逐动作用数一条。
pub(super) fn count_action(report: &ImportReport, action: FileAction) -> usize {
    report
        .files
        .iter()
        .filter(|file| file.action == action)
        .count()
}

/// 未支持 component 的段（**逐条**一行：不静默丢，M172 的硬线）。
///
/// 括号里那句走目录（[`Key::PackReportRejectedNote`]「客户端不支持」）——core 那条记录的
/// `message_key` 恒为 `pack.import.rejected.component`，它是给脚本查表的稳定 id，不是文案。
pub(super) fn rejected_lines(report: &ImportReport) -> Vec<String> {
    let note = CATALOG.text(Key::PackReportRejectedNote);
    report
        .rejected_components
        .iter()
        .map(|component| {
            format!(
                "未支持的 component：{} {}（{note}）",
                component.uid,
                component.version.as_deref().unwrap_or("-"),
            )
        })
        .collect()
}

/// 冲突段（逐条一行：成因 + 本次处置）。
///
/// 两半都取目录里那两句：GUI 的冲突/选择清单用的是同一批 key，两个壳不该给同一个事实
/// 各说一套（旧版这里手写「保留你的文件 / 按包覆盖」，GUI 写「保留我的 / 用包里的」）。
pub(super) fn conflict_lines(report: &ImportReport) -> Vec<String> {
    report
        .conflicts
        .iter()
        .map(|conflict| {
            format!(
                "冲突：{}（{}，已{}）",
                conflict.path,
                CATALOG.text(conflict_key(conflict.reason)),
                CATALOG.text(choice_key(conflict.choice)),
            )
        })
        .collect()
}

/// 缺文件段（逐条一行：这是什么 + 为什么搞不到 + 引擎原话）。
pub(super) fn missing_lines(report: &ImportReport) -> Vec<String> {
    report
        .missing
        .iter()
        .map(|missing| format!("缺文件：{}（{}）", missing.display, missing_note(missing)))
        .collect()
}

/// 一条缺失的「为什么搞不到」。
///
/// `PackMissingFetchFailed` 的模板本身就是「成因 + 引擎原话」（`下载失败：{detail}`），原样渲染；
/// 另两档 GUI 的短句只是标签，而 `detail` 是 core 对成因的那句话（离线 / 补不出地址的说明），
/// 人读一行放得下 ⇒ 两段都留（事实不能丢：M323 的判据是改前后 `detail` 逐字相同）。
fn missing_note(missing: &MissingFile) -> String {
    let key = missing_key(missing.reason);
    if missing.reason == MissingReason::FetchFailed {
        return CATALOG.render(key, &[("detail", &missing.detail)]);
    }
    if missing.detail.trim().is_empty() {
        return CATALOG.text(key).to_owned();
    }
    format!("{}：{}", CATALOG.text(key), missing.detail)
}

/// 告警 / 注记段（**M321 修的洞**：`pack.rs` 以前全文没有 `warnings`，于是 `--json` 里明明有
/// 6 条、人读输出一条都不印——同一个仓的 `ull install --loader` 会印）。
///
/// 渲染的是 `report.warnings`（数据层），不是 CLI 自己拼的字符串：GUI 走同一位，
/// 两边看到的必须是同一批告警。
///
/// # 两段按 core 给的级别切（M339，取代 M321 r1 那句「告警/注记 N 条」）
///
/// `report.warnings` 是一条**混装**通道：真包那次 4 条里有 2 条是成功注记
/// （`note.loader_installed` / `note.registered`），只有 2 条是 `note.loader_warning`。
/// M321 因为「严重度在 core 数据里不存在」，只好把两种混在一个标题下（`告警/注记 N 条`），
/// 用户得自己看行尾才能分辨。M339 把级别补进 `ImportIssue::level` 之后，这里**按数据切**：
/// 告警一段、注记一段，各算各的条数——CLI 不再抄任何 key→级别的分类表。
/// 人读行仍**不印级别词**（分组本身就是级别），级别本身在 `--json` 的 `warnings[].level` 里。
///
/// # 为什么人读行不再印 `message_key`（M323）
///
/// `message` 已经是人话（core 写的那句），括号里那个 `pack.import.note.…` 对用户是纯粹的噪音：
/// 同一个事实 GUI 只显示 `message`。稳定 key 仍在 `--json`（`warnings[].message_key`）里，
/// 脚本要查表照样拿得到。
///
/// # 段标题读目录（M342）
///
/// 标题取 [`Key::PackReportWarnings`] / [`Key::PackReportNotes`]——GUI 的报告与预检两屏读的
/// 也是这两个 key。以前这里是手写字面量，改目录时 CLI 不跟；排版也统一成目录那句
/// （`要处理：2 条`）：同一条信息一种排版，与其余各行同为「标签：内容」。
pub(super) fn warnings_lines(report: &ImportReport) -> Vec<String> {
    let mut lines = Vec::new();
    for (key, warning) in [
        (Key::PackReportWarnings, true),
        (Key::PackReportNotes, false),
    ] {
        let picked = report
            .warnings
            .iter()
            .filter(|issue| issue.is_warning() == warning)
            .collect::<Vec<_>>();
        if picked.is_empty() {
            continue;
        }
        lines.push(CATALOG.render(key, &[("count", &picked.len().to_string())]));
        for issue in picked {
            lines.push(format!("  · {}", issue.message));
        }
    }
    lines
}

/// 第六跳 processor 链那一段（逐步骤一行；以前这条链跑完什么都不说，现在人读一段、
/// `--json` 一段（`report.processors`），同一份数据两个出口）。
pub(super) fn processor_lines(report: &ImportReport) -> Vec<String> {
    let Some(processors) = &report.processors else {
        return Vec::new();
    };
    let total = processors.steps.len();
    let mut lines = vec![format!(
        "处理器：执行 {} 步 / 跳过 {} 步（Java {}，来源 {}）",
        processors.executed,
        processors.skipped,
        processors.java,
        // **M325**：来源取目录里那句人话（以前印的是 core 的 slug）。
        java_origin_text(processors.java_origin)
    )];
    for step in &processors.steps {
        lines.push(format!(
            "  处理器 {}/{total} {} → {}",
            step.index,
            step.label,
            step_body(step.status, step.outputs, u128::from(step.elapsed_ms))
        ));
    }
    lines
}

/// 实时那一行（stderr，逐步打印）：与 `ull install --loader` 的 `step_sink` 同一套措辞。
///
/// 不带 `N/total`：导入侧拿不到计划的总步数（`StepSink` 的回调里只有一步），所以只说"第几步"。
/// 终点那一段由 [`processor_lines`] 在人读输出里补齐。
pub(super) fn step_line(outcome: &StepOutcome) -> String {
    format!(
        "处理器 {} {} → {}",
        outcome.index,
        outcome.label,
        // 结局那半句只有一处实现（M346：`pack/wording.rs`，读目录 `PackProcessorStep*`）——
        // 本文件与 `install` 的两个出口都调它（判据是 `wording.rs` 里那条按路径豁免的源码扫描用例）。
        step_body(outcome.status.as_str(), outcome.outputs, outcome.elapsed_ms)
    )
}

// 一步的人读后半句（`executed` / `skipped` 两个稳定 id → 词）**搬到了
// `crate::commands::pack::wording::step_body`**（M346）：它与 `install` 的两个出口、
// `instances adopt` 的登记行同住一处，读目录 `PackProcessorStep*`，两个壳说同一套字。
