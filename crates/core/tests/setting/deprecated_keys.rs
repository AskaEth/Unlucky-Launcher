//! 键废弃机制：`ui.theme` → `appearance.theme_mode`（tower 键冲突裁决）。
//!
//! 规则：**读兼容**（旧值按 `THEME_ALIASES` 映射，新键已在则新键优先）+ **写永远走新键**
//! （`set` 旧键直接报错，落盘不含旧键，AI 白名单与工具 JSON Schema 也不含旧键）。

use serde_json::{json, Value};
use unlucky_core::setting::document::{LoadProblem, SettingDocument};
use unlucky_core::setting::keys::{launcher_schema, LEGACY_THEMES, THEME_ALIASES, THEME_MODES};
use unlucky_core::setting::schema::{SchemaId, SchemaRef, SchemaVersion, SCHEMA_KEY};
use unlucky_core::setting::store::SettingStore;

use crate::common::TempDir;

fn legacy_file(raw_ui: Value) -> Value {
    json!({
        SCHEMA_KEY: SchemaRef::new(SchemaId::LAUNCHER, SchemaVersion::new(1, 0, 0)).url(),
        "ui": raw_ui
    })
}

fn deprecated_problem(problems: &[LoadProblem]) -> Option<&'static str> {
    problems.iter().find_map(|problem| match problem {
        LoadProblem::DeprecatedKey {
            key, replaced_by, ..
        } if key == "ui.theme" => Some(*replaced_by),
        _ => None,
    })
}

#[test]
fn legacy_theme_values_are_translated_on_read() {
    assert_eq!(THEME_MODES.len(), 3);
    assert!(THEME_MODES.contains(&"auto") && THEME_MODES.contains(&"light"));
    assert!(THEME_MODES.contains(&"dark"));
    assert_eq!(
        LEGACY_THEMES.len(),
        THEME_ALIASES.len(),
        "三个旧值都要有映射"
    );

    for (legacy, expected) in [
        ("system", "auto"),
        ("metro_light", "light"),
        ("metro_dark", "dark"),
    ] {
        let (doc, problems) =
            SettingDocument::load(&launcher_schema(), &legacy_file(json!({ "theme": legacy })));

        assert_eq!(
            doc.effective_str("appearance.theme_mode"),
            Some(expected),
            "旧值 {legacy} 应映射成 {expected}"
        );
        assert_eq!(
            doc.legacy_value("ui.theme").and_then(Value::as_str),
            Some(legacy),
            "旧键原文要留着给 UI 提示/回滚"
        );
        assert_eq!(deprecated_problem(&problems), Some("appearance.theme_mode"));
        assert!(!doc.is_dirty(), "读兼容不算用户改动");
    }
}

#[test]
fn new_key_wins_when_both_present() {
    let raw = json!({
        SCHEMA_KEY: SchemaRef::new(SchemaId::LAUNCHER, SchemaVersion::new(1, 0, 0)).url(),
        "ui": { "theme": "metro_dark" },
        "appearance": { "theme_mode": "light" }
    });
    let (doc, _) = SettingDocument::load(&launcher_schema(), &raw);
    assert_eq!(doc.effective_str("appearance.theme_mode"), Some("light"));
}

#[test]
fn unmapped_legacy_value_is_reported() {
    let (doc, problems) =
        SettingDocument::load(&launcher_schema(), &legacy_file(json!({ "theme": "neon" })));
    assert_eq!(doc.effective_str("appearance.theme_mode"), Some("neon"));
    assert!(
        problems.iter().any(|problem| matches!(
            problem,
            LoadProblem::InvalidValue { key, .. } if key == "appearance.theme_mode"
        )),
        "映射不过去的旧值要报出来: {problems:?}"
    );
}

#[test]
fn writing_the_deprecated_key_is_refused() {
    let (mut doc, _) = SettingDocument::load(&launcher_schema(), &json!({}));
    let err = doc
        .set("ui.theme", json!("metro_dark"))
        .expect_err("旧键禁止写");
    assert!(err.to_string().contains("appearance.theme_mode"), "{err}");
    assert!(!doc.is_dirty());
}

#[test]
fn save_drops_the_old_key_and_writes_the_new_one() {
    let temp = TempDir::new("deprecated-save");
    temp.write(
        "config/launcher.json",
        &legacy_file(json!({ "theme": "metro_dark", "language": "en_us" })).to_string(),
    );

    let mut store = SettingStore::load(temp.child("config/launcher.json"), launcher_schema());
    assert_eq!(
        store.effective("appearance.theme_mode"),
        Some(&json!("dark"))
    );

    // 随便改一个别的键触发一次保存：旧键消失、新键（映射值）落盘。
    store.set("ui.scale", json!(1.25)).expect("改缩放");
    store.save().expect("保存");

    let saved: Value = serde_json::from_str(&temp.read("config/launcher.json")).expect("JSON 合法");
    assert_eq!(saved["appearance"]["theme_mode"], json!("dark"));
    assert!(
        saved["ui"].get("theme").is_none(),
        "废弃键不再落盘: {saved}"
    );
    assert_eq!(saved["ui"]["language"], json!("en_us"));
}

#[test]
fn whitelist_and_tool_schema_exclude_the_old_key() {
    let schema = launcher_schema();
    assert_eq!(schema.deprecated_keys(), vec!["ui.theme"]);
    assert_eq!(
        schema.replacement_of("ui.theme"),
        Some("appearance.theme_mode")
    );
    assert_eq!(schema.replacement_of("appearance.theme_mode"), None);
    assert!(!schema.accepts_ai_write("ui.theme"), "废弃键不许 AI 写");
    assert!(!schema.ai_writable_keys().contains(&"ui.theme"));

    let projected = schema.to_json_schema();
    let properties = projected["properties"].as_object().expect("properties");
    assert!(properties.contains_key("appearance.theme_mode"));
    assert!(!properties.contains_key("ui.theme"));
}
