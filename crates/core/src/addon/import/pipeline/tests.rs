//! 失败收尾的**口径判据**：同一份报告里的机器位与文案位不许互相打脸。
//!
//! 从 `pipeline.rs` 拆出来是为守 400 行硬线（与 `cancel_tests.rs` / `config/tests.rs` 同形）。

use super::*;
use crate::addon::import::report::{FileAction, FileSource, ImportIssueLevel, ImportedFile};
use crate::addon::import::ImportStatus;

fn base() -> ImportReport {
    ImportReport::empty(std::path::PathBuf::from("instance"))
}

fn messages(report: &ImportReport) -> Vec<&str> {
    report
        .warnings
        .iter()
        .map(|note| note.message.as_str())
        .collect()
}

fn incomplete() -> RollbackOutcome {
    RollbackOutcome {
        attempted: 3,
        failed: vec!["删除 config/a.toml 失败：拒绝访问".to_string()],
    }
}

fn file(path: &str, action: FileAction, bytes: u64) -> ImportedFile {
    ImportedFile {
        path: path.to_string(),
        source: FileSource::Overrides,
        sha1: None,
        bytes,
        disabled: false,
        action,
        note: None,
    }
}

#[test]
fn incomplete_rollback_never_claims_the_target_is_untouched() {
    // 复审 r6 第 1 条：机器位（rolled_back=false）诚实，文案位也必须诚实——同一份 warnings
    // 里不许同时出现"保持导入前的样子"与"有 N 项未能回滚"。
    let report = record_rollback(failed(base(), ImportError::fetch("提交失败")), incomplete());

    assert_eq!(report.status, ImportStatus::Failed);
    assert!(!report.rolled_back, "回滚不完整时不许宣称一致");
    assert_eq!(
        report.rollback.as_ref().map(RollbackOutcome::failure_count),
        Some(1)
    );
    let messages = messages(&report);
    assert!(
        messages
            .iter()
            .any(|line| line.contains("未能回滚") && line.contains("config/a.toml")),
        "失败项要逐条列出：{messages:?}"
    );
    assert!(
        !messages.iter().any(|line| line.contains("保持导入前的样子")
            || line.contains("未被写入")
            || line.contains("恢复为导入前的样子")),
        "回滚不完整却宣称一致（自相矛盾）：{messages:?}"
    );
}

#[test]
fn complete_rollback_claims_consistency_exactly_once() {
    let report = record_rollback(
        failed(base(), ImportError::fetch("提交失败")),
        RollbackOutcome {
            attempted: 2,
            failed: Vec::new(),
        },
    );

    assert!(report.rolled_back);
    assert_eq!(
        report.rollback.as_ref().map(RollbackOutcome::failure_count),
        Some(0)
    );
    let messages = messages(&report);
    assert!(
        messages
            .iter()
            .any(|line| line.contains("恢复为导入前的样子")),
        "{messages:?}"
    );
    assert!(
        !messages.iter().any(|line| line.contains("未能回滚")),
        "{messages:?}"
    );
}

#[test]
fn pre_commit_failure_claims_untouched_only_via_abandoned() {
    // `failed()` 本身不宣称任何目标目录状态；"从未写过目标目录"这条口径只由 `abandoned()` 给。
    let bare = failed(base(), ImportError::fetch("开包失败"));
    assert!(!bare.rolled_back);
    assert!(bare.rollback.is_none());
    assert!(!messages(&bare)
        .iter()
        .any(|line| line.contains("保持导入前的样子") || line.contains("未被写入")));

    let report = abandoned(base(), ImportError::fetch("开包失败"));
    assert!(report.rolled_back);
    assert!(report.rollback.is_none());
    let messages = messages(&report);
    assert!(
        messages.iter().any(|line| line.contains("未被写入")),
        "{messages:?}"
    );
}

/// M318 真包盘查的那份自相矛盾的报告：「失败（未写入）」+「文件 新13」（`bytes_written=4764`）。
///
/// 抓得住 `abandoned()` 里清账那两行：把 `files.retain(..)` 与 `bytes_written = 0` 删掉 ⇒
/// `installed()` 回到 2、「新 N」回到 2 ⇒ 本用例红。
#[test]
fn an_abandoned_import_does_not_claim_it_installed_anything() {
    let mut report = base();
    report.bytes_written = 4764;
    report
        .files
        .push(file("config/a.toml", FileAction::Installed, 1204));
    report
        .files
        .push(file("mods/b.jar", FileAction::Replaced, 3560));
    // 这两条描述的是目标目录**本来就有**的东西（读盘算过量摘要的既成事实），不是本次装了什么。
    report
        .files
        .push(file("mods/same.jar", FileAction::SkippedIdentical, 90));
    report
        .files
        .push(file("config/mine.toml", FileAction::KeptUser, 7));

    let report = abandoned(report, ImportError::fetch("死代理"));

    assert_eq!(report.status, ImportStatus::Failed);
    assert!(report.rolled_back);
    assert_eq!(report.installed(), 0, "一条都没落地，不许算「已装」");
    assert_eq!(report.bytes_written, 0, "那些字节写进的是临时目录");
    let left: Vec<&str> = report.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        left,
        vec!["mods/same.jar", "config/mine.toml"],
        "只许留下「磁盘上本来就这样」的那两条"
    );
    let messages = messages(&report);
    assert!(
        messages
            .iter()
            .any(|line| line.contains("未被写入") && line.contains("2 条文件")),
        "剔掉的那两条要在注记里说清去哪了：{messages:?}"
    );
}

/// 同族的另一半（M319 复审 P2-1）：写过又**完整回滚**，账本同样不许再算「已装」。
///
/// 调用点是提交阶段失败 / 登记失败（`pipeline.rs` 的两处 `record_rollback`）。抓得住
/// [`record_rollback`] 里清账那两行：删掉它们 ⇒ `installed()` 回 2、`bytes_written` 回 4764 ⇒
/// CLI 会打出「失败（已回滚）　文件 新13」。
#[test]
fn a_complete_rollback_does_not_keep_the_installed_ledger() {
    let mut report = base();
    report.bytes_written = 4764;
    report
        .files
        .push(file("config/a.toml", FileAction::Installed, 1204));
    report
        .files
        .push(file("mods/b.jar", FileAction::Replaced, 3560));
    report
        .files
        .push(file("mods/same.jar", FileAction::SkippedIdentical, 90));

    let report = record_rollback(
        failed(report, ImportError::fetch("提交失败")),
        RollbackOutcome {
            attempted: 3,
            failed: Vec::new(),
        },
    );

    assert_eq!(report.status, ImportStatus::Failed);
    assert!(report.rolled_back, "完整回滚才可以宣称一致");
    assert_eq!(report.installed(), 0, "回滚干净了，那 2 条不算落地");
    assert_eq!(report.bytes_written, 0);
    let left: Vec<&str> = report.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(left, vec!["mods/same.jar"], "只留没写过盘的那条");
    let messages = messages(&report);
    assert!(
        messages
            .iter()
            .any(|line| line.contains("已回滚 3 项") && line.contains("2 条已落地的文件")),
        "回滚了几项、几条不计入落地，都要说清：{messages:?}"
    );
}

/// 回滚**不完整**时反之：账本原样留着（目标目录里确实还有东西），没回去的逐条列出。
///
/// 抓得住「把清账搬到 `if` 外面」这种改法：那样这里 `installed()` 会变 0，与「有 1 项没回去」
/// 自相矛盾 ⇒ 本用例红。
#[test]
fn an_incomplete_rollback_keeps_the_ledger_it_could_not_undo() {
    let mut report = base();
    report.bytes_written = 1204;
    report
        .files
        .push(file("config/a.toml", FileAction::Installed, 1204));

    let report = record_rollback(failed(report, ImportError::fetch("提交失败")), incomplete());

    assert!(!report.rolled_back);
    assert_eq!(
        report.installed(),
        1,
        "回滚没做完时「还留着一条」才是真话：{:?}",
        report.files
    );
    assert_eq!(report.bytes_written, 1204);
    let messages = messages(&report);
    assert!(
        messages.iter().any(|line| line.contains("未能回滚")),
        "{messages:?}"
    );
    assert!(
        !messages.iter().any(|line| line.contains("已落地的文件")),
        "没回滚干净就不许说「不计入落地」：{messages:?}"
    );
}

/// 报告里那条 key 恰好一条的 issue（多一条也算错：级别分档按条数算）。
fn only_issue<'a>(report: &'a ImportReport, key: &str) -> &'a ImportIssue {
    let mut found = report
        .warnings
        .iter()
        .filter(|issue| issue.message_key == key);
    let issue = found
        .next()
        .unwrap_or_else(|| panic!("没有 {key} 这一条：{:?}", messages(report)));
    assert!(
        found.next().is_none(),
        "{key} 只该有一条：{:?}",
        messages(report)
    );
    issue
}

/// M352：回滚那三处构造点的**级别**（以前只有文案有用例，级别改反不会有任何用例红）。
///
/// 依据不是"代码里写着 `warning`"，而是「照这份报告做决定会不会出错」：
///
/// | 事实 | 用户要做什么 | 判成什么 |
/// |------|-------------|---------|
/// | 回滚**完整**（`ROLLBACK_DONE`）：目标目录回到导入前 | 重跑一次即可，没有残留要收拾 | 注记 |
/// | 回滚**不完整**（`NOTE_ROLLBACK_INCOMPLETE`）：可能留着半包内容 | 得自己去看 `rollback.failed`、收拾残留，否则会在"看着是新的、其实混着旧文件"的实例上启动 | 告警 |
/// | 从没写过目标目录（`abandoned` 的 `ROLLBACK_DONE`） | 同上第一行 | 注记 |
///
/// 与 `report/issue.rs` 里 `ImportReport::cancel` 那三条是**同一口径**（取消/失败的收尾不许
/// 各说一套），但那是另一处构造点，改这边不会连带改动那边——所以这里得单独钉。
///
/// 变异（两处都已实跑过）：把 `record_rollback` / `abandoned` 里 `ROLLBACK_DONE` 的 `note`
/// 改成 `warning` ⇒ 第 1、3 段红；把 `NOTE_ROLLBACK_INCOMPLETE` 的 `warning` 改成 `note` ⇒ 第 2 段红。
#[test]
fn rollback_issues_are_leveled_by_whether_the_user_has_to_act() {
    // ① 完整回滚 ⇒ 注记。
    let report = record_rollback(
        failed(base(), ImportError::fetch("提交失败")),
        RollbackOutcome {
            attempted: 2,
            failed: Vec::new(),
        },
    );
    let done = only_issue(&report, keys::ROLLBACK_DONE);
    assert_eq!(
        done.level,
        ImportIssueLevel::Note,
        "目标目录已恢复原样，用户没有残留要收拾 ⇒ 事实陈述：{}",
        done.message
    );
    assert!(
        done.message.contains("恢复为导入前的样子"),
        "{}",
        done.message
    );

    // ② 回滚没做完 ⇒ 告警（这一档判轻了，用户会在半包上启动）。
    let report = record_rollback(failed(base(), ImportError::fetch("提交失败")), incomplete());
    let broken = only_issue(&report, keys::NOTE_ROLLBACK_INCOMPLETE);
    assert_eq!(
        broken.level,
        ImportIssueLevel::Warning,
        "目标目录可能与导入前不同，用户必须自己收拾 ⇒ 告警：{}",
        broken.message
    );
    assert!(
        broken.message.contains("config/a.toml"),
        "{}",
        broken.message
    );

    // ③ 从未写过目标目录（`abandoned`）⇒ 注记。
    let report = abandoned(base(), ImportError::fetch("开包失败"));
    let untouched = only_issue(&report, keys::ROLLBACK_DONE);
    assert_eq!(
        untouched.level,
        ImportIssueLevel::Note,
        "一个字节都没写过目标目录 ⇒ 事实陈述：{}",
        untouched.message
    );
    assert!(
        untouched.message.contains("未被写入"),
        "{}",
        untouched.message
    );
}
