//! M352：`ImportIssue` 构造点的**级别守门**（合成夹具，不碰真包、不联网）。
//!
//! # 这个 target 要回答的问题
//!
//! 看板那条短板是「分级有 13 处『改反不会红』」：那些构造点要么只有 `message_key` 被断言
//! （把 `warning` 改成 `note`，所有用例照样绿），要么根本没走到。于是「这条该不该要用户动手」
//! 这件事**没有任何用例守**——级别是产品判断，判错了用户就会读错这份报告（要么被一堆假告警
//! 烦到忽略真的，要么真的该动手时没看见）。
//!
//! 这里的每条断言都是三件套：**级别 + key + 那句话里认得出来的一截**。依据写在每条断言的
//! `why` 上，全部形如「照这份报告做决定会不会出错」，不是「代码里写着 `Warning`」。
//!
//! # 为什么是合成夹具
//!
//! 真包太贵（要下载、要联网、还都塞不进 CI），而这里要验的是**级别**而不是包解析。所以夹具
//! 手搓到刚好触发那条注记（见 [`fixtures`] 的模块文档），走的是产品路径（`import_pack` /
//! `scan_pack`），不是把 `ImportIssue` 直接拼出来。
//!
//! # 反过来也验过
//!
//! 每条断言都做过一次「改反就该红」的重放：把那个构造点的 `note` ↔ `warning` 对调，跑这条
//! 用例看它红，然后复原。重放报文见 mission 报告（M352）。

mod fixtures;
mod pipeline_cases;
mod plan_cases;

use unlucky_core::addon::import::report::{ImportIssue, ImportIssueLevel};
use unlucky_core::addon::import::ImportReport;

/// 报告里**所有**注记：`warnings` 加逐文件那条 `note`（同一族数据两个出口，级别都在
/// [`ImportIssue`] 自己身上）。
fn issues(report: &ImportReport) -> Vec<&ImportIssue> {
    let mut all: Vec<&ImportIssue> = report.warnings.iter().collect();
    all.extend(report.files.iter().filter_map(|file| file.note.as_ref()));
    all
}

fn keys_of(issues: &[&ImportIssue]) -> String {
    let keys: Vec<&str> = issues.iter().map(|issue| issue.message_key).collect();
    format!("{keys:?}")
}

/// 报告里 key 恰一条的那条注记（多一条也算错：界面与命令行的分档按条数算）。
fn find<'a>(issues: &[&'a ImportIssue], key: &str) -> &'a ImportIssue {
    let mut found = issues
        .iter()
        .copied()
        .filter(|issue| issue.message_key == key);
    let issue = found
        .next()
        .unwrap_or_else(|| panic!("报告里没有 {key}：现有 {}", keys_of(issues)));
    assert!(
        found.next().is_none(),
        "{key} 只该有一条：{}",
        keys_of(issues)
    );
    issue
}

/// 一条注记的三件套断言：**级别 + key + 句子**。
///
/// `why` 必须是「照这份报告做决定会不会出错」这一层的理由。写成「代码里写着 `Warning`」的
/// 断言恒真——那正是这一轮要消灭的东西。
fn assert_issue(
    issues: &[&ImportIssue],
    key: &str,
    level: ImportIssueLevel,
    needle: &str,
    why: &str,
) {
    let issue = find(issues, key);
    assert_eq!(
        issue.level, level,
        "{key} 的级别判错了。依据：{why}（现判 {:?}）\n句子：{}",
        issue.level, issue.message
    );
    assert!(
        issue.message.contains(needle),
        "{key} 的句子变了（这条断言靠这截认出它来）：期望含 {needle:?}\n实际：{}",
        issue.message
    );
}
