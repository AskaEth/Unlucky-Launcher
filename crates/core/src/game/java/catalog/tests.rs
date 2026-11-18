//! `catalog` 的单测：目录、版本判据、拒绝文案、兜底闸。
//!
//! 从 `catalog.rs` 拆出来只为守 400 行硬线（M280 r1 要在这里加「兜底闸」与「清单被截断」
//! 的用例，留在主文件会到 470+）；**纯搬移**，既有断言一字未改。

use super::{
    available_versions_text, choices, default_major, find, matches_version, vendors_for,
    version_mismatch, version_pin_error, VersionListScope, BASELINE_MAJOR,
};
use crate::game::java::vendor::{ids, JdkVendor};

/// 目录必须覆盖用户点名的三档（8 / 17 / 21），并钉住基准版本。
///
/// 变异证明：从 `CHOICES` 里删掉 8 那一项 ⇒ 本用例红（`find(8)` 为 `None`）。
#[test]
fn the_catalog_covers_eight_seventeen_and_twenty_one() {
    for major in [8, 17, 21] {
        assert!(find(major).is_some(), "目录里必须有 Java {major}");
    }
    assert_eq!(BASELINE_MAJOR, 17, "D13 的基准版本是 17");
    assert!(find(BASELINE_MAJOR).expect("基准版本在目录里").baseline);
    assert_eq!(
        choices().len(),
        5,
        "目录是 8/11/17/21/25 五档，改动要同步本断言与 UI 文案"
    );
    assert!(
        choices()
            .windows(2)
            .all(|pair| pair[0].major < pair[1].major),
        "目录必须升序（UI 直接按这个顺序画）"
    );
}

/// 「8 只能由 Temurin / Zulu 承接」：Oracle 从 9 起、MS 已停发 8。
///
/// 变异证明：把 `vendors_for` 里的 `supports` 过滤删掉 ⇒ 本用例红（8 会列出 oracle/microsoft）。
#[test]
fn java_eight_is_only_served_by_the_vendors_that_still_ship_it() {
    assert_eq!(ids(&vendors_for(8, &[], true)), vec!["temurin", "zulu"]);
    assert_eq!(
        ids(&vendors_for(21, &[], true)),
        vec!["temurin", "oracle", "microsoft", "zulu"],
        "21 四家都在（国内默认序）"
    );
    assert!(
        !vendors_for(8, &[], true).contains(&JdkVendor::Microsoft),
        "微软已停发 8"
    );
}

/// 用户优先级要真的排在最前面（与 `install` 的链同源）。
#[test]
fn the_user_priority_leads_the_vendor_list() {
    let priority = vec![JdkVendor::Zulu];
    assert_eq!(ids(&vendors_for(17, &priority, true))[0], "zulu");
    assert_eq!(ids(&vendors_for(17, &priority, true)).len(), 4);
}

/// 「该装哪个」对齐到目录里的一档：向上取整，没有上下文给基准，超出目录给基准。
#[test]
fn the_default_major_rounds_up_into_the_catalog() {
    assert_eq!(default_major(None), 17);
    assert_eq!(default_major(Some(8)), 8, "1.12.2 → 8");
    assert_eq!(default_major(Some(16)), 17, "1.17 要 16 → 目录里的 17");
    assert_eq!(default_major(Some(17)), 17);
    assert_eq!(default_major(Some(21)), 21, "1.20.5+ → 21");
    assert_eq!(default_major(Some(25)), 25, "26.1 → 25");
    assert_eq!(default_major(Some(99)), 17, "超出目录时退回基准");
}

/// 四家的版本串用同一把尺子量（`matches_version` 是唯一判据）。
///
/// 变异证明：把 `matches_version` 换成 `starts_with` ⇒ 本用例红
/// （`21.0.13` 会命中 MS 的 `21.0.13.0.2`，那是另一个版本）。
#[test]
fn version_matching_is_shared_and_never_prefix_fuzzy() {
    // Temurin：公共部分 ↔ 带构建号；semver 与 release 名两种写法都算同一份。
    assert!(matches_version("21.0.13", "21.0.13+9"));
    assert!(matches_version("21.0.13+9", "jdk-21.0.13+9"));
    assert!(matches_version("21.0.13+9", "21.0.13+9.0.LTS"));
    assert!(matches_version(" 21.0.13 ", "21.0.13"));
    // Oracle：目录名就是三段，公共部分一致即命中。
    assert!(matches_version("21.0.2", "21.0.2"));
    // 少一位、多一段都不是同一个版本（不许前缀近似）。
    assert!(!matches_version("21.0", "21.0.13"));
    assert!(!matches_version("21.0.13", "21.0.13.0.2"));
    assert!(!matches_version("21.0.13+8", "21.0.13+9"));
    // 用户点名了构建号、候选没给 → 不算命中。
    assert!(!matches_version("21.0.13+9", "21.0.13"));
    assert!(!matches_version("", "21.0.13"));
}

/// 可用版本列表：给几条就列几条，多了收口（错误文案不能无限长）。
#[test]
fn available_versions_text_caps_the_list() {
    let versions: Vec<String> = ["21.0.13", "21.0.12", "21.0.11"]
        .iter()
        .map(|text| (*text).to_string())
        .collect();
    assert_eq!(
        available_versions_text(&versions, 10, VersionListScope::Complete),
        "21.0.13 / 21.0.12 / 21.0.11"
    );
    assert_eq!(
        available_versions_text(&versions, 2, VersionListScope::Complete),
        "21.0.13 / 21.0.12 等 3 个"
    );
    assert!(available_versions_text(&[], 5, VersionListScope::Complete).contains("一个都没列出来"));
}

/// 四家的「这家不能按你要的版本装」文案必须**互不相同**、各自说清原因，且给下一步。
///
/// 变异证明：把某两家共用同一句「不支持指定版本」⇒ 本用例红。
#[test]
fn pin_refusals_are_distinct_and_each_explains_itself() {
    let snapshot = vec!["21.0.13.0.2".to_string()];
    let listed = vec!["21.0.13+9".to_string()];
    let errors = [
        version_pin_error(
            JdkVendor::Temurin,
            21,
            "21.0.5",
            &listed,
            VersionListScope::Complete,
        ),
        version_pin_error(
            JdkVendor::Oracle,
            21,
            "21.0.5",
            &["21.0.2".to_string()],
            VersionListScope::Complete,
        ),
        version_pin_error(
            JdkVendor::Microsoft,
            21,
            "21.0.5",
            &snapshot,
            VersionListScope::Complete,
        ),
        version_pin_error(
            JdkVendor::Zulu,
            21,
            "21.0.5",
            &[],
            VersionListScope::Complete,
        ),
        version_pin_error(
            JdkVendor::Termux,
            21,
            "21.0.5",
            &[],
            VersionListScope::Complete,
        ),
    ];
    for (index, left) in errors.iter().enumerate() {
        for right in errors.iter().skip(index + 1) {
            assert_ne!(
                left.message(),
                right.message(),
                "每家的拒绝理由必须各说各的"
            );
        }
        assert!(
            left.message().contains("21.0.5"),
            "错误里要带上用户要的版本"
        );
    }
    assert!(
        errors[0].message().contains("21.0.13+9"),
        "可用版本要列出来"
    );
    // P3-3：连接词由 `source` 自带，模板不再补空格 —— 两种拼法都必须读得通。
    // （正向断言已蕴含「没有多余空格」，所以不留反向断言：r3 复审 P3-2 判它冗余。）
    assert!(
        errors[0].message().contains("Adoptium 里可用"),
        "{}",
        errors[0].message()
    );
    assert!(
        errors[1].message().contains("Oracle 镜像目录里可用"),
        "{}",
        errors[1].message()
    );
    assert!(errors[2].message().contains("只内置了 21.0.13.0.2"));
    assert!(errors[2]
        .hint()
        .unwrap_or_default()
        .contains("--vendor microsoft"));
    assert!(errors[3].message().contains("latest=true"));
    assert!(errors[3]
        .hint()
        .unwrap_or_default()
        .contains("--vendor temurin"));
    assert!(errors[4].message().contains("系统包"));
}

/// **兜底闸（P2-2 守卫 2）**：实际拿到的版本与请求不符时必须给出原因，相符/没钉版本时必须放行。
///
/// 变异证明：把 `version_mismatch` 改成恒 `None`（复审的变异 A）⇒ 本用例第一条红。
#[test]
fn the_version_gate_rejects_a_different_version_and_lets_everything_else_through() {
    let reason = version_mismatch(Some("21.0.12"), "21.0.13+9")
        .expect("要 21.0.12、拿到 21.0.13+9 ⇒ 必须拒绝");
    assert!(reason.contains("21.0.13+9"), "{reason}");
    assert!(reason.contains("不是请求的 21.0.12"), "{reason}");

    // 同一个版本（公共部分一致、构建号一致）⇒ 放行。
    assert_eq!(version_mismatch(Some("21.0.12"), "21.0.12+8"), None);
    assert_eq!(version_mismatch(Some("21.0.12+8"), "jdk-21.0.12+8"), None);
    // 没钉版本 ⇒ 恒放行（不许对默认路径误报）。
    assert_eq!(version_mismatch(None, "21.0.13+9"), None);
    // 构建号不同 ⇒ 拒绝。
    assert!(version_mismatch(Some("21.0.12+8"), "21.0.12+9").is_some());
}

/// **清单被截断时不许把窗口当全集（P2-1 文案）**：`Complete` 说「等 N 个」，
/// `Truncated` 必须补一句「只查了前 N 条」。
///
/// 变异证明：把 `Truncated` 分支改成与 `Complete` 相同 ⇒ 本用例第二条红。
#[test]
fn a_truncated_list_never_pretends_to_be_the_whole_catalog() {
    let versions: Vec<String> = (0..20).map(|index| format!("17.0.{}", index + 1)).collect();
    let complete = available_versions_text(&versions, 2, VersionListScope::Complete);
    let truncated = available_versions_text(&versions, 2, VersionListScope::Truncated);
    assert_eq!(complete, "17.0.1 / 17.0.2 等 20 个");
    assert_ne!(complete, truncated);
    assert!(
        truncated.contains("只查了前 20 条"),
        "截断时必须如实说：{truncated}"
    );
    // 完整清单的拒绝文案里不许出现「只查了」。
    let error = version_pin_error(
        JdkVendor::Temurin,
        17,
        "17.0.99",
        &versions,
        VersionListScope::Complete,
    );
    assert!(!error.message().contains("只查了"), "{}", error.message());
}
