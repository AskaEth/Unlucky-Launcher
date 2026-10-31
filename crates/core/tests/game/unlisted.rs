//! 内嵌补充清单 `unlisted-versions.json`：条目数、分类分布、合并规则。
//!
//! 这些断言同时是**数据完整性守门**：内嵌数据换版本时如果条目数/分类崩了，
//! 这里先红，而不是等用户在旧版列表里少看到几十个版本。

use unlucky_core::game::unlisted::{self, EMBEDDED_VERSION_COUNT};
use unlucky_core::game::version::{ReleaseType, RemoteVersion, VersionCategory, VersionList};

#[test]
fn embedded_list_has_expected_shape() {
    let embedded = unlisted::versions();
    assert_eq!(embedded.len(), EMBEDDED_VERSION_COUNT);
    assert_eq!(
        embedded.versions.len(),
        unlisted::entries().len(),
        "借用视图与清单一致"
    );
    assert!(embedded.latest.is_none(), "补充清单没有 latest 对象");

    for entry in unlisted::entries() {
        assert!(!entry.id.is_empty());
        assert!(!entry.url.is_empty(), "每条都要有下载地址: {}", entry.id);
        assert_ne!(
            entry.version_type,
            ReleaseType::Unknown,
            "内嵌数据的 type 必须能归类: {}",
            entry.id
        );
    }
}

#[test]
fn embedded_categories_match_analysis() {
    // analyse/03 §2.3 实测口径：old_alpha 88 / old_beta 16 / snapshot 70 /
    // pending 20 / unobfuscated 11。
    let counts = |category: VersionCategory| {
        unlisted::entries()
            .iter()
            .filter(|v| v.version_type.category() == category)
            .count()
    };
    assert_eq!(counts(VersionCategory::Old), 88 + 16);
    assert_eq!(counts(VersionCategory::Snapshot), 70);
    assert_eq!(counts(VersionCategory::Pending), 20);
    assert_eq!(counts(VersionCategory::Unobfuscated), 11);
    assert_eq!(counts(VersionCategory::Release), 0);
}

#[test]
fn embedded_ids_are_unique() {
    let mut ids: Vec<&str> = unlisted::entries().iter().map(|v| v.id.as_str()).collect();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), total, "同 id 重复会让合并结果不确定");
}

#[test]
fn merge_adds_missing_entries_only() {
    let mut official = VersionList {
        latest: None,
        versions: vec![RemoteVersion {
            id: "a1.1.1".to_owned(),
            version_type: ReleaseType::OldAlpha,
            url: "https://example.invalid/official-a1.1.1.json".to_owned(),
            time: None,
            release_time: None,
            sha1: Some("official".to_owned()),
            size: Some(1),
        }],
    };

    let added = unlisted::merge_into(&mut official);
    assert_eq!(added, EMBEDDED_VERSION_COUNT - 1, "只补官方缺的条目");
    assert_eq!(official.len(), EMBEDDED_VERSION_COUNT);

    // 同 id 冲突时官方条目胜出（我们与 HMCL 的差异，见 unlisted 模块注释）。
    assert_eq!(
        official.find("a1.1.1").map(|v| v.url.as_str()),
        Some("https://example.invalid/official-a1.1.1.json")
    );
}

#[test]
fn merge_into_empty_list_is_identity() {
    let mut empty = VersionList::default();
    assert!(empty.is_empty());
    assert_eq!(unlisted::merge_into(&mut empty), EMBEDDED_VERSION_COUNT);
    assert_eq!(empty, *unlisted::versions());
}
