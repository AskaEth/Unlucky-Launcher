//! 一份配置文件的键目录：[`SettingSchema`] 把 key → 元数据查表能力收拢在一处。
//!
//! 与 [`super::spec`] 的分工：`spec` 定义「一个键长什么样」，这里定义「一份文件的键集合」，
//! 并对外提供 AI 白名单 / 校验 / 工具 JSON Schema 投影（M16 `ToolSpec` 直接吃这份投影）。

use std::collections::BTreeSet;

use serde_json::{json, Map, Value};

use super::error::{SettingError, SettingResult};
use super::schema::{SchemaId, SchemaRef, SchemaVersion};
use super::spec::{Sensitivity, SettingSpec};

/// 一份配置文件里所有键的集合。
#[derive(Debug, Clone, PartialEq)]
pub struct SettingSchema {
    id: SchemaId,
    version: SchemaVersion,
    specs: Vec<SettingSpec>,
}

impl SettingSchema {
    /// 空 schema。
    #[must_use]
    pub fn new(id: SchemaId, version: SchemaVersion) -> Self {
        Self {
            id,
            version,
            specs: Vec::new(),
        }
    }

    /// 链式加一个键（重复键直接 panic —— 这是编译期就该发现的表写错）。
    #[must_use]
    pub fn with(mut self, spec: SettingSpec) -> Self {
        assert!(
            !self.specs.iter().any(|existing| existing.key == spec.key),
            "重复的配置键: {}",
            spec.key
        );
        self.specs.push(spec);
        self
    }

    /// 种类。
    #[must_use]
    pub const fn id(&self) -> SchemaId {
        self.id
    }

    /// 版本。
    #[must_use]
    pub const fn version(&self) -> SchemaVersion {
        self.version
    }

    /// `$schema` 标识。
    #[must_use]
    pub const fn schema_ref(&self) -> SchemaRef {
        SchemaRef::new(self.id, self.version)
    }

    /// 全部键元数据。
    #[must_use]
    pub fn specs(&self) -> &[SettingSpec] {
        &self.specs
    }

    /// 按键名取元数据。
    #[must_use]
    pub fn spec(&self, key: &str) -> Option<&SettingSpec> {
        self.specs.iter().find(|spec| spec.key == key)
    }

    /// 是否定义了该键。
    #[must_use]
    pub fn is_known(&self, key: &str) -> bool {
        self.spec(key).is_some()
    }

    /// 是否是某个已知键的**前缀路径**（用于区分「嵌套对象」与「陌生整棵子树」）。
    #[must_use]
    pub fn has_prefix(&self, segments: &[&str]) -> bool {
        let mut prefix = segments.join(".");
        prefix.push('.');
        self.specs.iter().any(|spec| spec.key.starts_with(&prefix))
    }

    /// 默认值。
    #[must_use]
    pub fn default_value(&self, key: &str) -> Option<&Value> {
        self.spec(key).map(|spec| &spec.default)
    }

    /// 校验一个键值对。
    ///
    /// # Errors
    /// 未定义的键 → [`SettingError::UnknownKey`]；值不合法 → [`SettingError::InvalidValue`]。
    pub fn validate(&self, key: &str, value: &Value) -> SettingResult<()> {
        let spec = self.spec(key).ok_or_else(|| SettingError::UnknownKey {
            key: key.to_owned(),
        })?;
        spec.kind
            .check(value)
            .map_err(|reason| SettingError::InvalidValue {
                key: key.to_owned(),
                reason,
            })
    }

    /// AI 可写键白名单（供 M16 `set_config` 前置检查）。
    #[must_use]
    pub fn ai_writable_keys(&self) -> Vec<&'static str> {
        self.specs
            .iter()
            .filter(|spec| spec.ai.writable && !spec.is_deprecated())
            .map(|spec| spec.key)
            .collect()
    }

    /// AI 是否允许写该键（未定义的键 = 不允许）。
    #[must_use]
    pub fn accepts_ai_write(&self, key: &str) -> bool {
        self.spec(key)
            .is_some_and(|spec| spec.ai.writable && !spec.is_deprecated())
    }

    /// 已废弃键清单（只读兼容，不再落盘）。
    #[must_use]
    pub fn deprecated_keys(&self) -> Vec<&'static str> {
        self.specs
            .iter()
            .filter(|spec| spec.is_deprecated())
            .map(|spec| spec.key)
            .collect()
    }

    /// 某键的替代键（不是废弃键则 `None`）。
    #[must_use]
    pub fn replacement_of(&self, key: &str) -> Option<&'static str> {
        self.spec(key)?.deprecated.map(|info| info.replaced_by)
    }

    /// 敏感分级。
    #[must_use]
    pub fn sensitivity(&self, key: &str) -> Option<Sensitivity> {
        self.spec(key).map(|spec| spec.ai.sensitivity)
    }

    /// 顶层键名集合（`download.concurrency` → `download`）。
    #[must_use]
    pub fn top_level_keys(&self) -> BTreeSet<&'static str> {
        self.specs
            .iter()
            .filter_map(|spec| spec.key.split('.').next())
            .collect()
    }

    /// 生成给 AI 工具用的 JSON Schema（只含 AI 可写键）。
    #[must_use]
    pub fn to_json_schema(&self) -> Value {
        let mut properties = Map::new();
        for spec in self
            .specs
            .iter()
            .filter(|spec| spec.ai.writable && !spec.is_deprecated())
        {
            let mut property = match spec.kind.to_json_schema() {
                Value::Object(map) => map,
                other => {
                    let mut map = Map::new();
                    map.insert("schema".to_owned(), other);
                    map
                }
            };
            property.insert("description".to_owned(), Value::from(spec.doc));
            property.insert(
                "x-ai-sensitivity".to_owned(),
                Value::from(spec.ai.sensitivity.as_str()),
            );
            if !spec.default.is_null() {
                property.insert("default".to_owned(), spec.default.clone());
            }
            properties.insert(spec.key.to_owned(), Value::Object(property));
        }

        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": format!("ull.{}", self.id.as_str()),
            "type": "object",
            "additionalProperties": false,
            "properties": properties,
        })
    }
}
