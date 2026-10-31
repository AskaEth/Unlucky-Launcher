//! 实例清单：稳定 UUID、增删查、落盘格式兼容。

use unlucky_core::game::profiles::{
    InstanceProfile, ProfileId, ProfileList, PROFILE_SCHEMA_VERSION,
};

#[test]
fn generated_ids_are_unique_and_uuid_shaped() {
    let ids: Vec<ProfileId> = (0..256).map(|_| ProfileId::generate()).collect();
    for id in &ids {
        assert!(id.is_uuid_shaped(), "id 形状不对: {id}");
    }
    let mut unique: Vec<&str> = ids.iter().map(ProfileId::as_str).collect();
    let total = unique.len();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), total, "同一批生成的 id 不能重复");
}

#[test]
fn id_shape_validation_rejects_junk() {
    assert!(!ProfileId::new("not-a-uuid").is_uuid_shaped());
    assert!(!ProfileId::new("").is_uuid_shaped());
    assert!(ProfileId::new("7c9e6679-7425-40de-944b-e07fc1f90ae7").is_uuid_shaped());
}

#[test]
fn upsert_and_remove_keep_one_entry_per_id() {
    let mut list = ProfileList::default();
    assert_eq!(list.schema_version, PROFILE_SCHEMA_VERSION);
    assert!(list.is_empty());

    let mut profile = InstanceProfile::new("主世界", "1.20.1");
    assert_eq!(list.upsert(profile.clone()), None, "首次插入没有旧值");
    assert_eq!(list.len(), 1);

    profile.name = "主世界（Fabric）".to_owned();
    let replaced = list.upsert(profile.clone()).expect("同 id 应替换");
    assert_eq!(replaced.name, "主世界");
    assert_eq!(list.len(), 1);
    assert_eq!(
        list.get(&profile.id).map(|p| p.name.as_str()),
        Some("主世界（Fabric）")
    );

    let removed = list.remove(&profile.id).expect("应可删除");
    assert_eq!(removed.id, profile.id);
    assert!(list.is_empty());
    assert!(list.remove(&profile.id).is_none());
}

#[test]
fn profiles_group_by_version() {
    let mut list = ProfileList::default();
    list.upsert(InstanceProfile::new("a", "1.20.1"));
    list.upsert(InstanceProfile::new("b", "1.20.1"));
    list.upsert(InstanceProfile::new("c", "1.19.4"));

    assert_eq!(list.for_version("1.20.1").len(), 2);
    assert_eq!(list.for_version("1.19.4").len(), 1);
    assert!(list.for_version("1.18.2").is_empty());
}

#[test]
fn roundtrips_profiles_json() {
    let mut list = ProfileList::default();
    let mut profile = InstanceProfile::new("整合包", "1.20.1-forge-47.2.0");
    profile.running_dir = Some(std::path::PathBuf::from("D:/mc/runs/pack"));
    profile
        .extra
        .insert("thirdPartyFlag".to_owned(), serde_json::json!(true));
    list.upsert(profile.clone());

    let json = list.to_json_string().expect("应可序列化");
    let reparsed = ProfileList::from_json_str(&json).expect("应可再解析");
    assert_eq!(reparsed, list);
    assert_eq!(reparsed.schema_version, PROFILE_SCHEMA_VERSION);
}

#[test]
fn reads_minimal_profile_file() {
    // 用户手改/旧版本写下的最小档案：只有三个必填语义字段，其它缺省。
    let list = ProfileList::from_json_str(
        r#"{ "profiles": [ { "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
                             "name": "旧档案", "version_id": "1.12.2" } ] }"#,
    )
    .expect("最小档案应可解析");

    assert_eq!(
        list.schema_version, PROFILE_SCHEMA_VERSION,
        "schemaVersion 缺省补齐"
    );
    assert_eq!(list.len(), 1);
    let profile = &list.profiles[0];
    assert_eq!(profile.name, "旧档案");
    assert!(profile.running_dir.is_none());
    assert!(profile.id.is_uuid_shaped());
}

#[test]
fn invalid_profiles_json_reports_context() {
    let error = ProfileList::from_json_str("{").expect_err("非法 JSON 应报错");
    match error {
        unlucky_core::game::error::GameError::Json { context, .. } => {
            assert_eq!(context, "profiles")
        }
        other => panic!("期望 Json 错误，得到 {other:?}"),
    }
}
