//! 版本清单（`version_manifest_v2.json`）：解析、分类、排序、与补充清单合并。

use unlucky_core::game::error::GameError;
use unlucky_core::game::version::{ReleaseType, RemoteVersion, VersionCategory, VersionList};

const MANIFEST_V2: &str = r#"{
  "latest": { "release": "1.20.1", "snapshot": "23w31a" },
  "versions": [
    { "id": "1.20.1", "type": "release", "url": "https://example.invalid/1.20.1.json",
      "time": "2023-06-12T13:25:51+00:00", "releaseTime": "2023-06-12T13:25:51+00:00",
      "sha1": "aaa", "size": 1000 },
    { "id": "23w31a", "type": "snapshot", "url": "https://example.invalid/23w31a.json",
      "time": "2023-08-02T12:00:00+00:00", "releaseTime": "2023-08-02T12:00:00+00:00",
      "sha1": "bbb", "size": 900 },
    { "id": "b1.7.3", "type": "old_beta", "url": "https://example.invalid/b1.7.3.json",
      "time": "2011-07-08T00:00:00+00:00", "releaseTime": "2011-07-08T00:00:00+00:00" }
  ]
}"#;

fn manifest() -> VersionList {
    VersionList::from_json_str(MANIFEST_V2).expect("清单应可解析")
}

#[test]
fn parses_manifest_v2_with_checksums() {
    let list = manifest();
    assert_eq!(list.len(), 3);

    let latest = list.latest.as_ref().expect("latest 应解析");
    assert_eq!(latest.release.as_deref(), Some("1.20.1"));
    assert_eq!(latest.snapshot.as_deref(), Some("23w31a"));

    let release = list.find("1.20.1").expect("1.20.1 应存在");
    assert!(release.is_verifiable());
    assert_eq!(release.size, Some(1000));
    // 内嵌旧清单没有 sha1/size（v1 形态）。
    assert!(!list.find("b1.7.3").expect("b1.7.3 应存在").is_verifiable());
}

#[test]
fn classifies_release_types() {
    assert_eq!(
        ReleaseType::from_json_str("release").category(),
        VersionCategory::Release
    );
    assert_eq!(
        ReleaseType::from_json_str("old_beta").category(),
        VersionCategory::Old
    );
    assert_eq!(
        ReleaseType::from_json_str("old_alpha").category(),
        VersionCategory::Old
    );
    assert_eq!(
        ReleaseType::from_json_str("pending").category(),
        VersionCategory::Pending
    );
    assert_eq!(
        ReleaseType::from_json_str("unobfuscated").category(),
        VersionCategory::Unobfuscated
    );
    assert_eq!(
        ReleaseType::from_json_str("未见过").category(),
        VersionCategory::Uncategorized
    );
    // 序列化回原始字符串，保证能原样写回清单。
    assert_eq!(ReleaseType::OldBeta.as_json_str(), "old_beta");
    assert_eq!(ReleaseType::Unknown.as_json_str(), "unknown");
}

#[test]
fn sorts_newest_first() {
    let mut list = manifest();
    list.sort_newest_first();
    let ids: Vec<&str> = list.versions.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, vec!["23w31a", "1.20.1", "b1.7.3"]);
}

#[test]
fn filters_by_ui_category() {
    let list = manifest();
    assert_eq!(list.filter_category(VersionCategory::Release).len(), 1);
    assert_eq!(list.filter_category(VersionCategory::Snapshot).len(), 1);
    assert_eq!(list.filter_category(VersionCategory::Old).len(), 1);
    assert!(list.filter_category(VersionCategory::Pending).is_empty());
}

#[test]
fn merging_keeps_existing_entries() {
    let mut list = manifest();
    let added = list.merge_missing(vec![
        // 同 id 冲突：已有条目（官方）胜出，URL 不被覆盖。
        RemoteVersion {
            id: "1.20.1".to_owned(),
            version_type: ReleaseType::Release,
            url: "https://mirror.invalid/1.20.1.json".to_owned(),
            time: None,
            release_time: None,
            sha1: None,
            size: None,
        },
        RemoteVersion {
            id: "a1.1.1".to_owned(),
            version_type: ReleaseType::OldAlpha,
            url: "https://mirror.invalid/a1.1.1.json".to_owned(),
            time: None,
            release_time: None,
            sha1: None,
            size: None,
        },
    ]);

    assert_eq!(added, 1, "只应新增缺失条目");
    assert_eq!(list.len(), 4);
    assert_eq!(
        list.find("1.20.1").map(|v| v.url.as_str()),
        Some("https://example.invalid/1.20.1.json")
    );
    assert!(list.find("a1.1.1").is_some());
}

#[test]
fn roundtrips_manifest_json() {
    let list = manifest();
    let json = list.to_json_string().expect("应可序列化");
    let reparsed = VersionList::from_json_str(&json).expect("应可再解析");
    assert_eq!(reparsed, list);
}

#[test]
fn invalid_json_reports_context() {
    let error = VersionList::from_json_str("not json").expect_err("非法 JSON 应报错");
    match error {
        GameError::Json { context, .. } => assert_eq!(context, "version_manifest"),
        other => panic!("期望 Json 错误，得到 {other:?}"),
    }
}
