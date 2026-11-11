//! 版本比较的边界用例：数值比而非字典序、预发布 vs 正式、构建元数据不参与排序。

use std::cmp::Ordering;

use crate::update::{Stage, Version};

fn version(text: &str) -> Version {
    Version::parse(text).expect("测试里的版本号应可解析")
}

#[test]
fn minor_numbers_compare_numerically() {
    // 字典序会把 "0.9.0" 判成比 "0.10.0" 新，这正是这条用例要钉死的行为。
    assert!(version("0.10.0") > version("0.9.0"));
    assert!(version("0.10.1") < version("0.11.0"));
    assert!(version("1.0.0") > version("0.999.999"));
    assert!(version("2.0.0") > version("1.99.99"));
}

#[test]
fn prerelease_sorts_below_the_release() {
    assert!(version("1.0.0-rc.1") < version("1.0.0"));
    assert!(version("1.0.0-alpha") < version("1.0.0"));
    // 预发布阶段之间：alpha < beta < rc（ASCII 序，不是特例表）
    assert!(version("1.0.0-alpha") < version("1.0.0-beta"));
    assert!(version("1.0.0-beta.2") < version("1.0.0-rc.1"));
    // 主次修订号优先于预发布标识
    assert!(version("1.0.1-alpha.1") > version("1.0.0"));
}

#[test]
fn prerelease_identifiers_follow_semver_rules() {
    assert!(version("1.0.0-alpha.1") < version("1.0.0-alpha.2"));
    assert!(version("1.0.0-alpha.2") < version("1.0.0-alpha.10"));
    assert!(version("1.0.0-alpha") < version("1.0.0-alpha.1"));
    // 纯数字段小于任何非数字段
    assert!(version("1.0.0-1") < version("1.0.0-alpha"));
    // 非数字段按 ASCII 比
    assert!(version("1.0.0-alpha-1") < version("1.0.0-alpha-x"));
}

#[test]
fn build_metadata_is_ignored_by_ordering_and_equality() {
    let with = version("1.2.3+build.7");
    let without = version("1.2.3");
    assert_eq!(with.cmp(&without), Ordering::Equal);
    assert_eq!(with, without);
    assert!(version("1.2.4+zzz") > version("1.2.3+aaa"));
    assert_eq!(with.to_string(), "1.2.3+build.7");
    assert_eq!(with.build_metadata(), Some("build.7"));
    assert_eq!(without.build_metadata(), None);
}

#[test]
fn stages_are_recognised_from_the_first_identifier() {
    assert_eq!(version("1.0.0").stage(), Stage::Stable);
    assert_eq!(version("1.0.0-alpha.3").stage(), Stage::Alpha);
    assert_eq!(version("1.0.0-a.2").stage(), Stage::Alpha);
    assert_eq!(version("1.0.0-beta.1").stage(), Stage::Beta);
    assert_eq!(version("1.0.0-rc.4").stage(), Stage::ReleaseCandidate);
    assert_eq!(version("1.0.0-preview.2").stage(), Stage::ReleaseCandidate);
    assert_eq!(version("1.0.0-spike-1").stage(), Stage::Unknown);
    assert_eq!(version("1.0.0-7").stage(), Stage::Unknown);
}

#[test]
fn round_trips_through_display() {
    for text in [
        "0.2.1",
        "1.0.0-alpha.1",
        "3.17.0+build-9",
        "10.0.12-rc.2+ci.9",
    ] {
        let parsed = version(text);
        let rendered = parsed.to_string();
        assert_eq!(version(&rendered), parsed, "{text} 回读不一致");
    }
}

#[test]
fn accepts_v_prefix_and_reports_prerelease() {
    assert_eq!(version("v1.4.2"), version("1.4.2"));
    assert_eq!(version("V1.4.2"), version("1.4.2"));
    assert!(!version("1.4.2").is_prerelease());
    assert!(version("1.4.2-rc.1").is_prerelease());
    assert_eq!(version("1.4.2-rc.1").prerelease().as_deref(), Some("rc.1"));
    assert_eq!(version("1.4.2").prerelease(), None);
    assert_eq!(
        (
            version("1.4.2").major(),
            version("1.4.2").minor(),
            version("1.4.2").patch()
        ),
        (1, 4, 2)
    );
}

#[test]
fn rejects_non_semver_input() {
    for text in [
        "",
        "1",
        "1.2",
        "1.2.3.4",
        "01.2.3",
        "1.02.3",
        "1.2.03",
        "1.2.3-",
        "1.2.3-alpha..1",
        "1.2.3+",
        "1.2.3-é",
        "1.-2.3",
        "1.2.x",
        "1.2.3-01",
    ] {
        assert!(
            Version::parse(text).is_err(),
            "`{text}` 不该被当成合法版本号"
        );
    }
}

#[test]
fn version_error_carries_input_and_reason() {
    let error = Version::parse("1.2.x").expect_err("应失败");
    assert_eq!(error.input(), "1.2.x");
    assert!(!error.reason().is_empty());
    assert!(error.to_string().contains("1.2.x"));
}

#[test]
fn new_builds_a_plain_release() {
    assert_eq!(Version::new(0, 2, 1), version("0.2.1"));
    assert!(!Version::new(0, 2, 1).is_prerelease());
}
