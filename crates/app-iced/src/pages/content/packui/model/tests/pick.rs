//! 冲突清单（M160）的视图模型守门：分页、计数、三个入口 → 决策表。
//!
//! 这里钉的是**不画**的那部分——一屏只出 `PAGE_SIZE` 行、翻页只换数据、计数与最终决策同源。
//! 画法本身靠出图（`ULL_CONTENT_PACK=pick`）。
//!
//! **M341**：预检注记按级别分两段（`要处理` / `只是告知`），分档与导入报告**共用**一处实现
//! （`model/report_issue.rs::section`）；这里钉分档结果与「空注记不画段头」。

use std::collections::BTreeMap;

use unlucky_i18n::Lang;

use super::super::pick::{pick_of, PAGE_SIZE};
use super::super::PackPageState;
use super::catalog;
use unlucky_presenter::dto::{
    ConflictChoiceView, ConflictDecisionsView, ConflictReasonView, ConflictSourceView,
    ImportConflictView, ImportIssueView, ImportScanView,
};

/// `count` 条冲突的预检结果（**注记为空**：M341 的空态那一支用它）。
fn scan(count: usize) -> ImportScanView {
    scan_notes(count, Vec::new())
}

/// `count` 条冲突 + 给定的预检注记（每条带级别）。
fn scan_notes(count: usize, notes: Vec<ImportIssueView>) -> ImportScanView {
    ImportScanView {
        target: std::path::PathBuf::from("instance"),
        format: None,
        pack_name: Some("Better MC".to_string()),
        pack_version: None,
        game_version: None,
        conflicts: (0..count)
            .map(|index| ImportConflictView {
                path: format!("config/row-{index:03}.toml"),
                source: ConflictSourceView::Overrides,
                reason: ConflictReasonView::PreExisting,
                disabled: false,
                pack_sha1: "aa".repeat(20),
                pack_size: 1_000,
                disk_sha1: "bb".repeat(20),
                disk_size: 2_000,
                base_sha1: None,
            })
            .collect(),
        unknown_remote: 0,
        lost_bytes: 0,
        incoming_bytes: 0,
        notes,
        error: None,
    }
}

fn pick(
    scan: &ImportScanView,
    page: usize,
    per_item: &BTreeMap<String, ConflictChoiceView>,
    batch: bool,
) -> super::super::pick::Pick {
    pick_of(scan, page, per_item, batch, catalog()).expect("有冲突就该画这一块")
}

/// 中文那一档（段标题要按**界面上那两句**断言：`要处理` / `只是告知` 是 M339 入目录的原话）。
fn pick_zh(scan: &ImportScanView) -> super::super::pick::Pick {
    pick_of(scan, 0, &BTreeMap::new(), false, Lang::ZhCn.catalog()).expect("有冲突就该画这一块")
}

#[test]
fn only_the_current_page_is_drawn() {
    let scan = scan(312);
    let first = pick(&scan, 0, &BTreeMap::new(), false);
    assert_eq!(first.rows.len(), PAGE_SIZE, "一屏只许画一页的行");
    assert_eq!(first.pages, 8, "312 条 / 40 条一页 = 8 页");
    assert_eq!(first.page, 0);
    assert!(first.can_next, "还有下一页");
    assert!(!first.can_prev, "第一页没有上一页");
    assert_eq!(first.range, "Items 1-40 of 312");
    assert_eq!(first.rows[0].path, "config/row-000.toml");

    let second = pick(&scan, 1, &BTreeMap::new(), false);
    assert_eq!(second.page, 1);
    assert_eq!(
        second.rows[0].path,
        format!("config/row-{PAGE_SIZE:03}.toml")
    );
    assert_eq!(second.range, "Items 41-80 of 312");
    assert!(second.can_prev && second.can_next);
}

#[test]
fn the_last_page_is_short_and_the_page_number_is_clamped() {
    let scan = scan(312);
    let last = pick(&scan, 7, &BTreeMap::new(), false);
    assert_eq!(last.rows.len(), 312 - 7 * PAGE_SIZE, "最后一页只剩尾巴");
    assert_eq!(last.range, "Items 281-312 of 312");
    assert!(!last.can_next);

    // 页码越界（例如用户先在 300 条的清单上翻到第 6 页，再重扫一份 10 条的）⇒ 夹到最后一页。
    let clamped = pick(&scan, 99, &BTreeMap::new(), false);
    assert_eq!(clamped.page, 7);
}

#[test]
fn the_summary_counts_what_the_user_actually_picked() {
    let scan = scan(10);
    let keep_all = pick(&scan, 0, &BTreeMap::new(), false);
    assert_eq!(keep_all.keep_count, 10);
    assert_eq!(keep_all.overwrite_count, 0);
    assert!(keep_all.summary.contains("Keep 10"), "{}", keep_all.summary);
    assert!(keep_all.keep_all_active && !keep_all.overwrite_all_active);

    let mut per_item = BTreeMap::new();
    per_item.insert(
        "config/row-001.toml".to_string(),
        ConflictChoiceView::Overwrite,
    );
    per_item.insert(
        "config/row-004.toml".to_string(),
        ConflictChoiceView::Overwrite,
    );
    let mixed = pick(&scan, 0, &per_item, false);
    assert_eq!(mixed.overwrite_count, 2);
    assert_eq!(mixed.keep_count, 8);
    // 丢弃量只算"真选了覆盖"的那两条（各 2 000 字节）。
    assert!(
        mixed.summary.contains("Overwrite 2") || mixed.summary.contains("overwrite 2"),
        "{}",
        mixed.summary
    );
    assert_eq!(
        mixed.rows[1].choice,
        ConflictChoiceView::Overwrite,
        "点过的那一行要显示成覆盖"
    );
    assert_eq!(mixed.rows[0].choice, ConflictChoiceView::KeepUser);

    let all = pick(&scan, 0, &BTreeMap::new(), true);
    assert_eq!(all.overwrite_count, 10);
    assert!(all.overwrite_all_active && !all.keep_all_active);
}

#[test]
fn no_conflict_means_no_block() {
    let scan = scan(0);
    assert!(
        pick_of(&scan, 0, &BTreeMap::new(), false, catalog()).is_none(),
        "没有冲突就不该画这一屏（不打扰用户）"
    );
}

#[test]
fn decisions_default_to_keep_all() {
    let page = PackPageState::default();
    assert_eq!(page.decisions(None), ConflictDecisionsView::KeepAll);
    // 逐项点一个"覆盖" ⇒ 逐项表里只有它，缺省档仍是保留。
    let mut page = PackPageState::default();
    page.pick_choices
        .insert("mods/a.jar".to_string(), ConflictChoiceView::Overwrite);
    let decisions = page.decisions(Some(&scan(3)));
    assert_eq!(decisions.default_choice(), ConflictChoiceView::KeepUser);
    assert_eq!(decisions.overwrite_count(), 1);
    assert_eq!(decisions.len(), 1);
}

#[test]
fn batch_overwrite_plus_a_kept_file_is_expressible() {
    // 全部覆盖之后单独放回一条：缺省档是覆盖、逐项里那条是保留。
    let mut page = PackPageState {
        pick_overwrite_all: true,
        ..PackPageState::default()
    };
    assert_eq!(
        page.decisions(Some(&scan(3))),
        ConflictDecisionsView::OverwriteAll,
        "只点整批覆盖时不需要逐项表"
    );
    page.pick_choices
        .insert("mods/pin.jar".to_string(), ConflictChoiceView::KeepUser);
    let decisions = page.decisions(Some(&scan(3)));
    assert_eq!(decisions.default_choice(), ConflictChoiceView::Overwrite);
    assert_eq!(decisions.keep_count(), 1);
    assert_eq!(decisions.overwrite_count(), 0);
}

#[test]
fn toggling_flips_the_row_and_clearing_the_batch_drops_per_item_choices() {
    let mut page = PackPageState::default();
    let path = "mods/a.jar";
    assert_eq!(page.pick_choice(path), ConflictChoiceView::KeepUser);
    page.pick_choices
        .insert(path.to_string(), ConflictChoiceView::Overwrite);
    assert_eq!(page.pick_choice(path), ConflictChoiceView::Overwrite);

    // 整批覆盖之后，那条逐项"保留"仍然优先于整批档。
    page.pick_overwrite_all = true;
    page.pick_choices
        .insert(path.to_string(), ConflictChoiceView::KeepUser);
    assert_eq!(page.pick_choice(path), ConflictChoiceView::KeepUser);
    assert_eq!(
        page.pick_choice("mods/other.jar"),
        ConflictChoiceView::Overwrite
    );
}

/// **M341 判据①**：预检注记按 **core 给的级别**分成两段，段标题与导入报告同一套
/// （M339 入目录的 `PackReportWarnings` / `PackReportNotes`）。
///
/// 走产品路径：喂一份带级别的 `ImportScanView` 给 `pick_of`（`Pack::of` 调的就是这一处），
/// 不手搓视图模型。夹具一条 `Warning` + 一条 `Note`。
///
/// **变异②（抓得住）**：把 `model/report_issue.rs` 的 `filter(|issue| issue.level == level)`
/// 改成两档共用一段（或把 `pick_of` 里传的 `ImportIssueLevelView::Warning` / `Note` 对调）
/// ⇒ 两段的标题计数与逐条对不上、`warnings`/`notes` 互换 ⇒ 本条红。
/// 抓不住：core 在构造点把某条 `ImportIssue` 判错档（跨 crate，那条由
/// `unlucky-core` 的 `addon::import::report::issue::tests` 钉住）。
#[test]
fn the_pre_scan_notes_are_split_by_level() {
    let scan = scan_notes(
        3,
        vec![
            ImportIssueView::warning("基线读不出来，本次按「没有基线」处理"),
            ImportIssueView::note("有 4 条远端条目预检判不了"),
        ],
    );
    let pick = pick_zh(&scan);
    // ① 告警档那条落在「要处理」段，计数按告警档算（不是 2 条）。
    assert_eq!(pick.warnings_title.as_deref(), Some("要处理：1 条"));
    assert_eq!(
        pick.warnings,
        vec!["基线读不出来，本次按「没有基线」处理".to_string()],
        "「要处理」段只收 `Warning` 档"
    );
    // ② 注记档那条落在「只是告知」段。
    assert_eq!(pick.notes_title.as_deref(), Some("只是告知：1 条"));
    assert_eq!(
        pick.notes,
        vec!["有 4 条远端条目预检判不了".to_string()],
        "「只是告知」段只收 `Note` 档"
    );

    // 只有一档时另一档整段不出现（段头是**按档**出的，不是两段一起出）。
    let only_note = pick_zh(&scan_notes(
        1,
        vec![ImportIssueView::note("有 4 条远端条目预检判不了")],
    ));
    assert_eq!(only_note.warnings_title, None, "没有告警就不许画空段头");
    assert_eq!(only_note.notes_title.as_deref(), Some("只是告知：1 条"));

    // 两档各自保留 core 给的顺序（同一档里多条按原序）。
    let ordered = pick_zh(&scan_notes(
        1,
        vec![
            ImportIssueView::warning("第一条告警"),
            ImportIssueView::warning("第二条告警"),
        ],
    ));
    assert_eq!(ordered.warnings_title.as_deref(), Some("要处理：2 条"));
    assert_eq!(
        ordered.warnings,
        vec!["第一条告警".to_string(), "第二条告警".to_string()]
    );
}

/// **M341 判据③**：`notes` 为空 ⇒ **两个段头都不出现**（本仓「空值不画标签」那条旧例）。
///
/// 抓得住：`pick_of` 里把段标题写成 `Some(render(..))`（不管有没有条目）⇒ 本条红；
/// 画法那边（`view/pick.rs`）也跟着 `None` 走，空段头连控件都不进树
/// （结构级证据在 `pages/content/packui/tests/m341.rs`）。
#[test]
fn no_notes_means_no_section_titles() {
    let pick = pick_zh(&scan(3));
    assert_eq!(pick.warnings_title, None, "空注记不许画空段头");
    assert_eq!(pick.notes_title, None, "空注记不许画空段头");
    assert!(pick.warnings.is_empty() && pick.notes.is_empty());
}
