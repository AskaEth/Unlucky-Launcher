//! **双投影契约测试（MCP 侧）**：注册表 → rmcp `Tool` 必须与 rig 侧逐项相等。
//!
//! research/02 §6.2 建议进 CI 的那条断言，M16 落的是「注册表自身是纯投影」+「rig 适配层
//! 只做直投」，M39 补上 rmcp 侧：
//!
//! 1. `mcp::projection::tool_attrs(&registry)` 的 `(name, description, input_schema)`
//!    与 `registry.definitions()` **逐项相等**（JSON 层面，不是"看起来差不多"）；
//! 2. `tools/list` 的顺序与注册顺序一致，数量等于注册表长度；
//! 3. 每个 schema 根是 `object`（rig 与 rmcp 的共同前提）；
//! 4. `read_only_hint` 与 `Access` 一致（外部客户端据此决定能不能静默调用）；
//! 5. 只读子集不会漏出写工具（`agent.mcp.allow_write=false` 的默认形态）。

use serde_json::{json, Value};
use unlucky_agent::mcp::projection::{tool_attr, tool_attrs};
use unlucky_agent::port::Ports;
use unlucky_agent::registry::{Access, ToolDefinition, ToolRegistry};
use unlucky_agent::testing::mock_ports;
use unlucky_agent::tools::build_v1_registry;

fn registry() -> ToolRegistry {
    let (ports, _settings) = mock_ports();
    build_v1_registry(&ports).expect("注册表应装配成功")
}

fn ports() -> Ports {
    mock_ports().0
}

#[test]
fn mcp_projection_equals_registry_definition_item_by_item() {
    let registry = registry();
    let definitions = registry.definitions();
    let attrs = tool_attrs(&registry);
    assert_eq!(attrs.len(), definitions.len());

    for (attr, definition) in attrs.iter().zip(definitions.iter()) {
        assert_eq!(attr.name.as_ref(), definition.name, "工具名必须相等");
        assert_eq!(
            attr.description.as_deref(),
            Some(definition.description.as_str()),
            "{} 的 description 必须逐字相等",
            definition.name
        );
        assert_eq!(
            Value::Object((*attr.input_schema).clone()),
            definition.input_schema,
            "{} 的 JSON Schema 必须逐项相等",
            definition.name
        );
    }
}

#[test]
fn projection_order_matches_registration_order() {
    let registry = registry();
    let projected: Vec<String> = tool_attrs(&registry)
        .iter()
        .map(|attr| attr.name.to_string())
        .collect();
    assert_eq!(projected, registry.names(), "两侧顺序必须一致");
}

#[test]
fn every_projected_schema_is_an_object_and_round_trips() {
    for attr in tool_attrs(&registry()) {
        let schema = Value::Object((*attr.input_schema).clone());
        assert_eq!(
            schema["type"],
            json!("object"),
            "{} 的 schema 根必须是 object",
            attr.name
        );
        // rmcp 会把 Tool 序列化进 tools/list 的响应体：必须能反序列化回来。
        let json = serde_json::to_string(&attr).expect("Tool 可序列化");
        let back: rmcp::model::Tool = serde_json::from_str(&json).expect("Tool 可反序列化");
        assert_eq!(back, attr);
    }
}

#[test]
fn read_only_hint_tracks_access() {
    let registry = registry();
    for (attr, spec) in tool_attrs(&registry).iter().zip(registry.specs()) {
        let hint = attr
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.read_only_hint)
            .expect("每个工具都应有 read_only_hint");
        assert_eq!(hint, spec.access == Access::ReadOnly, "{}", attr.name);
    }
}

#[test]
fn read_only_subset_projects_without_the_writing_tool() {
    let subset = registry().read_only_subset();
    assert_eq!(subset.len(), 9);
    let names: Vec<String> = tool_attrs(&subset)
        .iter()
        .map(|attr| attr.name.to_string())
        .collect();
    assert!(!names.iter().any(|name| name == "set_config"), "{names:?}");
    assert!(subset.access_of("set_config").is_none());
}

#[test]
fn projected_attrs_are_pure_functions_of_the_definition() {
    let definition = ToolDefinition {
        name: "demo".to_string(),
        description: "演示".to_string(),
        input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
    };
    let first = tool_attr(&definition, Access::ReadOnly);
    let second = tool_attr(&definition, Access::ReadOnly);
    assert_eq!(first, second);

    // 副作用等级换一下，只有 annotations 变，name/description/schema 不变。
    let mutating = tool_attr(&definition, Access::Mutating);
    assert_eq!(first.name, mutating.name);
    assert_eq!(first.input_schema, mutating.input_schema);
    assert_ne!(first.annotations, mutating.annotations);
}

#[test]
fn mock_ports_cover_every_port_the_registry_needs() {
    // 装配完整性：五个端口都在，注册表才有这十个工具（少一个端口 = 少工具）。
    let ports = ports();
    let registry = build_v1_registry(&ports).expect("注册表应装配成功");
    assert_eq!(registry.len(), 10);
    assert!(unlucky_agent::testing::expected_v1_tool_names()
        .iter()
        .all(|name| registry.get(name).is_some()));
}
