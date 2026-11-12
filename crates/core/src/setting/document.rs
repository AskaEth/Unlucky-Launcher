//! 配置文档模型：嵌套点路径取值、脏字段追踪、未知字段保留、`$schema` 保留。
//!
//! 出处：`DevDocs/analyse/07-settings-packaging-update.md` §1.2-§1.3（HMCL `DirtyTracker` +
//! `unknownFields` + `RawPreservingProperty`）与 `DevDocs/PlanDoc/04-data-and-config.md` §3。
//!
//! 三个关键取舍：
//! 1. **默认值不进文件**——只有「文件里读到的键」和「本次显式设过的键」会出现在落盘视图里，
//!    因此改默认值不需要写迁移代码；
//! 2. **未知字段整棵保留**——旧版本打开新版本写的文件不会丢配置；
//! 3. **坏值原样保留**——单个键解析/校验失败只记 `LoadProblem`，不丢数据、不整文件报废。

use std::collections::BTreeSet;
use std::fmt;

use serde_json::{Map, Value};

use super::catalog::SettingSchema;
use super::error::{SettingError, SettingResult};
use super::json_path::{collect_unknown, flatten, insert_at, lookup, merge_maps, segments};
use super::schema::{
    SchemaCompat, SchemaRef, SchemaVersion, SCHEMA_KEY, SCHEMA_URL_PREFIX, SCHEMA_VERSION_KEY,
};

/// 落盘视图模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// 完整视图：`$schema` + `schemaVersion` + 已读/已设键 + 未知键（保存用，无损）。
    Full,
    /// 只含本次脏字段（AI 增量 / IPC 同步 / 审计用）。
    DirtyOnly,
}

/// 加载时发现的问题（都不致命，配置照常用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadProblem {
    /// 文件里有、schema 不认识的键（整棵子树原样保留）。
    UnknownKey {
        /// 点分键；陌生子树按**最上层**节点报一次（不做逐叶展开）。
        key: String,
    },
    /// 已知键但值不合法（原样保留，等用户/AI 修）。
    InvalidValue {
        /// 点分键。
        key: String,
        /// 原因。
        reason: String,
    },
    /// 已知键的父路径类型不符（如 `download` 本该是对象却是字符串）。
    InvalidShape {
        /// 点分键。
        key: String,
        /// 原因。
        reason: String,
    },
    /// `schemaVersion` 镜像与 `$schema` 尾部不一致（以 `$schema` 为准）。
    SchemaVersionMismatch {
        /// 文件里写的整数。
        declared: u64,
        /// `$schema` 推出的 major。
        expected: u32,
    },
    /// 文件用的是已废弃键：值已按映射带到替代键，下次保存只写替代键。
    DeprecatedKey {
        /// 已废弃的键。
        key: String,
        /// 替代键。
        replaced_by: &'static str,
    },
    /// `$schema` 本身的状态（缺失 / 非法 / 更高版本等）。
    Schema(SchemaCompat),
    /// 有 `$schema` 字符串但认不出前缀（M40：值照读、文件按只读，不再静默回落默认值）。
    UnknownSchema {
        /// 文件里写的原串。
        url: String,
    },
}

impl fmt::Display for LoadProblem {
    /// 人读一行（CLI 的 `problems` 通道、日志与设置页共用一份文案）。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownKey { key } => write!(f, "{key}：schema 不认识的键（已原样保留）"),
            Self::InvalidValue { key, reason } => write!(f, "{key}：值不合法（{reason}），已保留"),
            Self::InvalidShape { key, reason } => write!(f, "{key}：结构不符（{reason}）"),
            Self::SchemaVersionMismatch { declared, expected } => {
                write!(
                    f,
                    "schemaVersion={declared} 不是 $schema 的 major {expected}"
                )
            }
            Self::DeprecatedKey { key, replaced_by } => write!(f, "{key}：改用 {replaced_by}"),
            Self::Schema(compat) => write!(f, "$schema 状态：{}", compat.as_str()),
            Self::UnknownSchema { url } => {
                write!(f, "$schema 认不出「{url}」，前缀需为 {SCHEMA_URL_PREFIX}")
            }
        }
    }
}

/// 一份配置文件的内存视图。
#[derive(Debug, Clone)]
pub struct SettingDocument {
    schema: SettingSchema,
    version: SchemaVersion,
    values: Map<String, Value>,
    /// 已废弃键的原始值：只供诊断/UI 提示，**永远不落盘**。
    legacy: Map<String, Value>,
    /// 废弃键（点分）清单。
    legacy_keys: BTreeSet<String>,
    dirty: BTreeSet<String>,
    unknown: Map<String, Value>,
    unknown_keys: BTreeSet<String>,
    preserve_schema: Option<String>,
}

impl SettingDocument {
    /// 空文档（全部走默认值），版本取 schema 版本。
    #[must_use]
    pub fn new(schema: SettingSchema) -> Self {
        let version = schema.version();
        Self {
            schema,
            version,
            values: Map::new(),
            legacy: Map::new(),
            legacy_keys: BTreeSet::new(),
            dirty: BTreeSet::new(),
            unknown: Map::new(),
            unknown_keys: BTreeSet::new(),
            preserve_schema: None,
        }
    }

    /// 从文件内容加载；返回文档与问题清单（永不失败）。
    #[must_use]
    pub fn load(schema: &SettingSchema, raw: &Value) -> (Self, Vec<LoadProblem>) {
        let mut problems = Vec::new();
        let mut document = Self::new(schema.clone());

        let original_schema = raw
            .get(SCHEMA_KEY)
            .and_then(Value::as_str)
            .map(str::to_owned);
        let compat = match &original_schema {
            Some(url) => SchemaCompat::check_url(url, schema.schema_ref()),
            None => SchemaCompat::Missing,
        };
        if compat.preserve_schema() {
            document.preserve_schema = original_schema.clone();
        }
        // 认得 `$schema` 键、但前缀解析不出来：别静默按「没有声明」装默认值。
        if let (SchemaCompat::Invalid, Some(url)) = (compat, original_schema.clone()) {
            problems.push(LoadProblem::UnknownSchema { url });
        }
        if let Some(parsed) = original_schema.as_deref().and_then(SchemaRef::parse_url) {
            document.version = parsed.version;
        }
        if let Some(declared) = raw.get(SCHEMA_VERSION_KEY).and_then(Value::as_u64) {
            let expected = document.version.major;
            if declared != u64::from(expected) {
                problems.push(LoadProblem::SchemaVersionMismatch { declared, expected });
            }
        }

        let Some(object) = raw.as_object() else {
            if !raw.is_null() {
                problems.push(LoadProblem::Schema(compat));
            }
            return (document, problems);
        };

        for spec in schema.specs() {
            let path = segments(spec.key);
            let Some(value) = lookup(object, &path) else {
                continue;
            };

            if let Some(deprecation) = spec.deprecated {
                // 废弃键：原文进 legacy（不落盘），值按映射带到替代键（新键已在则新键优先）。
                insert_at(&mut document.legacy, &path, value.clone());
                problems.push(LoadProblem::DeprecatedKey {
                    key: spec.key.to_owned(),
                    replaced_by: deprecation.replaced_by,
                });
                let replacement = segments(deprecation.replaced_by);
                if lookup(object, &replacement).is_none() {
                    let translated = deprecation.translate(value);
                    if schema
                        .validate(deprecation.replaced_by, &translated)
                        .is_err()
                    {
                        problems.push(LoadProblem::InvalidValue {
                            key: deprecation.replaced_by.to_owned(),
                            reason: format!("旧键 {} 的值 {value} 无法映射到新键", spec.key),
                        });
                    }
                    insert_at(&mut document.values, &replacement, translated);
                }
                continue;
            }

            if let Err(err) = schema.validate(spec.key, value) {
                problems.push(LoadProblem::InvalidValue {
                    key: spec.key.to_owned(),
                    reason: err.to_string(),
                });
            }
            insert_at(&mut document.values, &path, value.clone());
        }

        document.legacy_keys = flatten(&document.legacy);

        let unknown = collect_unknown(&[], object, schema, &mut problems);
        document.unknown_keys = flatten(&unknown);
        document.unknown = unknown;

        (document, problems)
    }

    /// 键表。
    #[must_use]
    pub const fn schema(&self) -> &SettingSchema {
        &self.schema
    }

    /// 文档版本（来自文件 `$schema`；迁移后用 [`Self::set_version`] 推进）。
    #[must_use]
    pub const fn version(&self) -> SchemaVersion {
        self.version
    }

    /// 设置文档版本（迁移链推进后调用）。
    pub const fn set_version(&mut self, version: SchemaVersion) {
        self.version = version;
    }

    /// 文件里显式出现过的值（不含默认值）。
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        let path = segments(key);
        lookup(&self.values, &path).or_else(|| lookup(&self.legacy, &path))
    }

    /// 已废弃键在文件里的原文（仅供诊断/UI 提示；不参与落盘）。
    #[must_use]
    pub fn legacy_value(&self, key: &str) -> Option<&Value> {
        lookup(&self.legacy, &segments(key))
    }

    /// 废弃键清单（文件里真的出现过的）。
    pub fn legacy_keys(&self) -> impl Iterator<Item = &str> {
        self.legacy_keys.iter().map(String::as_str)
    }

    /// 生效值：文件值 → schema 默认值（默认值为 `null` 视为未设置）。
    #[must_use]
    pub fn effective(&self, key: &str) -> Option<&Value> {
        self.get(key)
            .or_else(|| self.schema.default_value(key))
            .filter(|value| !value.is_null())
    }

    /// 生效布尔值。
    #[must_use]
    pub fn effective_bool(&self, key: &str) -> Option<bool> {
        self.effective(key).and_then(Value::as_bool)
    }

    /// 生效整数。
    #[must_use]
    pub fn effective_i64(&self, key: &str) -> Option<i64> {
        self.effective(key).and_then(Value::as_i64)
    }

    /// 生效浮点。
    #[must_use]
    pub fn effective_f64(&self, key: &str) -> Option<f64> {
        self.effective(key).and_then(Value::as_f64)
    }

    /// 生效文本。
    #[must_use]
    pub fn effective_str(&self, key: &str) -> Option<&str> {
        self.effective(key).and_then(Value::as_str)
    }

    /// 写一个键（走 schema 校验，并标脏）。
    ///
    /// # Errors
    /// 未定义的键或值不合法时返回 [`SettingError`]。
    pub fn set(&mut self, key: &str, value: Value) -> SettingResult<()> {
        if let Some(replaced_by) = self.schema.replacement_of(key) {
            return Err(SettingError::DeprecatedKey {
                key: key.to_owned(),
                replaced_by,
            });
        }
        self.schema.validate(key, &value)?;
        insert_at(&mut self.values, &segments(key), value);
        self.dirty.insert(key.to_owned());
        Ok(())
    }

    /// 是否有未落盘的改动。
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// 脏键清单。
    pub fn dirty_keys(&self) -> impl Iterator<Item = &str> {
        self.dirty.iter().map(String::as_str)
    }

    /// 把当前所有已读/已设键全部标脏（迁移后强制回写用）。
    ///
    /// 废弃键跳过：它们只做读兼容、不落盘，不需要回写。
    pub fn mark_all_dirty(&mut self) {
        for spec in self.schema.specs() {
            if spec.is_deprecated() {
                continue;
            }
            if self.get(spec.key).is_some() {
                self.dirty.insert(spec.key.to_owned());
            }
        }
    }

    /// 清脏（保存成功后调用）。
    pub fn mark_clean(&mut self) {
        self.dirty.clear();
    }

    /// 未知键（点分）清单。
    pub fn unknown_keys(&self) -> impl Iterator<Item = &str> {
        self.unknown_keys.iter().map(String::as_str)
    }

    /// 文件里原有的 `$schema`（仅 `preserve_schema` 状态下有值）。
    #[must_use]
    pub fn preserve_schema(&self) -> Option<&str> {
        self.preserve_schema.as_deref()
    }

    /// 显式指定保存时沿用的 `$schema` 字符串。
    pub fn set_preserve_schema(&mut self, url: Option<String>) {
        self.preserve_schema = url;
    }

    /// 落盘视图。
    #[must_use]
    pub fn render(&self, mode: RenderMode) -> Value {
        let mut root = Map::new();
        root.insert(SCHEMA_KEY.to_owned(), Value::String(self.schema_url()));
        root.insert(
            SCHEMA_VERSION_KEY.to_owned(),
            Value::from(self.version.major),
        );
        match mode {
            RenderMode::Full => {
                merge_maps(&mut root, &self.values);
                merge_maps(&mut root, &self.unknown);
            }
            RenderMode::DirtyOnly => {
                let patch = self.dirty_patch_map();
                merge_maps(&mut root, &patch);
            }
        }
        Value::Object(root)
    }

    /// 完整落盘视图（等价 `render(RenderMode::Full)`）。
    #[must_use]
    pub fn to_value(&self) -> Value {
        self.render(RenderMode::Full)
    }

    /// 只含脏字段的嵌套对象。
    #[must_use]
    pub fn dirty_patch(&self) -> Value {
        Value::Object(self.dirty_patch_map())
    }

    fn dirty_patch_map(&self) -> Map<String, Value> {
        let mut patch = Map::new();
        for key in &self.dirty {
            if let Some(value) = lookup(&self.values, &segments(key)) {
                insert_at(&mut patch, &segments(key), value.clone());
            }
        }
        patch
    }

    fn schema_url(&self) -> String {
        self.preserve_schema
            .clone()
            .unwrap_or_else(|| self.schema.schema_ref().url())
    }
}
