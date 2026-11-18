//! 迁移链：N→N+1 顺序校验、真实迁移跑通、收据闸门防重放、store 接线。

use std::path::PathBuf;

use serde_json::{json, Map, Value};
use unlucky_core::platform::receipt::{
    decide, fingerprint_bytes, fingerprint_file, read as read_receipt, MigrationReceipt,
    ReceiptKind, ReplayDecision,
};
use unlucky_core::setting::catalog::SettingSchema;
use unlucky_core::setting::migrate::{
    MigrationChain, MigrationGate, MigrationReport, SettingMigration,
};
use unlucky_core::setting::schema::{
    SchemaId, SchemaRef, SchemaVersion, SCHEMA_KEY, SCHEMA_VERSION_KEY,
};
use unlucky_core::setting::spec::{AiPolicy, SettingSpec};
use unlucky_core::setting::store::SettingStore;

use crate::common::TempDir;

/// 1.0.0 → 2.0.0：`download.concurrency` 改名 `download.threads`。
struct RenameThreads;

impl SettingMigration for RenameThreads {
    fn from(&self) -> SchemaVersion {
        SchemaVersion::new(1, 0, 0)
    }

    fn to(&self) -> SchemaVersion {
        SchemaVersion::new(2, 0, 0)
    }

    fn describe(&self) -> &'static str {
        "download.concurrency → download.threads"
    }

    fn apply(&self, root: &mut Map<String, Value>) -> Result<(), String> {
        let Some(download) = root.get_mut("download").and_then(Value::as_object_mut) else {
            return Ok(());
        };
        if let Some(value) = download.remove("concurrency") {
            download.insert("threads".to_owned(), value);
        }
        Ok(())
    }
}

/// 2.0.0 → 3.0.0：补一个镜像偏好默认值。
struct AddMirrorPreference;

impl SettingMigration for AddMirrorPreference {
    fn from(&self) -> SchemaVersion {
        SchemaVersion::new(2, 0, 0)
    }

    fn to(&self) -> SchemaVersion {
        SchemaVersion::new(3, 0, 0)
    }

    fn describe(&self) -> &'static str {
        "补齐 download.mirror_preference"
    }

    fn apply(&self, root: &mut Map<String, Value>) -> Result<(), String> {
        let Some(download) = root.get_mut("download").and_then(Value::as_object_mut) else {
            return Err("缺少 download 对象".to_owned());
        };
        download
            .entry("mirror_preference".to_owned())
            .or_insert_with(|| json!("auto"));
        Ok(())
    }
}

fn two_step_chain() -> MigrationChain {
    let mut chain = MigrationChain::new();
    chain.push(Box::new(RenameThreads)).expect("第一环");
    chain.push(Box::new(AddMirrorPreference)).expect("第二环");
    chain
}

#[test]
fn chain_requires_contiguous_major_steps() {
    let mut chain = MigrationChain::new();
    chain
        .push(Box::new(AddMirrorPreference))
        .expect("首环随意起点");
    assert_eq!(chain.len(), 1);

    let err = chain.push(Box::new(RenameThreads)).expect_err("逆序要报错");
    assert!(err.to_string().contains("顺序错误"), "{err}");
    assert_eq!(chain.latest(), Some(SchemaVersion::new(3, 0, 0)));
    assert_eq!(chain.describe().len(), 1);
    assert!(!chain.is_empty());
}

#[test]
fn duplicate_and_skipping_steps_are_rejected() {
    let mut chain = MigrationChain::new();
    chain.push(Box::new(RenameThreads)).expect("首环");
    assert!(chain.push(Box::new(RenameThreads)).is_err(), "重复起始版本");
    assert!(chain.push(Box::new(AddMirrorPreference)).is_ok());

    struct SkipMajor;
    impl SettingMigration for SkipMajor {
        fn from(&self) -> SchemaVersion {
            SchemaVersion::new(3, 0, 0)
        }
        fn to(&self) -> SchemaVersion {
            SchemaVersion::new(5, 0, 0)
        }
        fn describe(&self) -> &'static str {
            "跳版本（非法）"
        }
        fn apply(&self, _root: &mut Map<String, Value>) -> Result<(), String> {
            Ok(())
        }
    }
    let err = chain
        .push(Box::new(SkipMajor))
        .expect_err("跨两个 major 要报错");
    assert!(err.to_string().contains("major"), "{err}");
}

#[test]
fn migrates_sequentially_and_bumps_schema_version() {
    let chain = two_step_chain();
    let mut root = json!({ "download": { "concurrency": 32 } })
        .as_object()
        .cloned()
        .expect("对象");
    root.insert(SCHEMA_VERSION_KEY.to_owned(), json!(1));

    let report = chain
        .migrate(SchemaVersion::new(1, 0, 0), &mut root)
        .expect("迁移");
    assert_eq!(
        report,
        MigrationReport {
            from: SchemaVersion::new(1, 0, 0),
            to: SchemaVersion::new(3, 0, 0),
            applied: vec![
                "download.concurrency → download.threads",
                "补齐 download.mirror_preference",
            ],
        }
    );
    assert!(!report.is_noop());
    assert_eq!(root["download"]["threads"], json!(32));
    assert_eq!(root["download"]["mirror_preference"], json!("auto"));
    assert!(root["download"].get("concurrency").is_none());
    assert_eq!(root[SCHEMA_VERSION_KEY], json!(3));
}

#[test]
fn unknown_start_version_breaks_the_chain() {
    let chain = two_step_chain();
    let mut root = Map::new();
    let err = chain
        .migrate(SchemaVersion::new(9, 0, 0), &mut root)
        .expect_err("没有 9.0.0 起始步");
    assert!(err.to_string().contains("链里没有"), "{err}");
    assert!(chain.plan_from(SchemaVersion::new(2, 0, 0)).is_ok());
}

#[test]
fn failing_step_reports_migration_error() {
    let mut chain = MigrationChain::new();
    chain.push(Box::new(RenameThreads)).expect("第一环");
    chain.push(Box::new(AddMirrorPreference)).expect("第二环");

    let mut root = json!({ "download": 3 }).as_object().cloned().expect("对象");
    let err = chain
        .migrate(SchemaVersion::new(1, 0, 0), &mut root)
        .expect_err("download 不是对象");
    assert!(err.to_string().contains("迁移"), "{err}");
}

#[test]
fn receipt_gate_prevents_replay() {
    let temp = TempDir::new("migration-gate");
    let source = temp.write("legacy/hmcl.json", "{\"x\":1}");
    let target = temp.child("config/launcher.json");
    let fingerprint = fingerprint_file(&source).expect("算指纹").expect("文件在");

    let receipt_path = temp.child("config/migration-receipt.json");
    let gate = MigrationGate::new(&receipt_path, 1_700_000_000_000);
    assert_eq!(gate.now_ms(), 1_700_000_000_000);
    assert!(gate.should_run(&fingerprint).expect("无收据"));

    let receipt =
        MigrationReceipt::legacy_config(source, target, fingerprint.clone(), gate.now_ms());
    gate.record(&receipt).expect("写收据");
    assert!(!gate.should_run(&fingerprint).expect("同源指纹应跳过"));
    assert_eq!(
        gate.evaluate(&fingerprint).expect("评估"),
        ReplayDecision::SkipSameSource
    );
    assert_eq!(
        gate.evaluate("其他指纹").expect("评估"),
        ReplayDecision::ReplayChangedSource
    );

    let stored = read_receipt(&receipt_path)
        .expect("读收据")
        .expect("有收据");
    assert_eq!(stored.kind, ReceiptKind::LegacyConfig);
    assert_eq!(stored.source_fingerprint, fingerprint);
}

#[test]
fn receipt_decisions_and_fingerprints() {
    let receipt = MigrationReceipt::legacy_config(
        PathBuf::from("/a"),
        PathBuf::from("/b"),
        fingerprint_bytes(b"abc"),
        7,
    );
    assert_eq!(decide(None, "x"), ReplayDecision::ReplayNoReceipt);
    assert_eq!(
        decide(Some(&receipt), &fingerprint_bytes(b"abc")),
        ReplayDecision::SkipSameSource
    );
    assert_eq!(
        decide(Some(&receipt), "changed"),
        ReplayDecision::ReplayChangedSource
    );
    assert_eq!(ReplayDecision::SkipSameSource.as_str(), "skip_same_source");
    assert!(!ReplayDecision::SkipSameSource.should_replay());
    assert_eq!(fingerprint_bytes(b"abc").len(), 64);
    assert_ne!(
        unlucky_core::platform::receipt::fingerprint_data_dir_move(
            &PathBuf::from("/a"),
            &PathBuf::from("/b")
        ),
        unlucky_core::platform::receipt::fingerprint_data_dir_move(
            &PathBuf::from("/a"),
            &PathBuf::from("/c")
        )
    );
}

fn schema_v2() -> SettingSchema {
    SettingSchema::new(SchemaId::LAUNCHER, SchemaVersion::new(2, 0, 0)).with(SettingSpec::int(
        "download.threads",
        8,
        (1, 256),
        AiPolicy::silent(),
        "下载并发数（2.0.0 起的键名）",
    ))
}

#[test]
fn store_applies_chain_then_saves_new_version() {
    let temp = TempDir::new("migration-store");
    let old_url = SchemaRef::new(SchemaId::LAUNCHER, SchemaVersion::new(1, 0, 0)).url();
    temp.write(
        "config/launcher.json",
        &json!({
            SCHEMA_KEY: old_url,
            SCHEMA_VERSION_KEY: 1,
            "download": { "concurrency": 32 }
        })
        .to_string(),
    );

    let mut store = SettingStore::load(temp.child("config/launcher.json"), schema_v2());
    assert_eq!(store.doc().version(), SchemaVersion::new(1, 0, 0));
    assert!(!store.is_dirty(), "加载不是改动");

    let mut chain = MigrationChain::new();
    chain.push(Box::new(RenameThreads)).expect("一环足矣");
    let report = store.apply_chain(&chain).expect("迁移").expect("有迁移");
    assert_eq!(report.to, SchemaVersion::new(2, 0, 0));
    assert_eq!(store.doc().version(), SchemaVersion::new(2, 0, 0));
    assert_eq!(
        store.effective("download.threads"),
        Some(&json!(32)),
        "值要跟着键名一起搬"
    );
    assert!(store.is_dirty(), "迁移结果必须回写");

    store.save().expect("保存");
    let saved: Value = serde_json::from_str(&temp.read("config/launcher.json")).expect("JSON");
    assert_eq!(saved[SCHEMA_VERSION_KEY], json!(2));
    assert!(saved[SCHEMA_KEY]
        .as_str()
        .expect("$schema")
        .ends_with("launcher/2.0.0"));
    assert_eq!(saved["download"]["threads"], json!(32));

    let mut up_to_date = SettingStore::load(temp.child("config/launcher.json"), schema_v2());
    assert!(up_to_date.apply_chain(&chain).expect("已最新").is_none());
}
