//! 双投影的 **MCP 侧**：`ToolDefinition` → rmcp `Tool`（`ToolRoute::new_dyn` 的属性入参）。
//!
//! rig 侧在 [`crate::brain::rig`]（`ToolDefinition → RigToolDefinition`）。两侧都只吃裸
//! `serde_json::Value` schema，所以「一致性」是可断言的：`tests/mcp_contract.rs` 遍历注册表，
//! 断言 MCP 侧的 `(name, description, input_schema)` 与注册表逐项相等——这就是
//! research/02 §6.2 建议进 CI 的那条契约。
//!
//! 额外一位信息：`annotations.read_only_hint`。MCP 客户端拿它决定「这个工具能不能
//! 静默调用」，我们用它把 [`Access`] 语义透出去（只读提示，不是权限——权限在编排层）。

use std::sync::Arc;

use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::Value;

use crate::registry::{Access, ToolDefinition, ToolRegistry};

/// 一个 `ToolDefinition` → rmcp `Tool`。
///
/// 注册表在注册阶段已保证 `input_schema` 根是 `object`，所以这里的兜底分支
/// （非对象 → 空 schema）只是为了不 panic：真走到它说明注册校验被绕过了。
#[must_use]
pub fn tool_attr(definition: &ToolDefinition, access: Access) -> Tool {
    let schema: JsonObject = match &definition.input_schema {
        Value::Object(map) => map.clone(),
        _ => JsonObject::new(),
    };
    let mut attr = Tool::new(
        definition.name.clone(),
        definition.description.clone(),
        Arc::new(schema),
    );
    let mut annotations = ToolAnnotations::default();
    annotations.read_only_hint = Some(access == Access::ReadOnly);
    annotations.destructive_hint = Some(access == Access::Mutating);
    attr.annotations = Some(annotations);
    attr
}

/// 注册表 → rmcp `Tool` 列表（顺序同注册顺序，便于两侧比对）。
#[must_use]
pub fn tool_attrs(registry: &ToolRegistry) -> Vec<Tool> {
    registry
        .specs()
        .iter()
        .map(|spec| tool_attr(&spec.definition(), spec.access))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::mock_ports;
    use crate::tools::build_v1_registry;
    use serde_json::json;

    fn registry() -> ToolRegistry {
        let (ports, _settings) = mock_ports();
        build_v1_registry(&ports).expect("注册表应装配成功")
    }

    #[test]
    fn projection_keeps_name_description_and_schema() {
        let registry = registry();
        let attrs = tool_attrs(&registry);
        assert_eq!(attrs.len(), registry.len());

        for (attr, spec) in attrs.iter().zip(registry.specs()) {
            let definition = spec.definition();
            assert_eq!(attr.name.as_ref(), definition.name);
            assert_eq!(
                attr.description.as_deref(),
                Some(definition.description.as_str()),
                "{} 的说明必须逐字相等",
                definition.name
            );
            let projected = Value::Object((*attr.input_schema).clone());
            assert_eq!(
                projected, definition.input_schema,
                "{} 的 JSON Schema 必须逐字相等",
                definition.name
            );
        }
    }

    #[test]
    fn read_only_hint_matches_access() {
        let registry = registry();
        for (attr, spec) in tool_attrs(&registry).iter().zip(registry.specs()) {
            let hint = attr
                .annotations
                .as_ref()
                .and_then(|a| a.read_only_hint)
                .unwrap_or(false);
            assert_eq!(hint, spec.access == Access::ReadOnly, "{}", attr.name);
        }
    }

    #[test]
    fn schema_root_stays_an_object_on_the_wire() {
        for attr in tool_attrs(&registry()) {
            let value = Value::Object((*attr.input_schema).clone());
            assert_eq!(value["type"], json!("object"), "{}", attr.name);
        }
    }
}
