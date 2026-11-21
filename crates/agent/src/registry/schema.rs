//! JSON Schema 构造helper。
//!
//! 工具参数 schema 一律**手写 JSON**（research/02 §6.2：`rig` 的 `DynamicTool::new` 与
//! rmcp 的 `ToolRoute::new_dyn` 吃的都是裸 `serde_json::Value`），所以不需要 schemars 派生。
//! 手写的好处是同一份 schema 直接喂两方，坏处是啰嗦——这个模块把啰嗦收敛到几个函数。
//!
//! 生成的 schema 一律按 **JSON Schema 2020-12** 的写法（rmcp 支持该版本；
//! `additionalProperties: false` 让模型少给野参数，[`super::validate`] 会执行它）。

use serde_json::{json, Map, Value};

/// 对象 schema。`properties` 按给定顺序写入（JSON 对象保序，UI 展示稳定）。
#[must_use]
pub fn object(properties: &[(&str, Value)], required: &[&str]) -> Value {
    let mut props = Map::new();
    for (name, schema) in properties {
        props.insert((*name).to_string(), schema.clone());
    }
    json!({
        "type": "object",
        "properties": Value::Object(props),
        "required": required,
        "additionalProperties": false,
    })
}

/// 任意 JSON 值（不校验类型）。
#[must_use]
pub fn any() -> Value {
    json!({})
}

/// 字符串。
#[must_use]
pub fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

/// 非空字符串（`minLength: 1`，省掉一堆「路径不能为空」的运行时判断）。
#[must_use]
pub fn non_empty_string(description: &str) -> Value {
    json!({ "type": "string", "description": description, "minLength": 1 })
}

/// 枚举字符串。
#[must_use]
pub fn string_enum(values: &[&str], description: &str) -> Value {
    json!({ "type": "string", "description": description, "enum": values })
}

/// 布尔。
#[must_use]
pub fn boolean(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

/// 带范围约束的整数。
#[must_use]
pub fn integer(description: &str, minimum: i64, maximum: i64) -> Value {
    json!({
        "type": "integer",
        "description": description,
        "minimum": minimum,
        "maximum": maximum,
    })
}

/// 字符串数组（可选条数上限）。
#[must_use]
pub fn string_array(description: &str, max_items: u64) -> Value {
    json!({
        "type": "array",
        "description": description,
        "items": { "type": "string" },
        "maxItems": max_items,
    })
}

/// 枚举字符串数组（如 `kinds: ["stderr","exit"]`）。
#[must_use]
pub fn enum_string_array(values: &[&str], description: &str, max_items: u64) -> Value {
    json!({
        "type": "array",
        "description": description,
        "items": { "type": "string", "enum": values },
        "maxItems": max_items,
        "uniqueItems": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::validate::validate;

    #[test]
    fn object_records_required_and_forbids_extras() {
        let schema = object(
            &[("path", non_empty_string("键路径")), ("value", any())],
            &["path", "value"],
        );
        assert_eq!(schema["type"], json!("object"));
        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(schema["required"], json!(["path", "value"]));
        assert!(validate(&schema, &json!({ "path": "a", "value": 1 })).is_ok());
    }

    #[test]
    fn integer_builder_carries_bounds() {
        let schema = integer("条数", 1, 500);
        assert_eq!(schema["minimum"], json!(1));
        assert_eq!(schema["maximum"], json!(500));
        assert!(validate(&schema, &json!(0)).is_err());
        assert!(validate(&schema, &json!(500)).is_ok());
    }
}
