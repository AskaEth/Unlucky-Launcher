//! 判据用例：重复 / 必需缺失 / 无法判定 / 可选（M312 · MODDUP）。
//!
//! 这些用例的输入全是 [`ModRecord`] 字面量——**没有盘、没有 jar**。所以它们同时钉住一件事：
//! 判定真的从 `ModRecord` 出发（那份记录以前全仓零消费者）。把
//! `inspect::inspect` 里的 `report::report(&records)` 断掉，`mods/tests/moddup.rs` 的管道用例会红；
//! 把 `report` 的输入换掉，这一族的每一条都会红。

use super::*;
use crate::diagnose::collectors::{ModDependency, ModRecord};

fn record(id: &str, version: Option<&str>, file: &str) -> ModRecord {
    ModRecord {
        id: id.to_string(),
        name: Some(id.to_string()),
        version: version.map(str::to_string),
        file: Some(file.to_string()),
        enabled: true,
        deps: Vec::new(),
    }
}

fn disabled(mut record: ModRecord) -> ModRecord {
    record.enabled = false;
    record
}

fn with_deps(mut record: ModRecord, deps: Vec<ModDependency>) -> ModRecord {
    record.deps = deps;
    record
}

/// 同一个 modid 两个版本 ⇒ 一条重复，建议保留**版本新的**那一份，且两份都在 `copies` 里。
#[test]
fn duplicate_keeps_the_newest_and_lists_every_copy() {
    let records = [
        record("sodium", Some("0.5.8"), "sodium-0.5.8.jar"),
        record("sodium", Some("0.5.11"), "sodium-0.5.11.jar"),
    ];
    let report = report(&records);
    assert_eq!(report.duplicates.len(), 1, "{report:?}");
    let group = &report.duplicates[0];
    assert_eq!(group.id, "sodium");
    assert_eq!(group.keep, "sodium-0.5.11.jar", "`0.5.11` 比 `0.5.8` 新");
    assert_eq!(group.keep_version.as_deref(), Some("0.5.11"));
    assert_eq!(
        group.copies.len(),
        2,
        "重复组要列出全部副本（含建议保留的那份）"
    );
    assert_eq!(group.copies[0].file, "sodium-0.5.11.jar", "新的排最前");
    assert_eq!(report.problem_count(), 1);
    assert!(!report.is_clean());
}

/// **同 id 同版本不算重复**（两份一模一样的文件只是拷了两遍）。
#[test]
fn identical_copies_are_not_duplicates() {
    let records = [
        record("jei", Some("15.2.0.27"), "jei-15.2.0.27.jar"),
        record("jei", Some("15.2.0.27"), "jei-15.2.0.27 (1).jar"),
    ];
    let report = report(&records);
    assert!(report.duplicates.is_empty(), "{:?}", report.duplicates);
    assert!(report.is_clean());
}

/// 版本读不出的副本不参与重复判定（判据要的是**已知且不同**的两个版本）。
#[test]
fn unknown_versions_never_make_a_duplicate() {
    let records = [
        record("x", None, "x.jar"),
        record("x", Some("1.0"), "x-1.0.jar"),
        record("x", None, "x-old.jar"),
    ];
    assert!(report(&records).duplicates.is_empty());
}

/// `.old` 是旧版本残留（游戏不加载）⇒ 不算重复。
#[test]
fn old_leftovers_are_not_duplicates() {
    let records = [
        record("sodium", Some("0.5.11"), "sodium.jar"),
        record("sodium", Some("0.5.8"), "sodium.jar.old"),
    ];
    assert!(report(&records).duplicates.is_empty());
}

/// 必需缺失进问题报告，可选缺失只进提示桶（**这一条是「必需 vs 可选」的判据**）。
#[test]
fn required_missing_is_a_problem_and_optional_is_only_a_hint() {
    let records = [with_deps(
        record("sodium", Some("0.5.11"), "sodium.jar"),
        vec![
            ModDependency::required("fabric-api").with_range(">=0.92"),
            ModDependency::optional("sodium-extra"),
        ],
    )];
    let report = report(&records);
    assert_eq!(report.missing.len(), 1, "{report:?}");
    assert_eq!(report.missing[0].id, "fabric-api");
    assert_eq!(report.missing[0].range.as_deref(), Some(">=0.92"));
    assert_eq!(report.missing[0].required_by, vec!["sodium.jar"]);
    assert_eq!(report.optional.len(), 1, "可选缺失只能进提示桶");
    assert_eq!(report.optional[0].id, "sodium-extra");
    assert!(report.missing.iter().all(|item| item.id != "sodium-extra"));
    assert_eq!(report.problem_count(), 1, "可选缺失不该算问题");
}

/// 前置装着、没写版本区间（或写了通配）⇒ 满足，什么也不报。
#[test]
fn installed_prerequisite_without_a_range_is_satisfied() {
    let records = [
        with_deps(
            record("sodium", Some("0.5.11"), "sodium.jar"),
            vec![ModDependency::required("fabric-api")],
        ),
        record("fabric-api", Some("0.92.0"), "fabric-api-0.92.0.jar"),
    ];
    let report = report(&records);
    assert!(report.missing.is_empty());
    assert!(report.unverifiable.is_empty());
}

/// 装了但版本区间**判不了** ⇒ 进「无法判定」，**不许**报成缺失（误报会让用户去删没问题的 mod）。
#[test]
fn unjudgeable_range_is_unverifiable_not_missing() {
    let records = [
        with_deps(
            record("jei", Some("15.2.0.27"), "jei.jar"),
            vec![ModDependency::required("forge").with_range("[47,)")],
        ),
        with_deps(
            record("sodium", Some("0.5.11"), "sodium.jar"),
            vec![ModDependency::required("fabric-api").with_range("[1.0,2.0)")],
        ),
        record("fabric-api", Some("0.13.0"), "fabric-api-0.13.0.jar"),
    ];
    let report = report(&records);
    assert!(
        report.missing.is_empty(),
        "判不了不等于缺失：{:?}",
        report.missing
    );
    assert_eq!(report.unverifiable.len(), 1, "{report:?}");
    let item = &report.unverifiable[0];
    assert_eq!(item.id, "fabric-api");
    assert_eq!(item.range.as_deref(), Some("[1.0,2.0)"));
    assert_eq!(
        item.installed_version.as_deref(),
        Some("0.13.0"),
        "「无法判定」要带上盘上那份的版本，用户自己判"
    );
    assert!(report.is_clean(), "「无法判定」不算问题");
}

/// 平台/加载器自己提供的前置、以及自己依赖自己 —— 都不参与缺失判定。
#[test]
fn platform_and_self_dependencies_are_ignored() {
    let records = [with_deps(
        record("sodium", Some("0.5.11"), "sodium.jar"),
        vec![
            ModDependency::required("minecraft").with_range("~1.20"),
            ModDependency::required("java"),
            ModDependency::required("fabricloader"),
            ModDependency::required("sodium"),
        ],
    )];
    let report = report(&records);
    assert!(report.missing.is_empty(), "{:?}", report.missing);
}

/// 多个 mod 声明同一个缺失前置 ⇒ 合并成一条，`required_by` 排序去重。
#[test]
fn one_missing_prerequisite_lists_every_declarer() {
    let records = [
        with_deps(
            record("b-mod", Some("1.0"), "b.jar"),
            vec![ModDependency::required("fabric-api")],
        ),
        with_deps(
            record("a-mod", Some("2.0"), "a.jar"),
            vec![ModDependency::required("fabric-api")],
        ),
    ];
    let report = report(&records);
    assert_eq!(report.missing.len(), 1);
    assert_eq!(report.missing[0].required_by, vec!["a.jar", "b.jar"]);
}

/// 禁用中的副本仍然算「装着」（缺失判据只看盘上有没有，启停是另一件事）。
#[test]
fn disabled_copies_still_count_as_installed() {
    let records = [
        with_deps(
            record("sodium", Some("0.5.11"), "sodium.jar"),
            vec![ModDependency::required("fabric-api")],
        ),
        disabled(record(
            "fabric-api",
            Some("0.92.0"),
            "fabric-api-0.92.0.jar.disabled",
        )),
    ];
    assert!(report(&records).missing.is_empty());
}

/// 版本比较本身（重复组"建议保留"那一列靠它）。
#[test]
fn cmp_version_orders_mod_versions() {
    assert_eq!(cmp_version("0.5.11", "0.5.8"), std::cmp::Ordering::Greater);
    assert_eq!(cmp_version("1.0", "1.0.0"), std::cmp::Ordering::Equal);
}

/// 元数据读不出的那一份仍按**文件名**认得出 ⇒ 不许报「缺失」（宁可少报，不要误报）。
///
/// 场景：`fabric-api-0.92.2.jar` 是个坏 zip，id 只能从文件名猜成 `fabric-api-0`。
/// 精确匹配查不到它，早先的实现会因此把 `iris` 的必需前置报成"缺失"——用户会去装一个
/// 已经装着的 mod。文件名兜底之后这一条落到「无法判定」（判据仍是"不谎报缺失"）。
#[test]
fn a_prerequisite_whose_jar_has_no_metadata_is_not_missing() {
    let records = [
        with_deps(
            record("iris", Some("1.7.0"), "iris.jar"),
            vec![ModDependency::required("fabric-api").with_range("[0.92,)")],
        ),
        // 元数据读不出来：id 是文件名猜的，版本也只能从文件名猜。
        record("fabric-api-0", Some("0.92.2"), "fabric-api-0.92.2.jar"),
    ];
    let report = report(&records);
    assert!(
        report.missing.is_empty(),
        "装着的说没装：{:?}",
        report.missing
    );
    assert_eq!(report.unverifiable.len(), 1, "{report:?}");
    assert_eq!(report.unverifiable[0].id, "fabric-api");
}

/// 文件名兜底不许"粘"到别的 mod 上：`createaddition` / `sodium-extra` 都不是 `create` / `sodium`。
#[test]
fn the_file_name_fallback_needs_a_version_or_an_extension() {
    let records = [
        with_deps(
            record("mymod", Some("1.0"), "mymod.jar"),
            vec![
                ModDependency::required("create"),
                ModDependency::required("sodium"),
            ],
        ),
        record("createaddition", Some("1.2"), "createaddition-1.2.jar"),
        record("sodium-extra-0", Some("0.5"), "sodium-extra-0.5.jar"),
    ];
    let missing = report(&records)
        .missing
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(missing, ["create", "sodium"], "两个都不该被文件名兜底吃掉");
}
