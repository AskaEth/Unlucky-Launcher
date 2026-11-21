//! MODS 区视图模型的用例（M312）：报告 → 那几句已经拼好的话 + 行上的「重复」标记。
//!
//! 这一层不碰盘、也不画图：它证明的是「快照里的报告真的变成了用户能看见的字」——
//! 报告本身（谁重复、缺什么前置、必需还是可选）由 core 的 `addon::mods::report` 的用例守，
//! 这里只守映射：某一桶被漏掉、徽标挂错了行、或者问题区在空报告时仍然占位。
//!
//! 空目录 / 读失败 / 在飞置灰那些 M224 的老口径不在这里重测（`pages/instances/tests/` 与
//! presenter 的 `state::mods` 各有一份）。

use unlucky_i18n::{Catalog, Lang};
use unlucky_presenter::dto::{
    ModCopyView, ModDuplicateView, ModEntryView, ModListView, ModPrerequisiteView, ModProblemView,
};
use unlucky_presenter::state::ModsState;

use super::*;

fn entry(file: &str, enabled: bool) -> ModEntryView {
    ModEntryView {
        file: file.to_string(),
        name: file.to_string(),
        enabled,
        bytes: 1024,
        old: false,
    }
}

fn prerequisite(id: &str, installed: Option<&str>) -> ModPrerequisiteView {
    ModPrerequisiteView {
        id: id.to_string(),
        range: Some(">=1.0".to_string()),
        installed_version: installed.map(str::to_string),
        required_by: vec!["iris.jar".to_string()],
    }
}

/// 一份带两类问题的快照（顺序与 host 给的一致：新的在前）。
fn state(problems: ModProblemView) -> ModsState {
    // `ModsState::bind` 是 presenter 内部的（`pub(super)`）：这里直接摆好绑定与快照
    // （`changed` 只认「快照的实例 == 当前绑定」）。
    let mut state = ModsState {
        bound: Some("inst".to_string()),
        ..ModsState::default()
    };
    state.changed(ModListView {
        instance: "inst".to_string(),
        dir: "C:/mc/mods".to_string(),
        exists: true,
        entries: vec![
            entry("sodium-0.5.11.jar", true),
            entry("sodium-0.5.8.jar", true),
            entry("iris.jar", true),
        ],
        skipped: Vec::new(),
        running: false,
        known: true,
        problems,
    });
    state
}

fn problems() -> ModProblemView {
    ModProblemView {
        duplicates: vec![ModDuplicateView {
            id: "sodium".to_string(),
            display: "Sodium".to_string(),
            keep: "sodium-0.5.11.jar".to_string(),
            keep_version: Some("0.5.11".to_string()),
            copies: vec![
                ModCopyView {
                    file: "sodium-0.5.11.jar".to_string(),
                    version: Some("0.5.11".to_string()),
                    enabled: true,
                },
                ModCopyView {
                    file: "sodium-0.5.8.jar".to_string(),
                    version: Some("0.5.8".to_string()),
                    enabled: true,
                },
            ],
        }],
        missing: vec![prerequisite("indium", None)],
        unverifiable: vec![prerequisite("fabric-api", Some("0.92.2"))],
        optional: vec![prerequisite("sodium-extra", None)],
        opaque: 1,
    }
}

/// 报告 → 两段问题 + 三句提示；徽标只在真有问题时出现。
#[test]
fn the_report_becomes_sentences_and_a_badge() {
    let mods = from_state(&state(problems()), Catalog::of(Lang::ZhCn));

    assert_eq!(mods.problems.count, 2, "重复 1 组 + 必需缺失 1 项");
    assert_eq!(mods.problem_badge.as_deref(), Some("2 个问题"));
    assert!(!mods.problems.missing_heading.is_empty());

    let duplicate = &mods.problems.duplicates[0];
    assert!(
        duplicate.text.starts_with("Sodium · "),
        "{}",
        duplicate.text
    );
    assert!(
        duplicate.text.contains("sodium-0.5.11.jar"),
        "建议保留的必须是版本新的那一份：{}",
        duplicate.text
    );
    assert!(
        duplicate.text.contains("0.5.11"),
        "版本要跟着出现：{}",
        duplicate.text
    );

    let missing = &mods.problems.missing[0];
    assert_eq!(missing.text, "indium", "缺失前置的 id 是运行时数据，不翻译");
    assert!(missing.detail.contains("iris.jar"), "{}", missing.detail);

    // 三句提示：判不了 / 可选没装 / 读不出的文件。它们**不是问题**，所以计数里没有它们。
    assert_eq!(mods.problems.notes.len(), 3, "{:?}", mods.problems.notes);
    assert!(mods.problems.notes.iter().all(|note| !note.is_empty()));
}

/// 列表里那一行的副文案：「重复 · 建议保留 / 建议禁用」只挂在**启用中**的副本上。
#[test]
fn duplicate_copies_are_marked_on_their_rows() {
    let mods = from_state(&state(problems()), Catalog::of(Lang::ZhCn));
    let detail = |file: &str| {
        mods.items
            .iter()
            .find(|item| item.file == file)
            .unwrap_or_else(|| panic!("{file} 不在列表里"))
            .detail
            .clone()
    };
    assert!(
        detail("sodium-0.5.11.jar").contains("建议保留"),
        "{}",
        detail("sodium-0.5.11.jar")
    );
    assert!(
        detail("sodium-0.5.8.jar").contains("建议禁用"),
        "{}",
        detail("sodium-0.5.8.jar")
    );
    assert!(
        !detail("iris.jar").contains("重复"),
        "不重复的行不该挂标记：{}",
        detail("iris.jar")
    );
}

/// 空报告：没有徽标、没有问题区（正常实例一张卡不该多出任何一行）。
#[test]
fn a_clean_snapshot_draws_no_problem_block() {
    let mods = from_state(&state(ModProblemView::default()), Catalog::of(Lang::ZhCn));
    assert!(mods.problems.is_empty());
    assert!(mods.problem_badge.is_none());
    assert!(mods.problems.duplicates.is_empty() && mods.problems.missing.is_empty());
    // 「去模组库找」的字案一直在（按钮只在有缺失时画），空报告时不该被当成有内容。
    assert!(!mods.missing_cta.is_empty());
}
