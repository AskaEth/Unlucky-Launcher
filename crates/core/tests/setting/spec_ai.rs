//! 键元数据：AI 可写白名单、敏感分级、类型/范围校验、工具 JSON Schema。

use serde_json::json;
use unlucky_core::setting::catalog::SettingSchema;
use unlucky_core::setting::instance_keys::{instance_schema, profiles_schema};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::schema::{SchemaId, SchemaVersion};
use unlucky_core::setting::spec::{AiPolicy, Sensitivity, SettingSpec, ValueKind};

#[test]
fn ai_whitelist_excludes_credentials() {
    let schema = launcher_schema();
    let writable = schema.ai_writable_keys();
    assert!(writable.contains(&"download.concurrency"));
    assert!(writable.contains(&"appearance.theme_mode"));
    assert!(!writable.contains(&"ai.api_key"), "凭据永不可写（D9）");

    assert!(schema.accepts_ai_write("java.selected_path"));
    assert!(!schema.accepts_ai_write("ai.api_key"));
    assert!(!schema.accepts_ai_write("not.a.key"));
}

#[test]
fn sensitivity_grades_are_flagged() {
    let schema = launcher_schema();
    assert_eq!(
        schema.sensitivity("core.data_mode"),
        Some(Sensitivity::High)
    );
    assert_eq!(
        schema.sensitivity("download.concurrency"),
        Some(Sensitivity::Low)
    );
    assert_eq!(schema.sensitivity("ai.model"), Some(Sensitivity::Medium));
    assert_eq!(schema.sensitivity("ai.api_key"), Some(Sensitivity::High));
    assert!(Sensitivity::High.requires_confirmation());
    assert!(!Sensitivity::Low.requires_confirmation());
    assert_eq!(Sensitivity::Medium.as_str(), "medium");
}

#[test]
fn validates_types_ranges_and_enums() {
    let schema = launcher_schema();
    assert!(schema.validate("download.concurrency", &json!(8)).is_ok());
    assert!(schema.validate("download.concurrency", &json!(0)).is_err());
    assert!(schema
        .validate("download.concurrency", &json!(257))
        .is_err());
    assert!(schema
        .validate("download.concurrency", &json!("8"))
        .is_err());
    assert!(schema
        .validate("appearance.theme_mode", &json!("dark"))
        .is_ok());
    assert!(schema
        .validate("appearance.theme_mode", &json!("solarized"))
        .is_err());
    // 废弃键仍然「读兼容合法」——旧文件里的值不该被判成非法。
    assert!(schema.validate("ui.theme", &json!("metro_dark")).is_ok());
    assert!(schema.validate("ui.theme", &json!("auto")).is_err());
    assert!(schema.validate("ui.scale", &json!(1.5)).is_ok());
    assert!(schema.validate("ui.scale", &json!(9.0)).is_err());

    let err = schema.validate("nope.key", &json!(1)).expect_err("未知键");
    assert!(err.to_string().contains("nope.key"));
}

#[test]
fn value_kinds_cover_all_variants() {
    assert!(ValueKind::Int { min: 1, max: 2 }.check(&json!(2)).is_ok());
    assert!(ValueKind::Int { min: 1, max: 2 }.check(&json!(3)).is_err());
    assert!(ValueKind::Float { min: 0.0, max: 1.0 }
        .check(&json!(0.5))
        .is_ok());
    assert!(ValueKind::Text { max_len: 2 }.check(&json!("ab")).is_ok());
    assert!(ValueKind::Text { max_len: 2 }.check(&json!("abc")).is_err());
    assert!(ValueKind::Path.check(&json!("  ")).is_err());
    assert!(ValueKind::Path
        .check(&json!("C:/java/bin/java.exe"))
        .is_ok());
    assert!(ValueKind::TextList.check(&json!(["a", "b"])).is_ok());
    assert!(ValueKind::TextList.check(&json!([1])).is_err());
    assert!(ValueKind::Any.check(&json!({"whatever": 1})).is_ok());
}

#[test]
fn json_schema_projection_feeds_tool_specs() {
    let schema = launcher_schema();
    let projected = schema.to_json_schema();
    assert_eq!(projected["type"], json!("object"));
    assert_eq!(projected["additionalProperties"], json!(false));

    let properties = projected["properties"].as_object().expect("properties");
    assert!(properties.contains_key("download.concurrency"));
    assert!(!properties.contains_key("ai.api_key"), "锁定键不进工具参数");
    assert_eq!(properties["download.concurrency"]["type"], json!("integer"));
    assert_eq!(properties["download.concurrency"]["default"], json!(8));
    assert_eq!(
        properties["download.concurrency"]["x-ai-sensitivity"],
        json!("low")
    );
    assert_eq!(properties["appearance.theme_mode"]["type"], json!("string"));
}

#[test]
fn top_level_keys_and_defaults() {
    let schema = launcher_schema();
    let tops = schema.top_level_keys();
    for expected in ["core", "download", "java", "ai", "ui", "log"] {
        assert!(tops.contains(expected), "缺少顶层键组 {expected}");
    }
    assert_eq!(
        schema.default_value("download.concurrency"),
        Some(&json!(8))
    );
    assert_eq!(
        schema.default_value("java.selected_path"),
        Some(&json!(null))
    );
    assert!(schema.spec("download.concurrency").is_some());
    assert!(schema.has_prefix(&["download"]));
    assert!(!schema.has_prefix(&["nope"]));
}

#[test]
fn instance_and_profiles_schemas_are_usable() {
    let instance = instance_schema();
    assert_eq!(instance.id(), SchemaId::INSTANCE);
    // D20：实例键表统一成 camelCase 扁平形态（与 game::install::instance 的 serde 逐键一致）。
    assert!(instance.is_known("versionId"));
    assert!(instance.is_known("isolation"));
    assert!(instance.is_known("lastModeChange.probe.isolatedSide"));
    assert!(instance.accepts_ai_write("isolation"));
    assert!(
        !instance.accepts_ai_write("lastModeChange.at"),
        "切换记录由启动器写，AI 不许改"
    );
    assert!(instance.validate("isolation", &json!("isolated")).is_ok());
    assert!(instance.validate("isolation", &json!("inherit")).is_ok());
    assert!(instance.validate("isolation", &json!("maybe")).is_err());
    assert!(
        instance
            .validate("isolation", &json!("per_version"))
            .is_err(),
        "旧字面量由 game 侧读兼容，schema 白名单只认新值"
    );

    let profiles = profiles_schema();
    assert_eq!(profiles.id(), SchemaId::PROFILES);
    assert_eq!(
        profiles.spec("profiles").expect("profiles 键").kind,
        ValueKind::Any
    );
    assert!(
        !profiles.accepts_ai_write("profiles"),
        "列表本体不许 AI 整体改写"
    );
    assert!(profiles.accepts_ai_write("selected_profile_id"));
}

#[test]
#[should_panic(expected = "重复的配置键")]
fn duplicate_key_is_a_table_bug() {
    let schema = SettingSchema::new(SchemaId::LAUNCHER, SchemaVersion::new(9, 0, 0))
        .with(SettingSpec::bool(
            "dup.key",
            true,
            AiPolicy::silent(),
            "第一次",
        ))
        .with(SettingSpec::bool(
            "dup.key",
            false,
            AiPolicy::silent(),
            "第二次",
        ));
    assert_eq!(schema.specs().len(), 2);
}
