//! 配置端口：`core::setting` 的直译（**唯一真相源**在 core，这里不复制校验规则）。
//!
//! 三条纪律：
//! 1. 每次调用都重新 `SettingStore::load`——外部 MCP 客户端可能刚改了配置，
//!    缓存一份快照会让审计里的 `old` 骗人；
//! 2. **敏感值一律掩码**（M12 `Sensitivity::High` + agent 的凭据词表两道）：
//!    工具结果会回灌模型、也可能发往远端 LLM，AI 只需要知道「这个键是什么、怎么改」；
//! 3. **落点与 `ull config` 同源**（分流规则仍只在 `crate::context` 一处）：数据根键
//!    读写便携目录那份 `launcher.json`，其余键读写解析后的数据根。只持一份路径时
//!    standard / `--data-dir` 模式下 AI 改 `core.data_mode` 会「写了没反应」
//!    （finding 20261031-worker-chore40-improve-agent-settingport-core-ai）。

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};
use unlucky_agent::confirm::Sensitivity;
use unlucky_agent::port::{PortResult, SettingChange, SettingKey, SettingPort};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::spec::{Sensitivity as CoreSensitivity, ValueKind};
use unlucky_core::setting::store::{SaveOutcome, SettingStore, SkipReason};

use crate::context::is_data_root_key;

/// 掩码占位（与 CLI `config get` 同一串，读输出的人不用学两套）。
pub const MASK: &str = "<redacted>";

/// 把 `core::setting` 包成 agent 的 [`SettingPort`]。
#[derive(Debug, Clone)]
pub struct CoreSetting {
    /// 解析后的数据根那份 `launcher.json`（非数据根键的落点）。
    path: PathBuf,
    /// 便携目录那份 `launcher.json`（数据根键的落点）；与 `path` 重合或拿不到
    /// exe 目录时为 `None`。
    core_path: Option<PathBuf>,
}

impl CoreSetting {
    /// 只指向数据根那份 `<数据根>/.ULL/config/launcher.json`（两份落点重合时够用）。
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::split(path, None)
    }

    /// 数据根那份 + 便携目录那份（`Ctx::portable_launcher_config`，与数据根解析同源）。
    ///
    /// 两份路径重合（便携模式）时只留一份，免得同一个文件被加载两次。
    #[must_use]
    pub fn split(path: impl Into<PathBuf>, core_path: Option<PathBuf>) -> Self {
        let path = path.into();
        let core_path = core_path.filter(|core| core.as_path() != path.as_path());
        Self { path, core_path }
    }

    /// 某个键的落点文件（数据根键 → 便携目录那份，其余 → 数据根那份）。
    fn path_for(&self, key: &str) -> &Path {
        match &self.core_path {
            Some(core) if is_data_root_key(key) => core.as_path(),
            _ => self.path.as_path(),
        }
    }

    /// 按落点加载 store（读写都用它，`get`/`set` 不会各看一份文件）。
    fn store_for(&self, key: &str) -> SettingStore {
        SettingStore::load(self.path_for(key), launcher_schema())
    }

    /// 数据根那份（schema / 白名单等与落点无关的元数据都从它出）。
    fn store(&self) -> SettingStore {
        SettingStore::load(&self.path, launcher_schema())
    }
}

/// `summary` 里某个键的**原始**取值来源（掩码在 `summary` 里做）：数据根键取便携目录
/// 那份——也就是 `Ctx::resolve` 读的同一份，否则概要和「真正生效值」是两套真相。
fn raw_value<'a>(
    data: &'a SettingStore,
    core: Option<&'a SettingStore>,
    key: &str,
) -> Option<&'a Value> {
    match core {
        Some(core) if is_data_root_key(key) => core.effective(key),
        _ => data.effective(key),
    }
}

fn sensitivity_of(value: CoreSensitivity) -> Sensitivity {
    match value {
        CoreSensitivity::Low => Sensitivity::Low,
        CoreSensitivity::Medium => Sensitivity::Medium,
        CoreSensitivity::High => Sensitivity::High,
    }
}

fn kind_label(kind: &ValueKind) -> &'static str {
    match kind {
        ValueKind::Bool => "boolean",
        ValueKind::Int { .. } => "integer",
        ValueKind::Float { .. } => "number",
        ValueKind::Text { .. } => "string",
        ValueKind::Path => "path",
        ValueKind::Enum(_) => "enum",
        ValueKind::TextList => "string[]",
        ValueKind::EnumList(_) => "enum[]",
        ValueKind::Any => "json",
    }
}

/// 该键的值要不要在 AI 侧掩码：凭据词表命中，或 schema 标 `High`。
fn mask(value: Value, sensitive: bool) -> Value {
    if sensitive {
        json!(MASK)
    } else {
        value
    }
}

impl SettingPort for CoreSetting {
    /// 全量概要：每个键的生效值都取自**它自己的落点文件**（数据根键取便携目录那份）。
    fn summary(&self) -> Value {
        let store = self.store();
        let core = self
            .core_path
            .as_ref()
            .map(|path| SettingStore::load(path, launcher_schema()));
        let schema = store.doc().schema().clone();
        let mut map = Map::new();
        for spec in schema.specs() {
            if spec.is_deprecated() {
                continue;
            }
            let sensitive = spec.ai.sensitivity == CoreSensitivity::High
                || unlucky_agent::redact::is_sensitive_key(spec.key);
            let value = raw_value(&store, core.as_ref(), spec.key)
                .cloned()
                .unwrap_or(Value::Null);
            map.insert(spec.key.to_string(), mask(value, sensitive));
        }
        Value::Object(map)
    }

    fn get(&self, path: &str) -> PortResult<Value> {
        if unlucky_agent::redact::is_sensitive_key(path) {
            return Err(format!(
                "`{path}` 是凭据类配置，AI 不可读取也不可写入；请让用户自己在设置页查看"
            ));
        }
        let store = self.store_for(path);
        let schema = store.doc().schema().clone();
        let Some(value) = store.effective(path).cloned() else {
            return Err(format!("配置键 `{path}` 不在 schema 里"));
        };
        let sensitive = schema.sensitivity(path) == Some(CoreSensitivity::High);
        Ok(mask(value, sensitive))
    }

    fn set(&self, path: &str, value: &Value) -> PortResult<SettingChange> {
        let file = self.path_for(path);
        let mut store = self.store_for(path);
        let schema = store.doc().schema().clone();
        let sensitive = schema.sensitivity(path) == Some(CoreSensitivity::High);
        let old = store
            .effective(path)
            .cloned()
            .map(|old| mask(old, sensitive));
        store
            .set(path, value.clone())
            .map_err(|error| error.to_string())?;
        match store.save() {
            Ok(SaveOutcome::Saved(_)) | Ok(SaveOutcome::Skipped(SkipReason::NotDirty)) => {}
            // 只读/不可信：**不能**报成功——外部客户端会以为改完了。
            Ok(SaveOutcome::Skipped(reason)) => {
                return Err(format!(
                    "{} 当前不可写（{}）；请让用户检查文件权限或内容",
                    file.display(),
                    reason.as_str()
                ))
            }
            Err(error) => return Err(format!("写 {} 失败：{error}", file.display())),
        }
        Ok(SettingChange {
            path: path.to_string(),
            old,
            new: mask(value.clone(), sensitive),
        })
    }

    fn writable_keys(&self) -> Vec<SettingKey> {
        let store = self.store();
        let schema = store.doc().schema().clone();
        schema
            .specs()
            .iter()
            .filter(|spec| spec.ai.writable && !spec.is_deprecated())
            .map(|spec| SettingKey {
                path: spec.key.to_string(),
                sensitivity: sensitivity_of(spec.ai.sensitivity),
                value_type: kind_label(&spec.kind).to_string(),
                description: spec.doc.to_string(),
            })
            .collect()
    }

    fn schema(&self) -> Value {
        // core 的 `to_json_schema()` 只遍历 `ai.writable && !deprecated`，
        // 凭据键（`locked`）天然不在其中——这正是 `get_config_schema` 要的东西。
        self.store().doc().schema().to_json_schema()
    }
}

#[cfg(test)]
mod tests;
