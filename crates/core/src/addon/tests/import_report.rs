//! 报告的**契约**测试：三态退出码、JSON 形状、文案 key 表没有死常量。

use serde_json::Value;

use crate::addon::import::{
    import_pack, keys, ImportRequest, ImportStage, ImportStatus, MissingReason,
};
use crate::download::blocking::block_on;

use super::support::mrpack_stub;
use super::temp_root;

#[test]
fn status_maps_to_three_distinct_exit_codes() {
    // CLI 契约：0 = Success，3 = Partial，1 = Failed（2 留给内部错误）。
    assert_eq!(ImportStatus::Success.exit_code(), 0);
    assert_eq!(ImportStatus::Partial.exit_code(), 3);
    assert_eq!(ImportStatus::Failed.exit_code(), 1);
    // 每个状态都有独立文案 key。
    let mut keyed = vec![
        ImportStatus::Success.message_key(),
        ImportStatus::Partial.message_key(),
        ImportStatus::Failed.message_key(),
    ];
    keyed.sort_unstable();
    keyed.dedup();
    assert_eq!(keyed.len(), 3);
    // 五个阶段也各有 key，且都能进报告。
    let stages = [
        ImportStage::Probe,
        ImportStage::Extract,
        ImportStage::Fetch,
        ImportStage::Commit,
        ImportStage::Register,
    ];
    assert_eq!(stages.len(), 5);
    for stage in stages {
        assert!(keys::ALL.contains(&stage.message_key()), "{:?}", stage);
    }
}

#[test]
fn report_serialises_to_stable_json_with_structured_error() {
    // 失败也要输出结构化 JSON（不抛栈）：`status` / `error.code` / `rolled_back` 一个不少。
    let root = temp_root("report-json");
    let pack = root.join("evil.zip");
    super::write_zip(&pack, &[("manifest.json", b"{\"manifestType\":")]);
    let target = root.join("instance");
    let report = block_on(import_pack(&ImportRequest::new(&pack, &target)));

    let value: Value = serde_json::to_value(&report).expect("报告要能序列化");
    assert_eq!(value["status"], Value::String("failed".to_string()));
    assert_eq!(value["rolled_back"], Value::Bool(true));
    let error = value["error"].as_object().expect("error 是对象");
    assert!(error["code"].is_string(), "{error:?}");
    assert!(error["message"].is_string(), "{error:?}");
    assert_eq!(report.status.exit_code(), 1);
}

#[test]
fn successful_report_serialises_counts_and_loader() {
    let (fixture, fetcher) = mrpack_stub("report-ok");
    let target = temp_root("report-ok-out").join("instance");
    let report = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher),
    ));
    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);

    let value: Value = serde_json::to_value(&report).expect("序列化");
    assert_eq!(value["status"], Value::String("success".to_string()));
    assert_eq!(value["format"], Value::String("modrinth".to_string()));
    assert_eq!(value["loader"]["kind"], Value::String("fabric".to_string()));
    assert_eq!(
        value["loader"]["version_id"],
        Value::String("1.20.1-fabric-0.15.11".to_string())
    );
    assert!(value["files"]
        .as_array()
        .is_some_and(|files| !files.is_empty()));
    assert!(value["conflicts"].as_array().is_some_and(Vec::is_empty));
    assert!(value["missing"].as_array().is_some_and(Vec::is_empty));
}

#[test]
fn every_message_key_is_referenced_somewhere() {
    // 防"死常量表"：keys 模块里每个常量除了**定义**与 `ALL` 里的**一处**列举，至少还要有
    // 一处真实引用（`keys::X` 或同模块里的 `X`）。少于此数就说明它没人用。
    let table = key_table();
    let mut values: Vec<&str> = table.iter().map(|(_, value)| value.as_str()).collect();
    values.sort_unstable();
    let mut declared = keys::ALL.to_vec();
    declared.sort_unstable();
    assert_eq!(values, declared, "keys::ALL 与常量定义不一致");

    let sources = SOURCES.concat();
    let mut dead: Vec<&str> = Vec::new();
    for (name, value) in &table {
        let hits = sources.matches(name.as_str()).count();
        assert!(
            hits >= 3,
            "key {name}（{value}）只出现 {hits} 次，像是死常量"
        );
        if hits < 3 {
            dead.push(name);
        }
    }
    assert!(dead.is_empty(), "{dead:?}");
}

#[test]
fn reason_enums_carry_their_own_keys() {
    for reason in [
        MissingReason::Offline,
        MissingReason::NoSource,
        MissingReason::FetchFailed,
    ] {
        assert!(keys::ALL.contains(&reason.message_key()), "{reason:?}");
    }
}

/// 导入模块的全部源码（key 引用计数用）。
const SOURCES: &[&str] = &[
    include_str!("../import.rs"),
    include_str!("../import/error.rs"),
    include_str!("../import/report.rs"),
    include_str!("../import/report/keys.rs"),
    include_str!("../import/report/items.rs"),
    include_str!("../import/plan/paths.rs"),
    include_str!("../import/stage/commit.rs"),
    include_str!("../import/baseline.rs"),
    include_str!("../import/fetch.rs"),
    include_str!("../import/merge.rs"),
    include_str!("../import/phase.rs"),
    include_str!("../import/pipeline.rs"),
    include_str!("../import/plan.rs"),
    include_str!("../import/stage.rs"),
    include_str!("../mod.rs"),
];

/// 从 `keys` 模块的源码里解析出 `(常量名, 常量值)` 表（不手抄第二份清单）。
fn key_table() -> Vec<(String, String)> {
    let source = include_str!("../import/report/keys.rs");
    let mut out = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("pub const ") else {
            continue;
        };
        let Some((name, tail)) = rest.split_once(": &str = \"") else {
            continue;
        };
        let Some((value, _)) = tail.split_once('"') else {
            continue;
        };
        out.push((name.to_string(), value.to_string()));
    }
    assert!(!out.is_empty(), "没有解析到任何 key 常量");
    out
}
