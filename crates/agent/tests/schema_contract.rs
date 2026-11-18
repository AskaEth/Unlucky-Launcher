//! **双投影契约测试**（research/02 §6.2 建议进 CI 的那一条）。
//!
//! rig 侧（`DynamicTool::new`）与 rmcp 侧（`ToolRoute::new_dyn`）吃的都是
//! `(name, description, serde_json::Value schema)`。本 crate 只维护一份 `ToolSpec`，
//! 所以「两侧相等」在这里等价于三条可断言的性质：
//! 1. [`unlucky_agent::registry::ToolRegistry::definitions`] 是纯投影（多次调用逐字节相等）；
//! 2. 每个 schema 根是 `type: object`（两侧都要求对象入参）且拒绝多余参数；
//! 3. 名字唯一，`required` 真的被执行（缺参能被校验器抓到）。
//!
//! 本文件只锁**投影载荷本身**；两侧的一致性各有专测：
//! - rig 侧：`tests/brain_loop.rs` 断言 `provider` 收到的 `request.tools` 与
//!   `registry.definitions()` 逐字节相等（`brain/rig.rs` 只做直投）；
//! - MCP 侧：`tests/mcp_contract.rs` 断言 rmcp `Tool` 的 `(name, description, schema)` 与
//!   注册表逐项相等（M39 补的那条）。

use serde_json::json;
use unlucky_agent::registry::validate::validate;
use unlucky_agent::registry::{Access, ToolDefinition};
use unlucky_agent::testing::mock_ports;

fn registry() -> unlucky_agent::registry::ToolRegistry {
    let (ports, _settings) = mock_ports();
    unlucky_agent::tools::build_v1_registry(&ports).expect("注册表应装配成功")
}

#[test]
fn ships_exactly_the_v1_tools() {
    let registry = registry();
    assert_eq!(
        registry.names(),
        vec![
            "get_config",
            "set_config",
            "get_config_schema",
            "list_instances",
            "list_versions",
            "get_java_status",
            "list_accounts",
            "list_game_sessions",
            "read_game_log",
            "diagnose_crash",
        ]
    );
    assert_eq!(registry.len(), 10);
}

#[test]
fn definitions_are_pure_projections() {
    let registry = registry();
    let first = registry.definitions();
    let second = registry.definitions();
    assert_eq!(first, second, "投影必须是纯函数");

    for definition in &first {
        // 序列化后再反序列化必须完全一样（rig 与 rmcp 都走 JSON）。
        let json = serde_json::to_string(definition).expect("可序列化");
        let round_trip: ToolDefinition = serde_json::from_str(&json).expect("可反序列化");
        assert_eq!(&round_trip, definition);
        assert!(!definition.description.trim().is_empty());
        assert_eq!(
            definition.input_schema["type"],
            json!("object"),
            "{} 的 schema 根必须是 object",
            definition.name
        );
    }
}

#[test]
fn only_set_config_is_mutating() {
    let registry = registry();
    for name in registry.names() {
        let expected = if name == "set_config" {
            Access::Mutating
        } else {
            Access::ReadOnly
        };
        assert_eq!(registry.access_of(name), Some(expected), "{name}");
    }
    let read_only: Vec<String> = registry
        .read_only_definitions()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(
        read_only,
        vec![
            "get_config",
            "get_config_schema",
            "list_instances",
            "list_versions",
            "get_java_status",
            "list_accounts",
            "list_game_sessions",
            "read_game_log",
            "diagnose_crash",
        ],
        "只读 MCP 模式只暴露这九个（写侧只有 set_config）"
    );
}

#[test]
fn every_schema_rejects_unknown_arguments() {
    for definition in registry().definitions() {
        let err = validate(&definition.input_schema, &json!({ "__unexpected": 1 }))
            .expect_err(&format!("{} 应拒绝野参数", definition.name));
        assert!(
            err.iter().any(|e| e.contains("不是可接受的参数名")),
            "{}: {err:?}",
            definition.name
        );
    }
}

#[test]
fn required_fields_are_actually_enforced() {
    for definition in registry().definitions() {
        let required = definition
            .input_schema
            .get("required")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        if required.is_empty() {
            continue;
        }
        let err = validate(&definition.input_schema, &json!({}))
            .expect_err(&format!("{} 应有必填参数", definition.name));
        for field in required {
            let name = field.as_str().unwrap_or_default();
            assert!(
                err.iter().any(|e| e.contains(name)),
                "{} 的必填 `{name}` 没被校验：{err:?}",
                definition.name
            );
        }
    }
}

#[test]
fn integer_bounds_are_declared_and_enforced() {
    let registry = registry();
    let read_log = registry.get("read_game_log").expect("工具存在");
    let max_events = &read_log.input_schema["properties"]["max_events"];
    assert_eq!(max_events["minimum"], json!(1));
    assert_eq!(max_events["maximum"], json!(500));

    assert!(validate(
        &read_log.input_schema,
        &json!({ "session_id": "s", "max_events": 0 })
    )
    .is_err());
    assert!(validate(
        &read_log.input_schema,
        &json!({ "session_id": "s", "max_events": 501 })
    )
    .is_err());
    assert!(validate(
        &read_log.input_schema,
        &json!({ "session_id": "s", "max_events": 500 })
    )
    .is_ok());
}
