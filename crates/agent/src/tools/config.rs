//! `get_config` / `set_config`（02 文档 §3.1「有权限修改 ULL 各项配置」）。
//!
//! 安全边界都落在这里，但**分级确认的"要不要问"不在这里**：
//! 工具只负责「白名单过滤 → 生成 diff → 需要确认时只报价不执行」，
//! 问不问由编排层按 [`crate::confirm::ConfirmPolicy`] 决定（YOLO 也只改那一处）。
//!
//! 值校验（类型/范围/schema）归 M12 的配置 schema 所有：agent 不复制一份，
//! 否则两处规则迟早漂移（02 文档 §3.1「schema 强校验」）。

use std::sync::Arc;

use serde_json::{json, Value};

use crate::confirm::Sensitivity;
use crate::port::{SettingKey, SettingPort};
use crate::registry::schema::{any, non_empty_string, object, string};
use crate::registry::{
    RegistryError, ToolCtx, ToolError, ToolFn, ToolOutput, ToolRegistry, ToolSpec,
};

/// 读配置。
pub const GET_CONFIG: &str = "get_config";
/// 写配置。
pub const SET_CONFIG: &str = "set_config";
/// 错误提示里最多列多少个可写键（防"把白名单灌给模型"）。
pub const MAX_LISTED_KEYS: usize = 16;
/// 摘要里值的最大展示长度。
const BRIEF_CHARS: usize = 60;

/// 注册两个配置工具。
///
/// # Errors
/// 工具名重复/schema 非法时返回 [`RegistryError`]。
pub fn register(
    registry: &mut ToolRegistry,
    port: Arc<dyn SettingPort>,
) -> Result<(), RegistryError> {
    registry.register(get_config_spec(Arc::clone(&port)))?;
    registry.register(set_config_spec(port))
}

fn get_config_spec(port: Arc<dyn SettingPort>) -> ToolSpec {
    let handler: ToolFn = Arc::new(move |_ctx: &ToolCtx, args: &Value| {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|p| !p.is_empty());
        match path {
            None => {
                let summary = port.summary();
                let writable = port.writable_keys();
                Ok(ToolOutput::ok(
                    json!({
                        "scope": "all",
                        "values": summary,
                        "writable_keys": writable.iter().map(describe_key).collect::<Vec<_>>(),
                    }),
                    format!(
                        "读取全部配置概要（{} 个键，其中 {} 个 AI 可写）",
                        summary.as_object().map_or(0, serde_json::Map::len),
                        writable.len()
                    ),
                ))
            }
            Some(path) => {
                // 凭据类键一律不读：值会进 ToolOutput.content → 回灌模型 → 发往远端 LLM，
                // 违反本 crate 的硬边界（AI 永不接触账户凭据，见 `lib.rs`）。
                // 这是 agent 侧的第二道防线：第一道是 M12 不把凭据放白名单/概要。
                if crate::redact::is_sensitive_key(path) {
                    return Err(ToolError::NotAllowed(format!(
                        "`{path}` 是凭据类配置，AI 不可读取也不可写入；请让用户自己在设置页查看。"
                    )));
                }
                let value = port.get(path).map_err(ToolError::Port)?;
                Ok(ToolOutput::ok(
                    json!({ "scope": "key", "path": path, "value": value }),
                    format!("读取配置 `{path}` = {}", brief(&value)),
                ))
            }
        }
    });

    ToolSpec::read_only(
        GET_CONFIG,
        "读取启动器配置。不带 path 返回全部配置概要（含 AI 可写键及其敏感等级）；带 path 返回单个键的值。",
        object(
            &[(
                "path",
                string("配置键点分路径，如 `download.concurrency`；省略则返回全部概要"),
            )],
            &[],
        ),
        handler,
    )
}

fn set_config_spec(port: Arc<dyn SettingPort>) -> ToolSpec {
    let handler: ToolFn =
        Arc::new(move |ctx: &ToolCtx, args: &Value| apply_set_config(port.as_ref(), ctx, args));

    ToolSpec::mutating(
        SET_CONFIG,
        "修改启动器配置（仅限白名单内的键）。普通键立即生效；敏感键（Java 路径/数据目录/代理/AI 端点）会先返回 needs_confirmation，等用户确认后才会真正写入。值必须符合该键的类型与范围。",
        object(
            &[
                ("path", non_empty_string("配置键点分路径，必须是 get_config 列出的可写键")),
                ("value", any()),
            ],
            &["path", "value"],
        ),
        handler,
    )
}

/// `set_config` 的实现（独立成函数便于单测直接调用）。
///
/// # Errors
/// 键不在白名单、值缺失、端口写入失败时返回 [`ToolError`]。
pub fn apply_set_config(
    port: &dyn SettingPort,
    ctx: &ToolCtx,
    args: &Value,
) -> Result<ToolOutput, ToolError> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| ToolError::NotAllowed("缺少参数 `path`".to_string()))?
        .to_string();
    let value = args
        .get("value")
        .cloned()
        .ok_or_else(|| ToolError::NotAllowed("缺少参数 `value`".to_string()))?;

    let keys = port.writable_keys();
    let key = keys
        .iter()
        .find(|k| k.path == path)
        .ok_or_else(|| ToolError::NotAllowed(not_writable_message(&keys, &path)))?;

    // 旧值读不到不影响写（首次设置），只是 diff 少一半。
    let old = port.get(&path).ok();

    if !ctx.confirmed && key.sensitivity == Sensitivity::High {
        let diff = json!({
            "path": path,
            "old": old,
            "new": value,
            "sensitivity": key.sensitivity,
            "description": key.description,
        });
        return Ok(ToolOutput::needs_confirmation(
            format!(
                "AI 请求把 `{path}` 从 {} 改为 {}（{}）",
                brief_opt(&old),
                brief(&value),
                sensitivity_label(key.sensitivity)
            ),
            diff,
            key.sensitivity,
        ));
    }

    let change = port.set(&path, &value).map_err(ToolError::Port)?;
    Ok(ToolOutput::ok(
        json!({
            "status": "written",
            "path": change.path,
            "old": change.old,
            "new": change.new,
            "sensitivity": key.sensitivity,
        }),
        format!(
            "已把 `{}` 从 {} 改为 {}",
            change.path,
            brief_opt(&change.old),
            brief(&change.new)
        ),
    ))
}

/// 白名单拒绝的文案：说清"不能写"并给出可写键（模型会据此改用正确路径）。
fn not_writable_message(keys: &[SettingKey], path: &str) -> String {
    let listed: Vec<&str> = keys
        .iter()
        .take(MAX_LISTED_KEYS)
        .map(|k| k.path.as_str())
        .collect();
    let rest = keys.len().saturating_sub(listed.len());
    let tail = if rest > 0 {
        format!("，另有 {rest} 个键见 get_config 概要")
    } else {
        String::new()
    };
    format!(
        "配置键 `{path}` 不在 AI 可写白名单内。可写键（{} 个）：{}{tail}",
        keys.len(),
        listed.join("、")
    )
}

fn describe_key(key: &SettingKey) -> Value {
    json!({
        "path": key.path,
        "sensitivity": key.sensitivity,
        "value_type": key.value_type,
        "description": key.description,
    })
}

fn sensitivity_label(sensitivity: Sensitivity) -> &'static str {
    match sensitivity {
        Sensitivity::Low => "普通",
        Sensitivity::Medium => "中等（静默但审计）",
        Sensitivity::High => "敏感，需确认",
    }
}

fn brief(value: &Value) -> String {
    let rendered = match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    truncate(&rendered)
}

fn brief_opt(value: &Option<Value>) -> String {
    value
        .as_ref()
        .map_or_else(|| "（未设置）".to_string(), brief)
}

fn truncate(text: &str) -> String {
    if text.chars().count() <= BRIEF_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(BRIEF_CHARS).collect();
    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockSettings;

    fn ctx(confirmed: bool) -> ToolCtx {
        ToolCtx {
            agent_session_id: "s1".to_string(),
            confirmed,
        }
    }

    #[test]
    fn rejects_keys_outside_whitelist() {
        let port = MockSettings::new();
        let err = apply_set_config(
            &port,
            &ctx(true),
            &json!({ "path": "java.random.thing", "value": 1 }),
        )
        .unwrap_err();
        assert!(matches!(err, ToolError::NotAllowed(_)));
        assert!(err.message().contains("不在 AI 可写白名单内"));
        assert!(err.message().contains("download.concurrency"));
        assert_eq!(port.set_calls(), 0);
    }

    #[test]
    fn normal_key_writes_without_confirmation() {
        let port = MockSettings::new();
        let out = apply_set_config(
            &port,
            &ctx(false),
            &json!({ "path": "download.concurrency", "value": 16 }),
        )
        .unwrap();
        assert!(out.confirmation.is_none());
        assert_eq!(out.content["status"], json!("written"));
        assert_eq!(port.value_of("download.concurrency"), Some(json!(16)));
    }

    #[test]
    fn medium_key_writes_silently_too() {
        // M12：Medium = 静默生效但审计标记，不弹窗
        let port = MockSettings::new();
        let out = apply_set_config(
            &port,
            &ctx(false),
            &json!({ "path": "download.region", "value": "cn" }),
        )
        .unwrap();
        assert!(out.confirmation.is_none());
        assert_eq!(port.set_calls(), 1);
    }

    #[test]
    fn high_key_quotes_first_then_writes_after_confirm() {
        let port = MockSettings::new();
        let args = json!({ "path": "java.selected_path", "value": "D:/jdk17/bin/java.exe" });

        let quoted = apply_set_config(&port, &ctx(false), &args).unwrap();
        let confirmation = quoted.confirmation.expect("High 键应要求确认");
        assert_eq!(confirmation.sensitivity, Sensitivity::High);
        assert_eq!(confirmation.details["new"], json!("D:/jdk17/bin/java.exe"));
        assert_eq!(port.set_calls(), 0, "未确认前不得写入");

        let written = apply_set_config(&port, &ctx(true), &args).unwrap();
        assert!(written.confirmation.is_none());
        assert_eq!(port.set_calls(), 1);
        assert_eq!(
            port.value_of("java.selected_path"),
            Some(json!("D:/jdk17/bin/java.exe"))
        );
    }

    #[test]
    fn locked_credential_key_is_not_writable() {
        // M12：ai.api_key 是 writable=false，不在白名单里 → 任何情况下都拒绝
        let port = MockSettings::new();
        let err = apply_set_config(
            &port,
            &ctx(true),
            &json!({ "path": "ai.api_key", "value": "sk-x" }),
        )
        .unwrap_err();
        assert!(matches!(err, ToolError::NotAllowed(_)));
        assert_eq!(port.set_calls(), 0);
    }

    #[test]
    fn port_error_is_passed_through() {
        let port = MockSettings::new();
        port.fail_set_with("磁盘只读");
        let err = apply_set_config(
            &port,
            &ctx(true),
            &json!({ "path": "appearance.theme_mode", "value": "metro_light" }),
        )
        .unwrap_err();
        assert_eq!(err.code(), "port_error");
        assert!(err.message().contains("磁盘只读"));
    }
}
