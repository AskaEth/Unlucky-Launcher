//! 内存配置端口（`SettingPort` 的测试替身）。

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde_json::{json, Map, Value};

use crate::confirm::Sensitivity;
use crate::port::{PortResult, SettingChange, SettingKey, SettingPort};

/// 内存配置端口。
#[derive(Debug, Default)]
pub struct MockSettings {
    keys: Vec<SettingKey>,
    values: Mutex<BTreeMap<String, Value>>,
    fail_set: Mutex<Option<String>>,
    set_calls: Mutex<usize>,
}

/// 与 `PlanDoc/04` §3 / M12 `setting-接口约定.md` §8 的键表一致的默认白名单
/// （覆盖三档敏感度；`ai.api_key` 这类凭据是 `writable=false`，**故意不在这里**）。
fn default_keys() -> Vec<SettingKey> {
    fn key(
        path: &str,
        sensitivity: Sensitivity,
        value_type: &str,
        description: &str,
    ) -> SettingKey {
        SettingKey {
            path: path.to_string(),
            sensitivity,
            value_type: value_type.to_string(),
            description: description.to_string(),
        }
    }
    vec![
        key(
            "appearance.theme_mode",
            Sensitivity::Low,
            "enum",
            "界面主题",
        ),
        key(
            "download.concurrency",
            Sensitivity::Low,
            "integer",
            "下载并发（1~256）",
        ),
        key(
            "java.default_memory_mb",
            Sensitivity::Low,
            "integer",
            "最大内存（512~65536 MB）",
        ),
        key("download.region", Sensitivity::Medium, "enum", "下载源区域"),
        key("ai.model", Sensitivity::Medium, "text", "AI 模型名"),
        key(
            "java.selected_path",
            Sensitivity::High,
            "path",
            "Java 可执行文件路径",
        ),
        key("download.proxy_url", Sensitivity::High, "text", "HTTP 代理"),
        key("ai.base_url", Sensitivity::High, "text", "AI 端点"),
        key(
            "core.data_mode",
            Sensitivity::High,
            "enum",
            "数据目录模式（portable/standard/custom）",
        ),
    ]
}

impl MockSettings {
    /// 用默认白名单 + 一批初值造一个。
    #[must_use]
    pub fn new() -> Self {
        let mut values = BTreeMap::new();
        values.insert("appearance.theme_mode".to_string(), json!("metro_dark"));
        values.insert("download.concurrency".to_string(), json!(8));
        values.insert("java.default_memory_mb".to_string(), json!(2048));
        Self {
            keys: default_keys(),
            values: Mutex::new(values),
            fail_set: Mutex::new(None),
            set_calls: Mutex::new(0),
        }
    }

    /// 让接下来的 `set` 全部失败（测错误透传）。
    pub fn fail_set_with(&self, message: &str) {
        if let Ok(mut slot) = self.fail_set.lock() {
            *slot = Some(message.to_string());
        }
    }

    /// 当前值（断言「未确认时没有写进去」）。
    #[must_use]
    pub fn value_of(&self, path: &str) -> Option<Value> {
        self.values.lock().ok().and_then(|v| v.get(path).cloned())
    }

    /// 直接塞一个值，模拟 M12 真实实现里存在的**任意键**（含凭据键）。
    ///
    /// 白名单以外的键也能塞——测「`get_config` 不把凭据读给模型」需要它
    /// （`ai.api_key` 是 `writable=false`，故意不在白名单里）。
    pub fn seed_value(&self, path: &str, value: Value) {
        if let Ok(mut values) = self.values.lock() {
            values.insert(path.to_string(), value);
        }
    }

    /// 成功写入次数。
    #[must_use]
    pub fn set_calls(&self) -> usize {
        self.set_calls.lock().map(|n| *n).unwrap_or(0)
    }
}

impl SettingPort for MockSettings {
    fn summary(&self) -> Value {
        let values = self.values.lock().map(|v| v.clone()).unwrap_or_default();
        let mut map = serde_json::Map::new();
        for key in &self.keys {
            let value = values.get(&key.path).cloned().unwrap_or(Value::Null);
            map.insert(key.path.clone(), value);
        }
        Value::Object(map)
    }

    fn get(&self, path: &str) -> PortResult<Value> {
        self.values
            .lock()
            .ok()
            .and_then(|v| v.get(path).cloned())
            .ok_or_else(|| format!("配置键 `{path}` 没有值"))
    }

    fn set(&self, path: &str, value: &Value) -> PortResult<SettingChange> {
        if let Ok(slot) = self.fail_set.lock() {
            if let Some(message) = slot.as_ref() {
                return Err(message.clone());
            }
        }
        let old = self.get(path).ok();
        if let Ok(mut values) = self.values.lock() {
            values.insert(path.to_string(), value.clone());
        }
        if let Ok(mut calls) = self.set_calls.lock() {
            *calls += 1;
        }
        Ok(SettingChange {
            path: path.to_string(),
            old,
            new: value.clone(),
        })
    }

    fn writable_keys(&self) -> Vec<SettingKey> {
        self.keys.clone()
    }

    fn schema(&self) -> Value {
        // 与 `core::setting::catalog::SettingSchema::to_json_schema` 同形：
        // 只含白名单键 + `x-ai-sensitivity`，凭据键天然缺席。
        let mut properties = Map::new();
        for key in &self.keys {
            properties.insert(
                key.path.clone(),
                json!({
                    "type": key.value_type,
                    "description": key.description,
                    "x-ai-sensitivity": key.sensitivity,
                }),
            );
        }
        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "ull.launcher",
            "type": "object",
            "additionalProperties": false,
            "properties": properties,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_excludes_credential_keys() {
        let schema = MockSettings::new().schema();
        let json = schema.to_string();
        assert!(
            !json.contains("ai.api_key"),
            "schema 不得包含凭据键：{json}"
        );
        assert!(json.contains("download.concurrency"));
    }
}
