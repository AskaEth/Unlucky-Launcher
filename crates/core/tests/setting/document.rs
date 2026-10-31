//! 文档模型：脏字段、未知字段保留、默认值不落盘、嵌套点路径、坏值容错、`$schema` 保留。

use serde_json::{json, Value};
use unlucky_core::setting::document::{LoadProblem, RenderMode, SettingDocument};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::schema::{SchemaRef, SchemaVersion, SCHEMA_KEY, SCHEMA_VERSION_KEY};

fn schema_url(version: SchemaVersion) -> String {
    SchemaRef::new(unlucky_core::setting::schema::SchemaId::LAUNCHER, version).url()
}

fn loaded(raw: Value) -> (SettingDocument, Vec<LoadProblem>) {
    SettingDocument::load(&launcher_schema(), &raw)
}

#[test]
fn reads_nested_keys_and_keeps_unknown_subtrees() {
    let raw = json!({
        SCHEMA_KEY: schema_url(launcher_version()),
        SCHEMA_VERSION_KEY: 1,
        "download": { "concurrency": 16 },
        "ui": { "language": "en_us", "unknown_ui": true },
        "future_section": { "nested": { "flag": true } }
    });

    let (doc, problems) = loaded(raw);
    assert_eq!(doc.effective_i64("download.concurrency"), Some(16));
    assert_eq!(doc.effective_str("ui.language"), Some("en_us"));

    let unknown: Vec<&str> = doc.unknown_keys().collect();
    assert!(unknown.contains(&"ui.unknown_ui"), "{unknown:?}");
    assert!(
        unknown.contains(&"future_section.nested.flag"),
        "未知键清单是叶子粒度: {unknown:?}"
    );

    let reported: Vec<&str> = problems
        .iter()
        .filter_map(|problem| match problem {
            LoadProblem::UnknownKey { key } => Some(key.as_str()),
            _ => None,
        })
        .collect();
    assert!(reported.contains(&"ui.unknown_ui"), "{problems:?}");
    assert!(
        reported.contains(&"future_section"),
        "陌生子树按最上层节点报一次，不做逐叶展开: {problems:?}"
    );

    let rendered = doc.to_value();
    assert_eq!(rendered[SCHEMA_VERSION_KEY], json!(1));
    assert_eq!(
        rendered["future_section"]["nested"]["flag"],
        json!(true),
        "未知字段必须原样回写"
    );
    assert_eq!(rendered["ui"]["unknown_ui"], json!(true));
}

#[test]
fn defaults_never_reach_the_file() {
    let (doc, _) = loaded(json!({ SCHEMA_KEY: schema_url(launcher_version()) }));
    // 生效值有默认，但落盘视图里没有这些键。
    assert_eq!(doc.effective_i64("download.concurrency"), Some(8));
    assert_eq!(doc.effective_bool("ai.enabled"), Some(true));
    assert!(doc.get("download.concurrency").is_none());
    let rendered = doc.to_value();
    assert!(rendered.get("download").is_none(), "{rendered}");
    // 无默认值的路径键 → None。
    assert!(doc.effective_str("java.selected_path").is_none());
}

#[test]
fn tracks_dirty_keys_and_renders_patches() {
    let raw = json!({
        SCHEMA_KEY: schema_url(launcher_version()),
        "download": { "concurrency": 8, "retry_limit": 2 }
    });
    let (mut doc, _) = loaded(raw);
    assert!(!doc.is_dirty());

    doc.set("download.concurrency", json!(32)).expect("写");
    doc.set("appearance.theme_mode", json!("dark")).expect("写");

    let mut dirty: Vec<&str> = doc.dirty_keys().collect();
    dirty.sort_unstable();
    assert_eq!(dirty, vec!["appearance.theme_mode", "download.concurrency"]);

    let patch = doc.dirty_patch();
    assert_eq!(patch["download"]["concurrency"], json!(32));
    assert_eq!(patch["appearance"]["theme_mode"], json!("dark"));
    assert!(
        patch["download"].get("retry_limit").is_none(),
        "非脏字段不进补丁"
    );

    let dirty_view = doc.render(RenderMode::DirtyOnly);
    assert_eq!(dirty_view["download"]["concurrency"], json!(32));
    assert!(dirty_view["download"].get("retry_limit").is_none());

    doc.mark_clean();
    assert!(!doc.is_dirty());
    assert_eq!(doc.dirty_patch(), json!({}));
}

#[test]
fn invalid_values_are_preserved_with_a_problem() {
    let raw = json!({
        SCHEMA_KEY: schema_url(launcher_version()),
        "download": { "concurrency": 9999, "retry_limit": "many" }
    });
    let (doc, problems) = loaded(raw);
    assert_eq!(
        doc.get("download.concurrency").and_then(Value::as_i64),
        Some(9999),
        "坏值原样保留（用户能看见自己写坏了什么）"
    );
    let invalid: Vec<&str> = problems
        .iter()
        .filter_map(|problem| match problem {
            LoadProblem::InvalidValue { key, .. } => Some(key.as_str()),
            _ => None,
        })
        .collect();
    assert!(invalid.contains(&"download.concurrency"), "{problems:?}");
    assert!(invalid.contains(&"download.retry_limit"), "{problems:?}");
}

#[test]
fn schema_version_mirror_mismatch_is_reported_but_schema_wins() {
    let raw = json!({
        SCHEMA_KEY: schema_url(launcher_version()),
        SCHEMA_VERSION_KEY: 7
    });
    let (doc, problems) = loaded(raw);
    assert_eq!(doc.version(), launcher_version());
    assert!(problems.iter().any(|problem| matches!(
        problem,
        LoadProblem::SchemaVersionMismatch {
            declared: 7,
            expected: 1
        }
    )));
}

#[test]
fn newer_patch_preserves_original_schema_string() {
    // 同 minor、更高 patch（launcher schema 当前 1.3.0）。
    let newer = SchemaVersion::new(1, 3, 7);
    let url = schema_url(newer);
    let (doc, _) = loaded(json!({ SCHEMA_KEY: url.clone() }));
    assert_eq!(
        doc.preserve_schema(),
        Some(url.as_str()),
        "同 minor 更高 patch 要保留原 $schema"
    );
    assert_eq!(doc.to_value()[SCHEMA_KEY], json!(url));
    assert_eq!(doc.version(), newer);
}

#[test]
fn wrong_shape_for_known_prefix_is_reported() {
    let raw = json!({
        SCHEMA_KEY: schema_url(launcher_version()),
        "download": "oops"
    });
    let (doc, problems) = loaded(raw);
    assert!(
        problems.iter().any(|problem| matches!(
            problem,
            LoadProblem::InvalidShape { key, .. } if key == "download"
        )),
        "{problems:?}"
    );
    assert!(doc.get("download").is_none());
    // 原始坏结构不进 values，也不会被当成未知键回写。
    assert!(doc.to_value().get("download").is_none());
}

#[test]
fn non_object_json_yields_problem_and_empty_doc() {
    let (doc, problems) = loaded(json!([1, 2, 3]));
    assert!(!problems.is_empty());
    assert_eq!(doc.effective_i64("download.concurrency"), Some(8));
}

#[test]
fn set_rejects_unknown_keys_and_out_of_range_values() {
    let (mut doc, _) = loaded(json!({}));
    assert!(doc.set("nope.key", json!(1)).is_err());
    assert!(doc.set("download.concurrency", json!(0)).is_err());
    assert!(doc
        .set("appearance.theme_mode", json!("solarized"))
        .is_err());
    assert!(!doc.is_dirty(), "校验失败不能标脏");
}

#[test]
fn unknown_schema_prefix_reports_problem_and_keeps_values() {
    let raw = json!({
        SCHEMA_KEY: "https://unlucky-launcher.local/schema/launcher/1.2.0.json",
        "core": { "data_mode": "standard" },
    });
    let (doc, problems) = loaded(raw);
    // 值照读（旧实现按「没有声明」整份装默认值）。
    assert_eq!(doc.effective_str("core.data_mode"), Some("standard"));
    let found = problems.iter().find_map(|problem| match problem {
        LoadProblem::UnknownSchema { url } => Some(url.as_str()),
        _ => None,
    });
    assert_eq!(
        found,
        Some("https://unlucky-launcher.local/schema/launcher/1.2.0.json"),
        "{problems:?}"
    );
}

fn launcher_version() -> SchemaVersion {
    unlucky_core::setting::keys::launcher_version()
}
