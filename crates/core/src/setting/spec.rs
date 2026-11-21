//! 配置键元数据：类型/范围、默认值、AI 可写白名单、敏感分级。
//!
//! 出处：`DevDocs/PlanDoc/04-data-and-config.md` §3（schema 化 + AI 可写键 = schema 子集）、
//! `DevDocs/PlanDoc/02-ai-agent-design.md` §3.1（AI 安全边界：白名单 + 类型/范围校验 + 分级确认）。
//!
//! 这份元数据是「唯一真相源」：UI 设置页、AI 的 `get_config`/`set_config`、审计日志都读它。

use serde_json::{json, Value};

/// 键的值类型与约束。
#[derive(Debug, Clone, PartialEq)]
pub enum ValueKind {
    /// 布尔。
    Bool,
    /// 整数（含闭区间范围）。
    Int {
        /// 下界（含）。
        min: i64,
        /// 上界（含）。
        max: i64,
    },
    /// 浮点（含闭区间范围）。
    Float {
        /// 下界（含）。
        min: f64,
        /// 上界（含）。
        max: f64,
    },
    /// 文本（长度上限，按字符数）。
    Text {
        /// 最大字符数。
        max_len: usize,
    },
    /// 枚举（只允许列出的字面量）。
    Enum(&'static [&'static str]),
    /// 路径（非空字符串；绝对/相对都由业务层决定）。
    Path,
    /// 字符串列表。
    TextList,
    /// 枚举字面量数组：每个元素都必须落在白名单里（`overrides` 这类键用）。
    EnumList(&'static [&'static str]),
    /// 任意 JSON（数据结构本身不参与校验，用于档案列表这类「数据而非配置」的键）。
    Any,
}

impl ValueKind {
    /// 校验；返回中文原因便于直接给用户看。
    pub fn check(&self, value: &Value) -> Result<(), String> {
        match self {
            Self::Bool => value
                .is_boolean()
                .then_some(())
                .ok_or_else(|| format!("期望布尔，实际是 {value}")),
            Self::Int { min, max } => {
                let Some(number) = value.as_i64() else {
                    return Err(format!("期望整数，实际是 {value}"));
                };
                if number < *min || number > *max {
                    return Err(format!("{number} 超出范围 [{min}, {max}]"));
                }
                Ok(())
            }
            Self::Float { min, max } => {
                let Some(number) = value.as_f64() else {
                    return Err(format!("期望数字，实际是 {value}"));
                };
                if number < *min || number > *max {
                    return Err(format!("{number} 超出范围 [{min}, {max}]"));
                }
                Ok(())
            }
            Self::Text { max_len } => {
                let Some(text) = value.as_str() else {
                    return Err(format!("期望文本，实际是 {value}"));
                };
                let len = text.chars().count();
                if len > *max_len {
                    return Err(format!("文本 {len} 字符，超过上限 {max_len}"));
                }
                Ok(())
            }
            Self::Enum(allowed) => {
                let Some(text) = value.as_str() else {
                    return Err(format!("期望枚举字面量，实际是 {value}"));
                };
                if allowed.contains(&text) {
                    Ok(())
                } else {
                    Err(format!("{text:?} 不在 {allowed:?} 之中"))
                }
            }
            Self::Path => match value.as_str() {
                Some(text) if !text.trim().is_empty() => Ok(()),
                _ => Err(format!("期望非空路径字符串，实际是 {value}")),
            },
            Self::TextList => {
                let Some(items) = value.as_array() else {
                    return Err(format!("期望字符串数组，实际是 {value}"));
                };
                if items.iter().all(Value::is_string) {
                    Ok(())
                } else {
                    Err("数组里含非字符串元素".to_owned())
                }
            }
            Self::EnumList(allowed) => {
                let Some(items) = value.as_array() else {
                    return Err(format!("期望枚举字面量数组，实际是 {value}"));
                };
                for item in items {
                    let Some(text) = item.as_str() else {
                        return Err(format!("数组里含非字符串元素: {item}"));
                    };
                    if !allowed.contains(&text) {
                        return Err(format!("{text:?} 不在 {allowed:?} 之中"));
                    }
                }
                Ok(())
            }
            Self::Any => Ok(()),
        }
    }

    /// 转成 JSON Schema 片段（喂给 M16 的 `ToolSpec`）。
    #[must_use]
    pub(crate) fn to_json_schema(&self) -> Value {
        match self {
            Self::Bool => json!({ "type": "boolean" }),
            Self::Int { min, max } => json!({ "type": "integer", "minimum": min, "maximum": max }),
            Self::Float { min, max } => json!({ "type": "number", "minimum": min, "maximum": max }),
            Self::Text { max_len } => json!({ "type": "string", "maxLength": max_len }),
            Self::Enum(allowed) => json!({ "type": "string", "enum": allowed }),
            Self::Path => json!({ "type": "string", "minLength": 1 }),
            Self::TextList => json!({ "type": "array", "items": { "type": "string" } }),
            Self::EnumList(allowed) => {
                json!({ "type": "array", "items": { "type": "string", "enum": allowed } })
            }
            Self::Any => json!({}),
        }
    }
}

// AI 边界类型从 [`super::policy`] 拆出去（`spec.rs` 摸到 400 行硬上限），
// 路径保持不变：`setting::spec::AiPolicy` 仍是可用名。
pub use super::policy::{AiPolicy, Sensitivity};

/// 键废弃信息：替代键 + 旧值映射表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deprecation {
    /// 替代键（写永远走这里）。
    pub replaced_by: &'static str,
    /// 旧值 → 新值映射（读兼容）；表里没有的值原样带过去。
    pub aliases: &'static [(&'static str, &'static str)],
}

impl Deprecation {
    /// 把旧值翻译成新键能接受的值（非字符串或表里没有的值原样返回）。
    #[must_use]
    pub fn translate(&self, legacy: &Value) -> Value {
        match legacy.as_str() {
            Some(text) => self
                .aliases
                .iter()
                .find(|(old, _)| *old == text)
                .map_or_else(|| legacy.clone(), |(_, new)| Value::from(*new)),
            None => legacy.clone(),
        }
    }
}

/// 一个配置键的完整元数据。
#[derive(Debug, Clone, PartialEq)]
pub struct SettingSpec {
    /// 点分键名（如 `download.concurrency`）。
    pub key: &'static str,
    /// 类型与约束。
    pub kind: ValueKind,
    /// 默认值；`Value::Null` 表示「无默认」（如用户必须先选的路径）。
    pub default: Value,
    /// AI 策略。
    pub ai: AiPolicy,
    /// 一句话说明（UI 提示 + AI 工具描述）。
    pub doc: &'static str,
    /// 废弃信息：有值表示这个键只做**读兼容**，写必须走替代键。
    pub deprecated: Option<Deprecation>,
}

impl SettingSpec {
    /// 完全手工构造。
    #[must_use]
    pub const fn new(
        key: &'static str,
        kind: ValueKind,
        default: Value,
        ai: AiPolicy,
        doc: &'static str,
    ) -> Self {
        Self {
            key,
            kind,
            default,
            ai,
            doc,
            deprecated: None,
        }
    }

    /// 标记为「已废弃，读兼容到 `replaced_by`」，并按 `aliases` 翻译旧值。
    ///
    /// 用于键改名/语义变更（如 `ui.theme` → `appearance.theme_mode`，tower 键冲突裁决）：
    /// 旧文件照读、下次保存只写新键，AI 白名单与工具 JSON Schema 自动排除本键。
    #[must_use]
    pub const fn deprecated(
        mut self,
        replaced_by: &'static str,
        aliases: &'static [(&'static str, &'static str)],
    ) -> Self {
        self.deprecated = Some(Deprecation {
            replaced_by,
            aliases,
        });
        self
    }

    /// 该键是否已废弃。
    #[must_use]
    pub const fn is_deprecated(&self) -> bool {
        self.deprecated.is_some()
    }

    /// 布尔键。
    #[must_use]
    pub fn bool(key: &'static str, default: bool, ai: AiPolicy, doc: &'static str) -> Self {
        Self::new(key, ValueKind::Bool, Value::Bool(default), ai, doc)
    }

    /// 整数键（闭区间）。
    #[must_use]
    pub fn int(
        key: &'static str,
        default: i64,
        (min, max): (i64, i64),
        ai: AiPolicy,
        doc: &'static str,
    ) -> Self {
        Self::new(
            key,
            ValueKind::Int { min, max },
            Value::from(default),
            ai,
            doc,
        )
    }

    /// 浮点键（闭区间）。
    #[must_use]
    pub fn float(
        key: &'static str,
        default: f64,
        (min, max): (f64, f64),
        ai: AiPolicy,
        doc: &'static str,
    ) -> Self {
        Self::new(
            key,
            ValueKind::Float { min, max },
            Value::from(default),
            ai,
            doc,
        )
    }

    /// 文本键。
    #[must_use]
    pub fn text(
        key: &'static str,
        default: &str,
        max_len: usize,
        ai: AiPolicy,
        doc: &'static str,
    ) -> Self {
        Self::new(
            key,
            ValueKind::Text { max_len },
            Value::from(default),
            ai,
            doc,
        )
    }

    /// 枚举键。
    #[must_use]
    pub fn enumeration(
        key: &'static str,
        default: &'static str,
        allowed: &'static [&'static str],
        ai: AiPolicy,
        doc: &'static str,
    ) -> Self {
        Self::new(key, ValueKind::Enum(allowed), Value::from(default), ai, doc)
    }

    /// 路径键（无默认值，未设置时为 `null`）。
    #[must_use]
    pub fn path(key: &'static str, ai: AiPolicy, doc: &'static str) -> Self {
        Self::new(key, ValueKind::Path, Value::Null, ai, doc)
    }

    /// 枚举字面量数组键（默认空数组；元素逐个校验）。
    #[must_use]
    pub fn enum_list(
        key: &'static str,
        allowed: &'static [&'static str],
        ai: AiPolicy,
        doc: &'static str,
    ) -> Self {
        Self::new(
            key,
            ValueKind::EnumList(allowed),
            Value::Array(Vec::new()),
            ai,
            doc,
        )
    }

    /// 任意 JSON 键（数据结构不参与校验）。
    #[must_use]
    pub fn any(key: &'static str, ai: AiPolicy, doc: &'static str) -> Self {
        Self::new(key, ValueKind::Any, Value::Null, ai, doc)
    }
}
