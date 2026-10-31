//! 配置文件的加载降级、只写脏字段、坏文件备份、只读/不可读三档行为。

use serde_json::{json, Value};
use unlucky_core::setting::document::LoadProblem;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::schema::{
    FileAccess, SchemaCompat, SchemaId, SchemaRef, SchemaVersion, SCHEMA_KEY, SCHEMA_VERSION_KEY,
};
use unlucky_core::setting::store::{SaveOutcome, SettingStore, SkipReason};

use crate::common::TempDir;

fn schema_url(version: SchemaVersion) -> String {
    SchemaRef::new(SchemaId::LAUNCHER, version).url()
}

fn read_json(temp: &TempDir, name: &str) -> Value {
    serde_json::from_str(&temp.read(name)).expect("配置文件是合法 JSON")
}

#[test]
fn first_run_starts_writable_with_defaults() {
    let temp = TempDir::new("store-new");
    let path = temp.child("config/launcher.json");
    let store = SettingStore::load(&path, launcher_schema());

    assert_eq!(store.access(), FileAccess::ReadWrite);
    assert_eq!(store.compat(), SchemaCompat::Missing);
    assert!(store.problems().is_empty());
    assert_eq!(
        store.effective("download.concurrency"),
        Some(&json!(8)),
        "首次运行用默认值"
    );
    assert!(!path.exists(), "只读加载不该创建文件");
}

#[test]
fn saves_only_when_dirty_and_writes_schema_markers() {
    let temp = TempDir::new("store-save");
    let path = temp.child("config/launcher.json");
    let mut store = SettingStore::load(&path, launcher_schema());

    assert_eq!(
        store.save().expect("空保存"),
        SaveOutcome::Skipped(SkipReason::NotDirty)
    );

    store
        .set("download.concurrency", json!(16))
        .expect("改配置");
    let outcome = store.save().expect("保存");
    assert!(outcome.wrote());

    let raw = read_json(&temp, "config/launcher.json");
    assert_eq!(raw["download"]["concurrency"], json!(16));
    assert_eq!(raw[SCHEMA_VERSION_KEY], json!(1));
    assert!(
        raw[SCHEMA_KEY]
            .as_str()
            .expect("$schema")
            .ends_with(&format!("launcher/{}", launcher_schema().version())),
        "保存写当前 schema 版本"
    );
    assert!(raw.get("ai").is_none(), "默认值不进文件");
    assert!(!store.is_dirty(), "保存后清脏");

    let reloaded = SettingStore::load(&path, launcher_schema());
    assert_eq!(reloaded.effective("download.concurrency"), Some(&json!(16)));
}

#[test]
fn preserves_unknown_keys_across_save() {
    let temp = TempDir::new("store-unknown");
    let raw = json!({
        SCHEMA_KEY: schema_url(SchemaVersion::new(1, 0, 0)),
        SCHEMA_VERSION_KEY: 1,
        "download": { "concurrency": 4 },
        "from_the_future": { "flag": true }
    });
    temp.write("config/launcher.json", &raw.to_string());

    let mut store = SettingStore::load(temp.child("config/launcher.json"), launcher_schema());
    assert!(
        store.problems().iter().any(
            |problem| matches!(problem, LoadProblem::UnknownKey { key } if key == "from_the_future")
        ),
        "陌生顶层子树按最上层节点报一次: {:?}",
        store.problems()
    );

    store
        .set("download.concurrency", json!(12))
        .expect("改配置");
    store.save().expect("保存");

    let after = read_json(&temp, "config/launcher.json");
    assert_eq!(after["download"]["concurrency"], json!(12));
    assert_eq!(after["from_the_future"]["flag"], json!(true));
}

#[test]
fn corrupt_file_is_backed_up_before_overwrite() {
    let temp = TempDir::new("store-corrupt");
    temp.write("config/launcher.json", "{ definitely not json");

    let mut store = SettingStore::load(temp.child("config/launcher.json"), launcher_schema());
    assert_eq!(store.compat(), SchemaCompat::Unparseable);
    assert_eq!(
        store.access(),
        FileAccess::ReadWrite,
        "坏文件要能修，否则用户被永久卡住"
    );
    assert_eq!(store.effective("download.concurrency"), Some(&json!(8)));

    store.set("ui.language", json!("en_us")).expect("改配置");
    store.save().expect("保存");

    assert!(temp.exists("config/launcher.json.1"), "坏文件先备份成 .1");
    let raw = read_json(&temp, "config/launcher.json");
    assert_eq!(raw["ui"]["language"], json!("en_us"));
}

#[test]
fn newer_minor_is_read_only_and_keeps_its_schema() {
    let temp = TempDir::new("store-readonly");
    // 比当前 schema（1.3.0）更高的 minor：只读且保留原 $schema。
    let url = schema_url(SchemaVersion::new(1, 4, 0));
    temp.write(
        "config/launcher.json",
        &json!({ SCHEMA_KEY: url, "download": { "concurrency": 32 } }).to_string(),
    );

    let mut store = SettingStore::load(temp.child("config/launcher.json"), launcher_schema());
    assert_eq!(store.compat(), SchemaCompat::ReadOnlyPreserveSchema);
    assert_eq!(store.access(), FileAccess::ReadOnly);
    assert_eq!(
        store.effective("download.concurrency"),
        Some(&json!(32)),
        "只读档的值照读"
    );
    assert!(store.set("download.concurrency", json!(1)).is_err());
    assert_eq!(
        store.save().expect("只读保存"),
        SaveOutcome::Skipped(SkipReason::ReadOnly)
    );

    store.save_forced().expect("用户确认强写");
    assert_eq!(store.access(), FileAccess::ReadWrite);
    let raw = read_json(&temp, "config/launcher.json");
    assert!(
        raw[SCHEMA_KEY]
            .as_str()
            .expect("$schema")
            .ends_with("launcher/1.4.0"),
        "更 minor 的文件强写后也要保留原 $schema"
    );
}

#[test]
fn higher_major_is_unreadable_until_forced() {
    let temp = TempDir::new("store-unreadable");
    let url = schema_url(SchemaVersion::new(2, 0, 0));
    temp.write(
        "config/launcher.json",
        &json!({ SCHEMA_KEY: url, "brand_new": { "thing": 1 } }).to_string(),
    );

    let mut store = SettingStore::load(temp.child("config/launcher.json"), launcher_schema());
    assert_eq!(store.compat(), SchemaCompat::UnsupportedMajor);
    assert_eq!(store.access(), FileAccess::Unreadable);
    assert!(
        store.set("appearance.theme_mode", json!("dark")).is_err(),
        "不可读档全程不写"
    );
    assert_eq!(
        store.save().expect("不可读保存"),
        SaveOutcome::Skipped(SkipReason::Unreadable)
    );

    store.save_forced().expect("用户确认强写");
    assert!(temp.exists("config/launcher.json.1"));
    assert_eq!(store.access(), FileAccess::ReadWrite);
}

#[test]
fn reload_discards_in_memory_changes() {
    let temp = TempDir::new("store-reload");
    let path = temp.write(
        "config/launcher.json",
        &json!({ SCHEMA_KEY: schema_url(SchemaVersion::new(1, 0, 0)), "appearance": { "theme_mode": "light" } })
            .to_string(),
    );

    let mut store = SettingStore::load(&path, launcher_schema());
    store
        .set("appearance.theme_mode", json!("dark"))
        .expect("改配置");
    assert!(store.is_dirty());
    store.reload();
    assert!(!store.is_dirty());
    assert_eq!(
        store.effective("appearance.theme_mode"),
        Some(&json!("light"))
    );
}

#[test]
fn backup_paths_increment() {
    let temp = TempDir::new("store-backup");
    temp.write("config/launcher.json.1", "old");
    temp.write("config/launcher.json.2", "older");
    let store = SettingStore::load(temp.child("config/launcher.json"), launcher_schema());
    let next = store.next_backup_path();
    assert!(next.to_string_lossy().ends_with(".3"), "{next:?}");
}

/// finding 20261030-worker-java2-bug-schema-launcher-json-loadproblem：
/// `$schema` 前缀写错（手写/脚本生成的配置最常见的错）曾让整份文件静默回落默认值。
/// M40 起：值照读 + `UnknownSchema` 告警 + 文件只读（不悄悄改写我们看不懂的文件）。
#[test]
fn unknown_schema_prefix_warns_but_keeps_file_values() {
    let temp = TempDir::new("store-unknown-schema");
    let url = "https://unlucky-launcher.local/schema/launcher/1.2.0.json";
    temp.write(
        "config/launcher.json",
        &json!({
            SCHEMA_KEY: url,
            "core": { "data_mode": "standard" },
            "download": { "concurrency": 32 },
        })
        .to_string(),
    );

    let mut store = SettingStore::load(temp.child("config/launcher.json"), launcher_schema());
    assert_eq!(store.compat(), SchemaCompat::Invalid);
    assert_eq!(store.access(), FileAccess::ReadOnly);
    assert_eq!(
        store.effective("download.concurrency"),
        Some(&json!(32)),
        "认不出 $schema 也要读文件里的值"
    );
    assert!(
        store.problems().iter().any(
            |problem| matches!(problem, LoadProblem::UnknownSchema { url: found } if found == url)
        ),
        "{:?}",
        store.problems()
    );
    assert!(
        store.problems()[0].to_string().contains("$schema"),
        "{}",
        store.problems()[0]
    );
    assert!(store.set("download.concurrency", json!(1)).is_err());
    assert_eq!(
        store.save().expect("只读保存"),
        SaveOutcome::Skipped(SkipReason::ReadOnly)
    );
}
