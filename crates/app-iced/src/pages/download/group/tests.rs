//! 分组规则的判据：**用真实版本样本**钉住（不用「第几个点」这种相对说法，踩坑 T1）。
//!
//! 每条都写清抓得住什么 / 抓不住什么。

use super::*;
use unlucky_presenter::dto::{AncientKind, VersionKind};

const RELEASE: VersionKind = VersionKind::Release;
const SNAPSHOT: VersionKind = VersionKind::Snapshot;

fn ancient(kind: AncientKind) -> VersionKind {
    VersionKind::Ancient(kind)
}

/// 四种形态**同时存在**时各自归到哪个桶（这是模块文档那张表的产品化版本）。
///
/// 抓得住：有人改回「按第一个 `.` 切」——`1.21.4` 会变成 `1`、`a1.2.6` 会变成一个假主版本。
/// 抓不住：真实清单里还没出现的形态（比如将来的 `1.22.0`，它走同一条数值规则所以不会红）。
#[test]
fn the_four_shapes_land_in_the_documented_buckets() {
    // 1. 正式版
    assert_eq!(key("1.21.4", RELEASE).as_deref(), Some("1.21"));
    assert_eq!(key("1.9.4", RELEASE).as_deref(), Some("1.9"));
    assert_eq!(key("1.7.10", RELEASE).as_deref(), Some("1.7"));
    // 2. 预发布：切掉 `-pre3` 之后与它的正式版同一个桶
    assert_eq!(key("1.21.9-pre3", SNAPSHOT).as_deref(), Some("1.21"));
    assert_eq!(key("1.21-rc1", SNAPSHOT).as_deref(), Some("1.21"));
    assert_eq!(
        key("1.21.9-pre3", SNAPSHOT),
        key("1.21.4", RELEASE),
        "预发布必须和它的正式版同桶"
    );
    // 3. 周更快照：年桶
    assert_eq!(key("24w14a", SNAPSHOT).as_deref(), Some("24w"));
    assert_eq!(key("14w02a", SNAPSHOT).as_deref(), Some("14w"));
    // 4. 远古版：没有主版本
    assert_eq!(key("rd-132211", ancient(AncientKind::Classic)), None);
    assert_eq!(key("a1.2.6", ancient(AncientKind::OldAlpha)), None);
    assert_eq!(key("c0.30_01c", ancient(AncientKind::Classic)), None);
    assert_eq!(key("b1.7.3", ancient(AncientKind::OldBeta)), None);
}

/// 「按第一个 `.` 切」的坏法要留下痕迹：同一族的版本不许被拆成两个桶。
#[test]
fn splitting_on_the_first_dot_would_break_these_samples() {
    // 1.7 / 1.9 / 1.21 是三个不同的桶（按第一个点切会全归到 "1"）
    let buckets = ["1.7.10", "1.9.4", "1.21.4", "1.21.1"]
        .map(|id| key(id, RELEASE).expect("正式版都有主版本"))
        .to_vec();
    assert_eq!(buckets, vec!["1.7", "1.9", "1.21", "1.21"]);
    assert_ne!(buckets[0], buckets[2]);
}

/// 版本序：`1.21 > 1.9`（字典序会说反），周更年桶同理。
#[test]
fn the_version_order_is_numeric_not_lexicographic() {
    use std::cmp::Ordering;
    assert_eq!(compare("1.21", "1.9"), Ordering::Greater);
    assert_eq!(compare("1.9", "1.21"), Ordering::Less);
    assert_eq!(compare("24w", "14w"), Ordering::Greater);
    assert_eq!(compare("1.21", "1.21"), Ordering::Equal);

    let mut labels = vec!["1.9", "1.21", "24w", "1.7"];
    labels.sort_by(|left, right| compare(right, left));
    assert_eq!(labels, vec!["24w", "1.21", "1.9", "1.7"]);
}

/// 桶聚合：数量正确、按版本序降序、无主版本的桶排在最后。
#[test]
fn buckets_are_counted_and_sorted() {
    use unlucky_i18n::Lang;

    let catalog = Lang::En.catalog();
    let rows: Vec<super::super::model::VersionRow> = [
        ("1.21.9", RELEASE),
        ("1.21.8", RELEASE),
        ("1.20.4", RELEASE),
        ("24w14a", SNAPSHOT),
        ("rd-132211", ancient(AncientKind::Classic)),
    ]
    .into_iter()
    .map(|(id, kind)| {
        super::super::model::VersionRow::of(
            &unlucky_presenter::dto::VersionSummary {
                id: id.to_string(),
                kind,
                released: "2026-09-20".to_string(),
                installed: false,
            },
            catalog,
        )
    })
    .collect();

    let buckets = buckets(&rows);
    let keys: Vec<Option<&str>> = buckets.iter().map(|bucket| bucket.key.as_deref()).collect();
    assert_eq!(
        keys,
        vec![Some("24w"), Some("1.21"), Some("1.20"), None],
        "24w > 1.21 > 1.20 > 无主版本"
    );
    assert_eq!(buckets[1].count, 2, "1.21 桶里两个版本");
    assert_eq!(buckets[3].count, 1, "远古版落在 None 桶");
}
