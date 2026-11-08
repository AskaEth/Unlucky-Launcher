//! D20 隔离键的 schema 契约：全局 `instances.mode`、实例 `isolation`、`lastModeChange`、
//! 恒共享层不可配、旧配置缺键即默认值（不需要迁移步骤）。

use serde_json::json;
use unlucky_core::game::install::instance::{InstanceConfig, Isolation, OverrideKey};
use unlucky_core::game::isolation::{observe_mode, IsolationMode};
use unlucky_core::game::GameLayout;
use unlucky_core::setting::document::{LoadProblem, RenderMode, SettingDocument};
use unlucky_core::setting::instance_keys::{
    instance_schema, instance_version, ISOLATION_OVERRIDES, ISOLATION_RESOLVED, OVERRIDE_KEYS,
};
use unlucky_core::setting::keys::{launcher_schema, launcher_version, INSTANCE_MODES};
use unlucky_core::setting::schema::{SchemaId, SchemaVersion};
use unlucky_core::setting::spec::{Sensitivity, ValueKind};
use unlucky_core::setting::store::SettingStore;

use super::common::TempDir;

#[test]
fn global_mode_key_defaults_to_isolated() {
    let schema = launcher_schema();
    let spec = schema.spec("instances.mode").expect("instances.mode 键");
    assert_eq!(spec.default, json!("isolated"), "D20 默认隔离");
    assert_eq!(spec.kind, ValueKind::Enum(INSTANCE_MODES));
    assert_eq!(
        spec.ai.sensitivity,
        Sensitivity::High,
        "切档会改变存档可见性，AI 改必须弹窗确认"
    );
    assert!(schema.validate("instances.mode", &json!("shared")).is_ok());
    assert!(schema
        .validate("instances.mode", &json!("isolated"))
        .is_ok());
    assert!(schema
        .validate("instances.mode", &json!("per_version"))
        .is_err());
    assert!(schema.validate("instances.mode", &json!("custom")).is_err());
}

#[test]
fn shared_layers_are_not_configurable() {
    let schema = launcher_schema();
    for key in ["shared.versions", "shared.assets", "shared.libraries"] {
        assert!(
            !schema.is_known(key),
            "{key} 是恒共享层，不该出现在可配键表里"
        );
    }
}

#[test]
fn instance_isolation_defaults_to_inherit() {
    let schema = instance_schema();
    let spec = schema.spec("isolation").expect("isolation 键");
    assert_eq!(spec.default, json!("inherit"));
    assert_eq!(spec.kind, ValueKind::Enum(ISOLATION_OVERRIDES));
    for value in ["inherit", "isolated", "shared", "custom"] {
        assert!(
            schema.validate("isolation", &json!(value)).is_ok(),
            "{value}"
        );
    }
    assert!(schema.validate("isolation", &json!("per_version")).is_err());
}

#[test]
fn inherit_never_reaches_the_file() {
    // 默认值不落盘：空文档渲染出来只应该有 $schema / schemaVersion。
    let schema = instance_schema();
    let (document, problems) = SettingDocument::load(&schema, &json!({}));
    assert!(problems.is_empty(), "{problems:?}");
    let rendered = document.render(RenderMode::Full);
    assert!(rendered.get("isolation").is_none(), "{rendered}");
    assert!(rendered.get("lastModeChange").is_none(), "{rendered}");
    let dirty = document.render(RenderMode::DirtyOnly);
    assert!(dirty.get("isolation").is_none(), "{dirty}");
}

#[test]
fn last_mode_change_keys_are_bookkeeping_only() {
    let schema = instance_schema();
    for key in [
        "lastModeChange.at",
        "lastModeChange.from",
        "lastModeChange.to",
        "lastModeChange.probe.isolatedSide",
        "lastModeChange.probe.sharedSide",
        "lastModeChange.warned",
    ] {
        assert!(schema.is_known(key), "{key} 必须登记");
        assert!(!schema.accepts_ai_write(key), "{key} 不许 AI 写");
    }
    assert!(schema
        .validate("lastModeChange.probe.isolatedSide", &json!(12))
        .is_ok());
    assert!(schema
        .validate("lastModeChange.probe.isolatedSide", &json!(-1))
        .is_err());
    assert!(schema
        .validate("lastModeChange.warned", &json!(true))
        .is_ok());
    assert!(schema.validate("lastModeChange.warned", &json!(1)).is_err());
    assert!(schema
        .validate("lastModeChange.from", &json!("shared"))
        .is_ok());
    assert!(schema
        .validate("lastModeChange.from", &json!("inherit"))
        .is_err());
    assert_eq!(ISOLATION_RESOLVED, ["isolated", "shared", "custom"]);
}

#[test]
fn overrides_key_is_an_enum_whitelist() {
    let schema = instance_schema();
    assert!(schema.validate("overrides", &json!([])).is_ok());
    assert!(schema
        .validate("overrides", &json!(["maxMemory", "runningDirectory"]))
        .is_ok());
    assert!(
        schema
            .validate("overrides", &json!(["max_memory"]))
            .is_err(),
        "旧 snake_case 拼写不许过（拼错会静默失效）"
    );
    assert!(schema.validate("overrides", &json!([1])).is_err());
    assert_eq!(OVERRIDE_KEYS, ["maxMemory", "javaType", "runningDirectory"]);
}

#[test]
fn schema_versions_register_the_new_keys_without_migration() {
    // 1.6.0 登记 M161 的自更新两键（`update.auto_check` / `update.channel`；
    // 1.5.0 = M116 窗口圆角，1.4.0 = M91 外观下层三键，1.3.0 = D17「外观（高级）」，
    // 1.2.0 = D23 的 JDK 落点/优先级键）。
    assert_eq!(launcher_version(), SchemaVersion::new(1, 6, 0));
    assert_eq!(instance_version(), SchemaVersion::new(1, 1, 0));
    assert_eq!(launcher_schema().id(), SchemaId::LAUNCHER);
    // major 仍是 1：迁移链（按 major 走）不需要新步骤，老文件缺键即默认值。
    assert_eq!(launcher_version().major, 1);
    assert_eq!(instance_version().major, 1);
}

#[test]
fn old_launcher_file_without_new_keys_loads_with_defaults() {
    let temp = std::env::temp_dir().join(format!(
        "ull-setting-d20-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&temp).expect("临时目录");
    let path = temp.join("launcher.json");
    // 旧版本写的文件：只有 $schema 1.0.0 和一个老键，没有 instances.mode。
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({
            "$schema": "https://schemas.unlucky-launcher.dev/ull/launcher/1.0.0",
            "schemaVersion": 1,
            "ui": { "scale": 1.25 }
        }))
        .expect("序列化"),
    )
    .expect("写文件");

    let store = SettingStore::load(&path, launcher_schema());
    assert!(store.access().can_read(), "1.0.0 文件必须可读");
    assert!(store.problems().is_empty(), "{:?}", store.problems());
    assert_eq!(
        store.doc().effective_str("instances.mode"),
        Some("isolated"),
        "缺键取 D20 默认值"
    );
    assert_eq!(store.doc().effective_f64("ui.scale"), Some(1.25));
    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn setting_lists_agree_with_the_game_enums() {
    // schema 白名单与 game 侧枚举必须一一对应（两处字面量漂移就是静默不生效）。
    let mut modes: Vec<&str> = [IsolationMode::Isolated, IsolationMode::Shared]
        .iter()
        .map(|mode| mode.as_str())
        .collect();
    modes.sort_unstable();
    let mut declared = INSTANCE_MODES.to_vec();
    declared.sort_unstable();
    assert_eq!(modes, declared);

    for value in ISOLATION_OVERRIDES
        .iter()
        .filter(|value| **value != "inherit")
    {
        assert!(
            Isolation::parse(value).is_some(),
            "{value} 在 game 侧解析不出来"
        );
        assert_eq!(Isolation::parse(value).map(Isolation::as_str), Some(*value));
    }
    for value in ISOLATION_RESOLVED {
        assert!(Isolation::parse(value).is_some(), "{value}");
    }
}

#[test]
fn writer_output_loads_without_unknown_or_invalid_keys() {
    // 回归 P2-1（review r1）：写入者（InstanceConfig + observe_mode）产出的键必须被 schema 全认。
    // 之前 `lastModeChange.warned` 没登记，document 层会把真实文件标成 UnknownKey——
    // 这正是「schema 只做写入者白名单视图」这条契约的裂缝。
    let temp = TempDir::new("d20-writer-shape");
    let game = GameLayout::new(temp.child("game"));
    let mut config = InstanceConfig::new("11111111-2222-4333-8444-555555555555", "1.20.1");
    config.isolation = Some(Isolation::Custom);
    config.game_dir = Some(temp.child("elsewhere"));
    config.overrides = vec![OverrideKey::MaxMemory];
    observe_mode(
        &mut config,
        &game,
        "2026-10-30T00:00:00Z",
        Isolation::Custom,
    )
    .expect("观察");

    let raw: serde_json::Value =
        serde_json::from_str(&config.to_json_string().expect("序列化")).expect("合法 JSON");
    // 写入者确实输出了 warned（P2-1 的前提）。
    assert_eq!(raw["lastModeChange"]["warned"], json!(true));

    let (document, problems) = SettingDocument::load(&instance_schema(), &raw);
    let mut unknown: Vec<String> = Vec::new();
    let mut invalid: Vec<String> = Vec::new();
    for problem in &problems {
        match problem {
            LoadProblem::UnknownKey { key } => unknown.push(key.clone()),
            LoadProblem::InvalidValue { key, reason } => invalid.push(format!("{key}: {reason}")),
            _ => {}
        }
    }
    assert!(unknown.is_empty(), "写入者的键没登记进 schema：{unknown:?}");
    assert!(
        invalid.is_empty(),
        "写入者的值过不了 schema 校验：{invalid:?}"
    );
    assert_eq!(
        document.get("lastModeChange.warned"),
        Some(&json!(true)),
        "记账键要能读回"
    );
    // 落盘视图不能把它当「未知键」重复输出。
    let rendered = document.render(RenderMode::Full);
    assert_eq!(rendered["lastModeChange"]["warned"], json!(true));
    assert_eq!(rendered["overrides"], json!(["maxMemory"]));
}
