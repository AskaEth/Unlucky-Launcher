//! `get_config_schema`：把 M12 的配置 schema（**只含 AI 可写键**）投给模型。
//!
//! 为什么单独一个工具而不是塞进 `get_config`：模型要能「先看有哪些键、类型是什么、
//! 哪些是敏感键」，再决定要不要写——这是一次性的静态信息，和读当前值不是同一件事。
//! 拆开还能让 token 预算更可控（schema 比概要大一个量级）。
//!
//! 凭据边界：schema 由 M12 的 `SettingSchema::to_json_schema()` 生成，只遍历
//! `ai.writable && !deprecated` 的键，`ai.api_key`（`locked`）天然不在其中。

use std::sync::Arc;

use serde_json::{json, Value};

use crate::port::SettingPort;
use crate::registry::schema::object;
use crate::registry::{
    RegistryError, ToolCtx, ToolError, ToolFn, ToolOutput, ToolRegistry, ToolSpec,
};

/// 工具名。
pub const GET_CONFIG_SCHEMA: &str = "get_config_schema";

/// 注册 `get_config_schema`。
///
/// # Errors
/// 工具名重复/schema 非法时返回 [`RegistryError`]。
pub fn register(
    registry: &mut ToolRegistry,
    port: Arc<dyn SettingPort>,
) -> Result<(), RegistryError> {
    let handler: ToolFn = Arc::new(move |_ctx: &ToolCtx, _args: &Value| {
        let schema = port.schema();
        if schema.get("properties").is_none() {
            return Err(ToolError::Port(
                "配置 schema 不可用：装配层没有提供 SettingPort::schema()".to_string(),
            ));
        }
        let writable = schema
            .get("properties")
            .and_then(Value::as_object)
            .map_or(0, serde_json::Map::len);
        Ok(ToolOutput::ok(
            json!({
                "scope": "ai_writable_schema",
                "writable_key_count": writable,
                "schema": schema,
            }),
            format!("读取配置 schema（{writable} 个 AI 可写键）"),
        ))
    });

    registry.register(ToolSpec::read_only(
        GET_CONFIG_SCHEMA,
        "读取启动器配置的 JSON Schema（只含 AI 可写键）。每个键带类型、取值范围、说明与 `x-ai-sensitivity` 敏感等级；凭据类键不会出现。写配置前先看它。",
        object(&[], &[]),
        handler,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockSettings;

    fn ctx() -> ToolCtx {
        ToolCtx {
            agent_session_id: "s1".to_string(),
            confirmed: false,
        }
    }

    #[test]
    fn returns_schema_without_credentials() {
        let port: Arc<dyn SettingPort> = Arc::new(MockSettings::new());
        let mut registry = ToolRegistry::new();
        register(&mut registry, port).expect("注册应成功");
        let spec = registry.get(GET_CONFIG_SCHEMA).expect("工具存在").clone();
        let out = (spec.handler)(&ctx(), &json!({})).expect("应返回 schema");
        let text = out.content.to_string();
        assert!(out.content["writable_key_count"].as_u64().unwrap_or(0) > 0);
        assert!(text.contains("x-ai-sensitivity"));
        assert!(!text.contains("ai.api_key"), "凭据键不得进 schema：{text}");
    }
}
