//! `plan` / `inspect` 的**只读**判据与拒绝理由：零写盘、缺父/不存在/非法目标的报错形状。
//!
//! 与 `register_cases` 的分工：那边验「登记写出了什么」，这边验「还没登记时算出了什么、拒绝得对不对」。

use std::path::PathBuf;

use unlucky_core::game::adopt::{self, AdoptError, AdoptTarget};
use unlucky_core::game::install::instance::Isolation;

use crate::fixture::{
    assert_unchanged, foreign_minecraft, request, snapshot, snapshot_opt, TempRoot,
};

/// 缺省档位本身就是「数据区独立目录」，而且**不是** `isolated`（判据的单一真相源）。
///
/// 抓得住：把 `#[default]` 挪到 `VersionDir`/`Shared`（第一条断言红），或让缺省档映射到
/// `Isolated`（第二条断言红）——后者正是「导入别人的版本后第一次启动就在他的 `versions/<id>/`
/// 里长 mods/saves」的那个坑。
/// 抓不住：CLI 是否真的用 `AdoptTarget::default()`（那条由 cli/adopt_test 的行为断言钉着）。
#[test]
fn the_default_target_is_an_own_dir_not_the_global_isolated_mode() {
    let target = AdoptTarget::default();
    assert_eq!(target, AdoptTarget::InstanceDir);
    assert_eq!(target.as_str(), "instance_dir");
    assert_eq!(target.isolation(), Isolation::Custom);
    assert_ne!(target.isolation(), Isolation::Isolated);
}

/// `--isolation custom --game-dir <相对路径>` ⇒ 明确报错（不猜、不悄悄改成绝对路径）。
///
/// 抓得住：相对路径被默默接受（`TargetInvalid` 变成成功）。
/// 抓不住：Windows 上 `C:foo` 这种「带盘符但非绝对」的形状，只看 `is_absolute`。
#[test]
fn custom_target_requires_an_absolute_directory() {
    let root = TempRoot::new("custom-rel");
    foreign_minecraft(&root.game_root());
    let error = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::Custom(PathBuf::from("relative/dir"))),
    )
    .expect_err("相对路径应被拒");
    assert_eq!(error.code(), "adopt_target_invalid");
    assert!(error.to_string().contains("绝对路径"), "{error}");
}

/// `--dry-run` 的 core 形态（`plan`）：一个文件都不写、目录也不建。
///
/// 抓得住：`plan` 顺手建了运行目录或写了档案（两种断言都会红）。
/// 抓不住：CLI 的 `--dry-run` 是否真的只调 `plan`（那条在 cli 侧用例里）。
#[test]
fn plan_writes_nothing_at_all() {
    let root = TempRoot::new("plan");
    foreign_minecraft(&root.game_root());
    let before = snapshot(root.path());

    let plan = adopt::plan(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect("计划应成功");
    let after = snapshot(root.path());
    assert_unchanged(&before, &after, "plan 之后（含 .ULL）");

    assert!(plan.candidate.adoptable());
    assert!(!plan.run_dir.exists(), "plan 不该建目录");
    // `.ULL` 连**空目录**都不许出现（存在性用 `snapshot_opt`，与「空树」区分开）。
    assert!(
        snapshot_opt(&root.path().join(".ULL")).is_none(),
        "plan 不该建配置目录（连空的也不行）"
    );
}

/// 不可登记的版本（缺父）⇒ 一条都不登记，问题清单原样带出。
///
/// 抓得住：缺父的版本被登记（启动必失败却显示「导入成功」）、问题被折叠成一句空话。
/// 抓不住：用户在原来的启动器里修好之后重试（那条由重试路径自然覆盖）。
#[test]
fn not_adoptable_versions_are_refused_with_their_reasons() {
    let root = TempRoot::new("refuse");
    foreign_minecraft(&root.game_root());
    let error = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1-orphan", AdoptTarget::InstanceDir),
    )
    .expect_err("缺父版本应被拒");
    assert_eq!(error.code(), "adopt_not_adoptable");
    let codes: Vec<&str> = error
        .problems()
        .iter()
        .map(|problem| problem.code())
        .collect();
    assert_eq!(codes, vec!["missing_parent"]);
    assert!(
        !root.path().join(".ULL/config/profiles.json").exists(),
        "拒绝时不许留档案"
    );
}

/// 不存在的版本 ⇒ `adopt_version_not_found`（不是「帮你在网上下一个」）。
///
/// 抓得住：不存在的 id 被当成可登记（档案指向空版本）。
/// 抓不住：`versions/` 整个不存在的情形（上一条扫描用例覆盖）。
#[test]
fn an_unknown_version_is_reported_as_not_found() {
    let root = TempRoot::new("unknown");
    foreign_minecraft(&root.game_root());
    let error = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("9.9.9", AdoptTarget::InstanceDir),
    )
    .expect_err("不存在的版本应报错");
    assert!(matches!(error, AdoptError::UnknownVersion { .. }));
    assert_eq!(error.code(), "adopt_version_not_found");
}
