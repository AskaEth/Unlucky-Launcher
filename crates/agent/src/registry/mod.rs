//! 工具注册表：整个 agent 的**唯一真相源**。
//!
//! 一个 `ToolSpec`（名称 + 描述 + serde JSON Schema）投影两份：
//! - 内置客户端的 rig `DynamicTool` 视图；
//! - 对外 MCP server 的 rmcp `ToolRouter` 视图。
//!
//! 两份投影必须一致，用契约测试固定（同 name / 同 description / 同 schema）。
//! 设计依据：`DevDocs/PlanDoc/02-ai-agent-design.md` §1。
//!
//! 工具 v1 清单（M16 起，M39 扩到十个）：`get_config` / `set_config` / `get_config_schema` /
//! `list_instances` / `list_versions` / `get_java_status` / `list_accounts` /
//! `list_game_sessions` / `read_game_log` / `diagnose_crash`。
//!
//! 安全策略随工具走：敏感键分级确认、YOLO 模式标记、审计留痕（02 §3.1 / §4b）。
//!
//! ## 落地形态
//!
//! ```text
//! registry（Vec<ToolSpec>）
//!   ├─ definitions() → rig `DynamicTool::new`    （内置 agent，`brain::rig`）
//!   └─ definitions() → rmcp `ToolRoute::new_dyn` （外部 MCP，`mcp::projection`）
//! ```
//!
//! 防漂移的关键一招（research/02 §6.2 建议进 CI）：**契约测试**遍历注册表，断言两侧
//! 投影拿到的 `(name, description, schema)` 逐字节相等。因为两侧都只吃
//! [`ToolDefinition`]，这条约束在本 crate 里天然成立——测试只需要锁住
//! 「definition 是纯投影」+「schema 根必须是 object」+「名字不重复」。
//!
//! 注册表**不持有任何端口**：工具闭包在构造时就捕获了 `Arc<dyn …Port>`，注册表只是索引。

pub mod schema;
pub mod spec;
pub mod validate;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

pub use spec::{
    Access, Confirmation, ToolCtx, ToolDefinition, ToolError, ToolFn, ToolOutput, ToolSpec,
};

/// 注册失败原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// 工具名为空或重复。
    DuplicateName(String),
    /// schema 不合法（根必须是 `type: object`，否则 MCP 侧直接拒收）。
    InvalidSchema {
        /// 工具名。
        tool: String,
        /// 原因。
        reason: String,
    },
    /// 说明为空（模型靠 description 选工具，空说明等于没有这个工具）。
    EmptyDescription(String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateName(name) => write!(f, "工具名重复或为空：`{name}`"),
            Self::InvalidSchema { tool, reason } => {
                write!(f, "工具 `{tool}` 的 schema 不合法：{reason}")
            }
            Self::EmptyDescription(name) => write!(f, "工具 `{name}` 缺少说明"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// 工具注册表。
#[derive(Debug, Default, Clone)]
pub struct ToolRegistry {
    specs: Vec<Arc<ToolSpec>>,
    index: BTreeMap<String, usize>,
}

impl ToolRegistry {
    /// 空注册表。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个工具。注册即校验名/schema/说明，**运行时不再做二次判断**。
    ///
    /// # Errors
    /// 名字重复或为空、schema 根不是 object、说明为空时返回 [`RegistryError`]。
    pub fn register(&mut self, spec: ToolSpec) -> Result<(), RegistryError> {
        let name = spec.name.trim();
        if name.is_empty() || self.index.contains_key(name) {
            return Err(RegistryError::DuplicateName(name.to_string()));
        }
        if spec.description.trim().is_empty() {
            return Err(RegistryError::EmptyDescription(name.to_string()));
        }
        if spec.input_schema.get("type").and_then(|t| t.as_str()) != Some("object") {
            return Err(RegistryError::InvalidSchema {
                tool: name.to_string(),
                reason: "根类型必须是 object（rig 与 rmcp 的入参都是对象 schema）".to_string(),
            });
        }
        self.index.insert(name.to_string(), self.specs.len());
        self.specs.push(Arc::new(spec));
        Ok(())
    }

    /// 按名查找。
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Arc<ToolSpec>> {
        self.index.get(name).and_then(|i| self.specs.get(*i))
    }

    /// 全部工具（按注册顺序，模型看到的顺序稳定）。
    #[must_use]
    pub fn specs(&self) -> &[Arc<ToolSpec>] {
        &self.specs
    }

    /// 工具名列表（错误提示/审计用）。
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.specs.iter().map(|s| s.name).collect()
    }

    /// 工具数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    /// 是否没有工具。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// **投影 1/2 的公共载荷**：`(name, description, input_schema)`。
    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.specs.iter().map(|s| s.definition()).collect()
    }

    /// 只读子集（P1 的 `--readonly` 外部 MCP 模式用）。
    #[must_use]
    pub fn read_only_definitions(&self) -> Vec<ToolDefinition> {
        self.specs
            .iter()
            .filter(|s| s.access == Access::ReadOnly)
            .map(|s| s.definition())
            .collect()
    }

    /// **只读模式**的注册表副本（外部 MCP 默认形态）。
    ///
    /// 返回的是「物理上不含写工具」的新注册表：`tools/list` 里看不到 `set_config`，
    /// 硬调它也会在编排层得到 `unknown_tool`（并留一条审计），而不是静默执行。
    /// 这比「列表过滤 + 调用拦截」两套逻辑更不容易漏——只有一份子集，两侧天然一致。
    #[must_use]
    pub fn read_only_subset(&self) -> Self {
        let mut subset = Self::new();
        for spec in self.specs.iter().filter(|s| s.access == Access::ReadOnly) {
            let name = spec.name.to_string();
            if subset.index.insert(name, subset.specs.len()).is_none() {
                subset.specs.push(Arc::clone(spec));
            }
        }
        subset
    }

    /// 名 → access（装配处检查白名单/只读模式）。
    #[must_use]
    pub fn access_of(&self, name: &str) -> Option<Access> {
        self.get(name).map(|s| s.access)
    }

    /// 去重后的名字集合（契约测试用）。
    #[must_use]
    pub fn unique_names(&self) -> BTreeSet<&str> {
        self.specs.iter().map(|s| s.name).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::schema::{non_empty_string, object};
    use serde_json::{json, Value};

    fn echo(name: &'static str) -> ToolSpec {
        ToolSpec::read_only(
            name,
            "回显",
            object(&[("text", non_empty_string("文本"))], &["text"]),
            Arc::new(|_ctx: &ToolCtx, args: &Value| Ok(ToolOutput::ok(args.clone(), "echo"))),
        )
    }

    #[test]
    fn register_and_lookup() {
        let mut reg = ToolRegistry::new();
        reg.register(echo("echo")).unwrap();
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.names(), vec!["echo"]);
        assert!(reg.get("echo").is_some());
        assert!(reg.get("nope").is_none());
        assert_eq!(reg.access_of("echo"), Some(Access::ReadOnly));
    }

    #[test]
    fn rejects_duplicate_empty_and_non_object_schema() {
        let mut reg = ToolRegistry::new();
        reg.register(echo("echo")).unwrap();
        assert_eq!(
            reg.register(echo("echo")),
            Err(RegistryError::DuplicateName("echo".into()))
        );

        let bad = ToolSpec::read_only(
            "bad",
            "x",
            json!({ "type": "array" }),
            Arc::new(|_, _| Ok(ToolOutput::ok(Value::Null, "x"))),
        );
        assert!(matches!(
            reg.register(bad),
            Err(RegistryError::InvalidSchema { .. })
        ));

        let no_desc = ToolSpec::read_only(
            "nodesc",
            "  ",
            json!({ "type": "object" }),
            Arc::new(|_, _| Ok(ToolOutput::ok(Value::Null, "x"))),
        );
        assert!(matches!(
            reg.register(no_desc),
            Err(RegistryError::EmptyDescription(_))
        ));
    }

    #[test]
    fn definitions_are_stable_and_filterable() {
        let mut reg = ToolRegistry::new();
        reg.register(echo("a")).unwrap();
        reg.register(ToolSpec::mutating(
            "b",
            "写",
            json!({ "type": "object" }),
            Arc::new(|_, _| Ok(ToolOutput::ok(Value::Null, "b"))),
        ))
        .unwrap();
        assert_eq!(reg.definitions().len(), 2);
        assert_eq!(reg.read_only_definitions().len(), 1);
        assert_eq!(reg.definitions()[0].name, "a");
        assert_eq!(reg.unique_names().len(), 2);
    }
}
