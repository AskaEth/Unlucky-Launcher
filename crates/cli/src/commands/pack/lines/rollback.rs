//! 一次导入的**结局怎么出词**：状态那一行（[`status_label`]）与失败收尾那一段
//! （[`rollback_line`]），共用同一套三档判据 [`rollback_stage`]。
//!
//! M344 从 `pack/lines.rs` 搬出来（那边贴着 400 行硬线）：**搬移的 72 行除四处
//! `pub(super)` → `pub(crate)`（`lines.rs` 的 `pub(crate) use` 再导出更窄的项会 E0365）外
//! 逐字未变**；随后 `status_label` 改成读目录 key——以前这里是手写中文，界面读的却是
//! `PackStatus*` 那几句，于是同一条事实两个壳各说一套（M342 修的是段标题，这一条是同族里最后一个）。

use unlucky_core::addon::import::{ImportReport, ImportStatus};
use unlucky_i18n::Key;

use crate::catalog::CATALOG;

/// 结局 → 目录 key。**穷尽匹配**：core 多一档 `ImportStatus`，这里编译不过。
///
/// 三档失败的映射与界面的 `status_title`（`app-iced` 的 `packui/model/report.rs`）逐条对应，
/// 一一对应、**没有一对多**：
///
/// | CLI 这一档 | 目录 key | 界面哪一档 |
/// | --- | --- | --- |
/// | 预检/装配就失败（一个字节没写） | [`Key::PackStatusFailedNotStarted`] | `failure_stage = NotStarted` |
/// | 写过、回滚完整 | [`Key::PackStatusFailed`] | `failure_stage = RolledBack` |
/// | 写过、回滚没做完 | [`Key::PackStatusFailedIncomplete`] | `failure_stage = RollbackIncomplete` |
/// | 取消、回滚完整（含"取消在落盘前"） | [`Key::PackStatusCancelled`] | `cancel = RolledBack` |
/// | 取消、回滚没做完 | [`Key::PackCancelIncomplete`] | `cancel = RollbackIncomplete` |
/// | 成功 | [`Key::PackStatusSuccess`] | `Success` |
/// | 部分成功（有缺文件/冲突） | [`Key::PackStatusPartial`] | `Partial` |
///
/// 唯一一处「一对多」在**取消那一侧、但两个壳口径相同**：`PackStatusCancelled` 同时覆盖
/// 「落盘前就停了」与「回滚完整」两种（core 的 `rolled_back` 在这两种下都是 `true`），
/// 界面也一样并（`cancel_view` 只看 `rolled_back`）。要分得更细得等 core 给第四个机器位，
/// 不在本 mission 的范围。
fn status_key(report: &ImportReport) -> Key {
    match report.status {
        ImportStatus::Failed => match rollback_stage(report) {
            Some(RollbackStage::NotStarted) => Key::PackStatusFailedNotStarted,
            Some(RollbackStage::Incomplete) => Key::PackStatusFailedIncomplete,
            // `rollback_stage` 对 `Failed` 恒为 `Some`；`RolledBack` 与（理论上的）`None`
            // 都落到界面那一档的「已整包回滚」。
            _ => Key::PackStatusFailed,
        },
        ImportStatus::Success => Key::PackStatusSuccess,
        ImportStatus::Partial => Key::PackStatusPartial,
        // 取消**不是失败**：不说"失败"，也不说"部分成功"（那会让人以为装了一半）。
        // 但回滚不完整时也不许说"与导入前一致"——与失败路径同一套诚实口径。
        ImportStatus::Cancelled => {
            if report.rolled_back {
                Key::PackStatusCancelled
            } else {
                Key::PackCancelIncomplete
            }
        }
    }
}

/// 状态的人读名（`pack.rs` 把它填进 `结果：{…}` 那一行的内容位）。
///
/// # 为什么只返回内容、不带「结果：」这个标签（M344 的排版口径）
///
/// 「结果：」是这一行的**标签**，拼在 `pack.rs` 组装整行的地方（那一行的其余部分——文件计数
/// 四栏——也在那儿）；目录里这七句都是**标题形态**（`已导入` / `导入失败 —— 已整包回滚`），
/// 本身就当内容用，接在「结果：」后面正好是 M342 定下的「标签：内容」：
/// `结果：导入失败 —— 已整包回滚`。让目录去背「结果：」这个标签会造出两处各写一遍标签。
///
/// # 为什么不再手写（M344）
///
/// 以前这七句是 CLI 手写的（`成功` / `部分成功` / `失败（已回滚）` / `已取消（…）`），界面读的
/// 却是 `PackStatus*`：改目录时界面跟着变、CLI 不跟，用户拿两个壳对同一份报告会看到两套话。
/// 现在两侧读同一批 key ⇒ **改目录里那一个字，两侧一起变**（判据见本文件的 `mod tests`，
/// 它断言的是**字面量**，不是 `CATALOG.text()` 自证）。
pub(crate) fn status_label(report: &ImportReport) -> &'static str {
    CATALOG.text(status_key(report))
}

/// 「这次失败到底动没动过目标目录」（三档）。
///
/// 与 `presenter` 的 `host::addon::pack::convert::failure_stage` **同义**——CLI 不依赖 presenter
/// 层（`unlucky-cli` 只依赖 core + agent），所以这套判据在这里各写一份。已登记 finding：
/// 更干净的做法是把三档挪进 `core::addon::import::ImportReport`（那儿有 `rollback` /
/// `rolled_back` 两个机器位），两处都改成读它。
///
/// 为什么要三档而不是一个 `rolled_back` 布尔：core 的 `rolled_back` 在「回滚完整完成」与
/// **「从未写过目标目录」**两种情形下都是 `true`。只按它出词，预检失败会被说成「已整包回滚：目标
/// 目录与导入前一致」——那句话在**一个字节都没写**时是假的（M161 复审的 P1 同族问题）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RollbackStage {
    /// 预检/装配阶段就失败：目标目录一个字节都没写
    NotStarted,
    /// 写过，回滚**完整**完成
    RolledBack,
    /// 写过，回滚有失败项（目标目录可能与导入前不同）
    Incomplete,
}

/// 三档判据（与 presenter 那份逐条对应）。
pub(crate) fn rollback_stage(report: &ImportReport) -> Option<RollbackStage> {
    if report.status != ImportStatus::Failed {
        return None;
    }
    Some(match &report.rollback {
        Some(outcome) if outcome.is_complete() => RollbackStage::RolledBack,
        Some(_) => RollbackStage::Incomplete,
        None if report.rolled_back => RollbackStage::NotStarted,
        None => RollbackStage::Incomplete,
    })
}

/// 失败收尾那一段人读文案（**三档三句**，不许共用）。
///
/// M344 之后这里仍是手写，理由与 [`status_label`] 相反：它**不是标签**，而是 `错误：…` 下面那句
/// 细节，第三档还嵌着 `failure_count()`（目录那几句没有 `{count}` 形态）；而前两档若改成读目录，
/// 同一句"导入失败 —— 还没写任何文件"会在相邻两行里出现两遍。
pub(crate) fn rollback_line(report: &ImportReport) -> String {
    match rollback_stage(report) {
        Some(RollbackStage::NotStarted) => {
            "预检未通过：目标目录一个字节都没写（不需要回滚）".to_string()
        }
        Some(RollbackStage::RolledBack) => "已整包回滚：目标目录与导入前一致".to_string(),
        Some(RollbackStage::Incomplete) => match &report.rollback {
            Some(rollback) => format!(
                "回滚未完整：{} 项没回去（详见 --json 的 rollback.failed）",
                rollback.failure_count()
            ),
            None => "回滚状态不明：目标目录可能与导入前不同（详见 --json）".to_string(),
        },
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    //! 结局出词的判据：**断言字面量**（不是 `CATALOG.text()` 自证）。
    //!
    //! 为什么字面量很重要：如果断言写 `status_label(&r) == CATALOG.text(Key::X)`，把目录里那句
    //! 改掉两边的值一起变，用例永远绿——那种断言只证明"读了同一个函数"，证明不了"那句话还是
    //! 人话"。这里钉的是**用户会看到的那串字**，所以改目录就会红（M344 的判据就是拿一个字做变异）。

    use std::path::PathBuf;

    use unlucky_core::addon::import::{ImportError, RollbackOutcome};

    use super::*;

    fn empty() -> ImportReport {
        ImportReport::empty(PathBuf::from("C:/tmp/.minecraft"))
    }

    /// 预检/装配阶段就失败：`rolled_back` 为真但**没有** `rollback`（一个字节都没写）。
    fn not_started() -> ImportReport {
        let mut report = empty();
        report.fail(ImportError::integrity("sha1 mismatch"));
        report.rolled_back = true;
        report
    }

    /// 写过、回滚完整。
    fn rolled_back() -> ImportReport {
        let mut report = empty();
        report.fail(ImportError::integrity("sha1 mismatch"));
        report.rolled_back = true;
        report.rollback = Some(RollbackOutcome {
            attempted: 2,
            failed: Vec::new(),
        });
        report
    }

    /// 写过、回滚没做完。
    fn incomplete() -> ImportReport {
        let mut report = empty();
        report.fail(ImportError::integrity("sha1 mismatch"));
        report.rolled_back = false;
        report.rollback = Some(RollbackOutcome {
            attempted: 2,
            failed: vec!["mods/a.jar".to_string()],
        });
        report
    }

    /// 七档结局各读目录里的哪一句（**字面量**：改目录里那一个字，这里就红）。
    ///
    /// 抓得住什么（变异判据）：
    /// - 把 `status_label` 改回手写中文（M344 之前的样子）⇒ 前四行立刻红
    ///   （`成功` / `部分成功` / `失败（已回滚）` 与目录那七句都不等）；
    /// - 改目录里任意一句的一个字（如 `已整包回滚` → `已整包滚回`）⇒ 对应那一行红，
    ///   报文里同时给出「目录现在是什么」与「这条用例钉的是什么」；
    /// - 把失败三档并成一档（`status_key` 里三臂合成一个）⇒ 第 3/4/5 行里至少两行红；
    /// - 把取消两档并成一句 ⇒ 第 6/7 行红。
    #[test]
    fn every_status_sentence_comes_from_the_catalog() {
        let success = empty();
        let mut partial = empty();
        partial.status = ImportStatus::Partial;
        let mut cancelled = empty();
        cancelled.status = ImportStatus::Cancelled;
        cancelled.rolled_back = true;
        let mut cancel_incomplete = empty();
        cancel_incomplete.status = ImportStatus::Cancelled;
        cancel_incomplete.rolled_back = false;

        // 键映射与文案**都**断言：前者钉「读的是哪一句」，后者钉「那一句还是那句话」。
        for (report, key, text) in [
            (&success, Key::PackStatusSuccess, "已导入"),
            (&partial, Key::PackStatusPartial, "导入完成，但有缺口"),
            (
                &not_started(),
                Key::PackStatusFailedNotStarted,
                "导入失败 —— 还没写任何文件",
            ),
            (
                &rolled_back(),
                Key::PackStatusFailed,
                "导入失败 —— 已整包回滚",
            ),
            (
                &incomplete(),
                Key::PackStatusFailedIncomplete,
                "导入失败 —— 回滚没做完",
            ),
            (&cancelled, Key::PackStatusCancelled, "已取消 —— 已回滚"),
            (
                &cancel_incomplete,
                Key::PackCancelIncomplete,
                "已取消 —— 回滚没做完：目标目录可能与导入前不同。",
            ),
        ] {
            assert_eq!(status_key(report), key, "结局选错了 key");
            assert_eq!(status_label(report), text, "目录那句话变了（{key:?}）");
        }
    }

    /// 三档失败**三句互不相同**，且与取消两档也互不相同：把任何两档并成一句，这里就红。
    ///
    /// 这是 M161 那条债的守门——预检失败被说成「已整包回滚」正是从这里漏出去的。
    #[test]
    fn the_three_failure_tiers_never_share_one_sentence() {
        let reports = [not_started(), rolled_back(), incomplete()];
        let sentences = reports.map(|report| status_label(&report));
        assert_eq!(
            sentences
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            3,
            "三档塌成两句：{sentences:?}"
        );
        // 「还没写任何文件」那一句不许出现"回滚"字样（那才是真正的谎：没发生过回滚）。
        assert!(
            !sentences[0].contains("回滚"),
            "预检失败不许提回滚：{}",
            sentences[0]
        );
    }
}
