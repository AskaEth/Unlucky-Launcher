//! D23 新增的两个设置键：`java.installRoot` / `java.vendorPriority`。
//!
//! 三件事要说清：
//! 1. 两个键都登记在 launcher schema 里（`ull config get` 能看到、AI 白名单受策略约束）；
//! 2. `java.vendorPriority` 的允许值集合与 `game::java::vendor::VENDOR_IDS` **同源**
//!    （setting 层不依赖 game 层，靠这条契约测试固定，避免两处漂移）；
//! 3. schema 版本推到 1.2.0 但 major 仍为 1 → 不需要迁移步骤（默认值不落盘）。

use serde_json::json;
use unlucky_core::game::java::vendor::VENDOR_IDS;
use unlucky_core::setting::keys::{
    launcher_schema, launcher_version, JAVA_INSTALL_ROOT_UNSET, JDK_VENDORS,
};
use unlucky_core::setting::schema::{SchemaId, SchemaRef, SchemaVersion};
use unlucky_core::setting::store::SettingStore;

#[test]
fn both_d23_keys_are_registered_with_their_defaults() {
    let schema = launcher_schema();
    assert_eq!(
        schema.default_value("java.installRoot"),
        Some(&json!(JAVA_INSTALL_ROOT_UNSET))
    );
    assert_eq!(
        schema.default_value("java.vendorPriority"),
        Some(&json!([]))
    );
    // 空串/空数组都有明确语义：installRoot 空 = 按数据目录模式派生；
    // vendorPriority 空 = 按地区默认序（国内 Temurin→Oracle→MS→Zulu）。
    assert!(schema.spec("java.installRoot").is_some());
    assert!(schema.spec("java.vendorPriority").is_some());
}

#[test]
fn vendor_priority_whitelist_matches_the_vendor_module() {
    assert_eq!(JDK_VENDORS, VENDOR_IDS, "两处必须同一份集合");
    let schema = launcher_schema();
    assert!(schema
        .validate("java.vendorPriority", &json!(["oracle"]))
        .is_ok());
    assert!(schema
        .validate("java.vendorPriority", &json!(["temurin", "zulu"]))
        .is_ok());
    assert!(schema
        .validate("java.vendorPriority", &json!(["graalvm"]))
        .is_err());
    assert!(schema
        .validate("java.vendorPriority", &json!([1, 2]))
        .is_err());
    assert!(schema
        .validate("java.vendorPriority", &json!("microsoft"))
        .is_err());
}

#[test]
fn install_root_accepts_a_path_and_keeps_empty_as_unset() {
    let schema = launcher_schema();
    assert!(schema
        .validate("java.installRoot", &json!("D:/jdk"))
        .is_ok());
    // 空串就是默认值，语义是「没设覆盖键」（不是错误）：所以类型是 text 而不是 path
    // （Path 类型不接受空串，而这里需要一个合法的「未设置」值）。
    assert!(schema.validate("java.installRoot", &json!("")).is_ok());
    assert!(schema.validate("java.installRoot", &json!(5)).is_err());
}

#[test]
fn schema_version_bumps_minor_only_so_no_migration_step_is_needed() {
    // 1.6.0 = M161 的自更新两键（此前 1.5.0 是 M116 窗口圆角）。
    assert_eq!(launcher_version(), SchemaVersion::new(1, 6, 0));
    assert_eq!(
        launcher_version().major,
        1,
        "major 不变 → 迁移链（按 major 走）不需要新步骤"
    );
}

#[test]
fn an_old_config_file_without_the_new_keys_still_loads() {
    let dir = std::env::temp_dir().join(format!(
        "ull-java-keys-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    ));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let path = dir.join("launcher.json");
    // 1.1.0 的文件（真实 $schema 前缀）：没有 java.installRoot / java.vendorPriority，
    // 且 core.data_mode 故意写非默认值 standard —— 证明值是「读出来的」不是「默认值撞上的」。
    let old_url = SchemaRef::new(SchemaId::LAUNCHER, SchemaVersion::new(1, 1, 0)).url();
    std::fs::write(
        &path,
        format!(r#"{{"$schema":"{old_url}","schemaVersion":1,"core":{{"data_mode":"standard"}}}}"#),
    )
    .expect("写旧配置");

    let store = SettingStore::load(&path, launcher_schema());
    assert_eq!(
        store.doc().effective_str("core.data_mode"),
        Some("standard"),
        "旧文件的值必须照读（不是碰巧等于默认值）"
    );
    // 缺键取默认值，不报错、不写盘（「默认值不落盘」）。
    assert_eq!(
        store.doc().effective_str("java.installRoot"),
        Some(JAVA_INSTALL_ROOT_UNSET)
    );
    assert_eq!(
        store.effective("java.vendorPriority"),
        Some(&json!([])),
        "缺键 → 空数组 → 按地区默认序"
    );
    assert!(
        store.problems().is_empty(),
        "只缺新键不算问题：{:?}",
        store.problems()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
