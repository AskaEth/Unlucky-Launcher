//! 参数校验：JSON Schema 2020-12 的**最小子集**实现。
//!
//! 为什么不引 `jsonschema` crate：agent 的 schema 全是自己手写的（[`super::schema`]），
//! 用到的关键字只有 `type/required/properties/additionalProperties/enum/minimum/maximum/
//! minLength/minItems/maxItems/uniqueItems/items`。三百行的校验器换掉一棵依赖树，
//! 对 Win7 单文件形态（D12）值得。
//!
//! 校验失败的文案是给**模型**看的（它会据此改正参数重试），所以要具体：
//! 「参数 `limit` 必须是 1~500 的整数，实际是 2000」，不是 `schema validation failed`。

use serde_json::Value;

/// 校验入口。返回全部错误（一次给全，省模型来回试错）。
///
/// # Errors
/// 参数不符合 schema 时返回人类可读的错误列表。
pub fn validate(schema: &Value, args: &Value) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    validate_at(schema, args, "", &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// 只取第一条错误（日志/审计摘要用）。
#[must_use]
pub fn first_error(schema: &Value, args: &Value) -> Option<String> {
    validate(schema, args)
        .err()
        .and_then(|errors| errors.into_iter().next())
}

fn validate_at(schema: &Value, value: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(schema) = schema.as_object() else {
        return; // `{}` = 任意值
    };

    if let Some(expected) = schema.get("type").and_then(Value::as_str) {
        if !type_matches(expected, value) {
            errors.push(format!(
                "参数 {} 应为 {}，实际是 {}",
                label(path),
                expected,
                type_name(value)
            ));
            return; // 类型都不对，后面的范围检查没有意义
        }
    }

    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        if !allowed.iter().any(|v| v == value) {
            let list: Vec<String> = allowed.iter().map(render).collect();
            errors.push(format!(
                "参数 {} 只能取 [{}]，实际是 {}",
                label(path),
                list.join(", "),
                render(value)
            ));
        }
    }

    check_bounds(schema, value, path, errors);

    match value {
        Value::Object(map) => {
            let required = schema.get("required").and_then(Value::as_array);
            for name in required.into_iter().flatten().filter_map(Value::as_str) {
                if !map.contains_key(name) {
                    errors.push(format!("缺少必填参数 {}", label(&join(path, name))));
                }
            }
            let properties = schema.get("properties").and_then(Value::as_object);
            for (key, entry) in map {
                match properties.and_then(|p| p.get(key)) {
                    Some(sub) => validate_at(sub, entry, &join(path, key), errors),
                    None => {
                        if schema.get("additionalProperties").and_then(Value::as_bool)
                            == Some(false)
                        {
                            let known: Vec<&str> = properties
                                .map(|p| p.keys().map(String::as_str).collect())
                                .unwrap_or_default();
                            errors.push(format!(
                                "参数 {} 不是可接受的参数名（可用：{}）",
                                label(&join(path, key)),
                                if known.is_empty() {
                                    "无".to_string()
                                } else {
                                    known.join(", ")
                                }
                            ));
                        }
                    }
                }
            }
        }
        Value::Array(items) => {
            if let Some(sub) = schema.get("items") {
                for (i, item) in items.iter().enumerate() {
                    validate_at(sub, item, &format!("{path}[{i}]"), errors);
                }
            }
        }
        _ => {}
    }
}

fn check_bounds(
    schema: &serde_json::Map<String, Value>,
    value: &Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    let number = value.as_f64();
    let minimum = schema.get("minimum").and_then(Value::as_f64);
    let maximum = schema.get("maximum").and_then(Value::as_f64);
    if let (Some(n), Some(min), Some(max)) = (number, minimum, maximum) {
        if n < min || n > max {
            errors.push(format!(
                "参数 {} 必须在 {}~{} 之间，实际是 {}",
                label(path),
                render_number(min),
                render_number(max),
                render_number(n)
            ));
            return;
        }
    } else if let (Some(n), Some(min)) = (number, minimum) {
        if n < min {
            errors.push(format!(
                "参数 {} 不能小于 {}，实际是 {}",
                label(path),
                render_number(min),
                render_number(n)
            ));
        }
    } else if let (Some(n), Some(max)) = (number, maximum) {
        if n > max {
            errors.push(format!(
                "参数 {} 不能大于 {}，实际是 {}",
                label(path),
                render_number(max),
                render_number(n)
            ));
        }
    }

    if let Some(text) = value.as_str() {
        if let Some(min_len) = schema.get("minLength").and_then(Value::as_u64) {
            if (text.chars().count() as u64) < min_len {
                errors.push(format!(
                    "参数 {} 不能为空（最少 {} 个字符）",
                    label(path),
                    min_len
                ));
            }
        }
    }

    if let Some(items) = value.as_array() {
        if let Some(max_items) = schema.get("maxItems").and_then(Value::as_u64) {
            if items.len() as u64 > max_items {
                errors.push(format!(
                    "参数 {} 最多 {} 项，实际 {} 项",
                    label(path),
                    max_items,
                    items.len()
                ));
            }
        }
        if let Some(min_items) = schema.get("minItems").and_then(Value::as_u64) {
            if (items.len() as u64) < min_items {
                errors.push(format!(
                    "参数 {} 至少 {} 项，实际 {} 项",
                    label(path),
                    min_items,
                    items.len()
                ));
            }
        }
        if schema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
            if let Some(duplicate) = first_duplicate(items) {
                errors.push(format!(
                    "参数 {} 不能有重复项（{} 出现了多次）",
                    label(path),
                    render(duplicate)
                ));
            }
        }
    }
}

/// 数组里第一个重复出现的元素（`uniqueItems` 用；`null` 也算一个值）。
fn first_duplicate(items: &[Value]) -> Option<&Value> {
    items
        .iter()
        .enumerate()
        .find(|(index, item)| items.iter().take(*index).any(|prev| prev == *item))
        .map(|(_, item)| item)
}

fn type_matches(expected: &str, value: &Value) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        "integer" => value.as_f64().is_some_and(|n| n.fract() == 0.0),
        "number" => value.is_number(),
        _ => true, // 未知关键字不拦（前向兼容）
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.as_f64().is_some_and(|f| f.fract() == 0.0) => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn label(path: &str) -> String {
    if path.is_empty() {
        "（根）".to_string()
    } else {
        format!("`{path}`")
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn render(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{s}\""),
        other => other.to_string(),
    }
}

fn render_number(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::schema::{
        enum_string_array, integer, non_empty_string, object, string_array, string_enum,
    };
    use serde_json::json;

    fn errors(schema: &Value, args: &Value) -> Vec<String> {
        validate(schema, args).unwrap_err()
    }

    #[test]
    fn accepts_valid_and_reports_missing_required() {
        let schema = object(
            &[("session_id", non_empty_string("会话 id"))],
            &["session_id"],
        );
        assert!(validate(&schema, &json!({ "session_id": "20261029T124501-9f3c2a" })).is_ok());
        let errs = errors(&schema, &json!({}));
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("缺少必填参数 `session_id`"), "{errs:?}");
    }

    #[test]
    fn rejects_empty_string_and_wrong_type() {
        let schema = object(
            &[("session_id", non_empty_string("会话 id"))],
            &["session_id"],
        );
        assert!(errors(&schema, &json!({ "session_id": "" }))[0].contains("不能为空"));
        assert!(errors(&schema, &json!({ "session_id": 1 }))[0].contains("应为 string"));
    }

    #[test]
    fn rejects_out_of_range_and_unknown_keys() {
        let schema = object(
            &[
                ("limit", integer("条数", 1, 500)),
                ("kinds", string_array("类型", 8)),
            ],
            &[],
        );
        let errs = errors(&schema, &json!({ "limit": 2000 }));
        assert!(errs[0].contains("1~500"), "{errs:?}");
        let errs = errors(&schema, &json!({ "limit": 1, "evil": true }));
        assert!(errs[0].contains("不是可接受的参数名"), "{errs:?}");
    }

    #[test]
    fn checks_enum_and_nested_objects() {
        let schema = object(&[("level", string_enum(&["error", "warn"], "级别"))], &[]);
        assert!(validate(&schema, &json!({ "level": "error" })).is_ok());
        assert!(errors(&schema, &json!({ "level": "fatal" }))[0].contains("只能取"));
    }

    #[test]
    fn rejects_duplicate_items_when_unique() {
        let schema = object(
            &[("kinds", enum_string_array(&["stderr", "exit"], "类型", 4))],
            &[],
        );
        let errs = errors(&schema, &json!({ "kinds": ["stderr", "exit", "stderr"] }));
        assert!(errs[0].contains("不能有重复项"), "{errs:?}");
        assert!(validate(&schema, &json!({ "kinds": ["stderr", "exit"] })).is_ok());
    }

    #[test]
    fn integer_type_rejects_fraction() {
        assert!(validate(&json!({ "type": "integer" }), &json!(1.5)).is_err());
        assert!(validate(&json!({ "type": "integer" }), &json!(2.0)).is_ok());
    }
}
