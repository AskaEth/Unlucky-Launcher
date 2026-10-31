//! 工具的**唯一定义类型**（[`ToolSpec`]）与它的两份投影载荷（[`ToolDefinition`]）。
//!
//! 双投影的支点在 research/02 §6.2：`rig::tool::DynamicTool::new(name, description,
//! parameters: serde_json::Value, ..)` 与 `rmcp::ToolRoute::new_dyn(name, description,
//! schema, handler)` 的入参**都是裸 JSON Schema**。所以本 crate 只维护一份
//! [`ToolSpec`]，由 [`ToolSpec::definition`] 生成两侧共用的 `(name, description, schema)`。
//!
//! 工具实现是**同步**的：端口调用全是本地文件/内存，没有网络 IO；异步只存在于
//! provider 边界（[`crate::brain::provider::ChatProvider`]）。P1 若出现网络类工具
//! （mod 搜索/版本安装）再升成 `Fn -> BoxFuture`，不影响调用方语义。

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::confirm::Sensitivity;

/// 工具的副作用等级（外部 MCP 只读模式、P1 的 `--readonly` 都按它筛）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// 只读：可以给外部 agent、可以在无确认的情况下调用。
    ReadOnly,
    /// 会改用户数据（写配置），受白名单 + 分级确认约束。
    Mutating,
}

/// 工具定义的三元组：**双投影就投这个**。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// 工具名（模型可见，`snake_case`）。
    pub name: String,
    /// 一句话说明（模型可见）。
    pub description: String,
    /// 参数 JSON Schema（2020-12，根类型必须 `object`）。
    pub input_schema: Value,
}

/// 工具调用上下文：编排层注入，工具只读。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCtx {
    /// agent 对话会话 id（审计用；与游戏 session_id 不是一回事）。
    pub agent_session_id: String,
    /// 是否已获确认。**只有编排层能置 true**（YOLO 或用户点了允许），
    /// 工具据此决定「先报价再执行」还是「直接执行」。
    pub confirmed: bool,
}

/// 工具的返回。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    /// 给模型/审计的结构化结果（编排层序列化成 JSON 字符串回灌）。
    pub content: Value,
    /// 一句话摘要（UI 展示 + 审计 summary）。
    pub summary: String,
    /// 非空表示「这次没执行，先要确认」（[`crate::confirm::ConfirmRequest`] 的原料）。
    pub confirmation: Option<Confirmation>,
}

/// 待确认的操作详情（dry-run 的产物）。
#[derive(Debug, Clone, PartialEq)]
pub struct Confirmation {
    /// 一句话请求（「AI 请求把 X 从 A 改为 B」）。
    pub summary: String,
    /// 结构化 diff（`{"path":…,"old":…,"new":…}`）。
    pub details: Value,
    /// 敏感等级（决定 YOLO 能不能跳过）。
    pub sensitivity: Sensitivity,
}

impl ToolOutput {
    /// 成功返回。
    #[must_use]
    pub fn ok(content: Value, summary: impl Into<String>) -> Self {
        Self {
            content,
            summary: summary.into(),
            confirmation: None,
        }
    }

    /// 需要确认（本次**没有**产生副作用）。
    #[must_use]
    pub fn needs_confirmation(
        summary: impl Into<String>,
        details: Value,
        sensitivity: Sensitivity,
    ) -> Self {
        let summary = summary.into();
        Self {
            content: serde_json::json!({
                "status": "needs_confirmation",
                "summary": summary,
            }),
            confirmation: Some(Confirmation {
                summary: summary.clone(),
                details,
                sensitivity,
            }),
            summary,
        }
    }
}

/// 工具错误。**全部面向用户可读**——它们会作为工具结果回到模型，
/// 由模型决定重试还是换方案（rmcp 的 `Ok(CallToolResult::error(..))` 同义，
/// 见 research/02 §3.2：协议级 `Err` 会被客户端吞掉，不能用来报业务错误）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// 参数语义不合法（schema 校验之外的规则，如路径不在白名单）。
    NotAllowed(String),
    /// 找不到对象（会话/键/文件）。
    NotFound(String),
    /// 端口（M12/M15/M18）报错，文本原样透传。
    Port(String),
    /// 工具自身内部错误（不该发生）。
    Internal(String),
}

impl ToolError {
    /// 一句话文案（写日志/审计用）。
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::NotAllowed(m) | Self::NotFound(m) | Self::Port(m) | Self::Internal(m) => m,
        }
    }

    /// 错误码（审计与 UI 分类用）。
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotAllowed(_) => "not_allowed",
            Self::NotFound(_) => "not_found",
            Self::Port(_) => "port_error",
            Self::Internal(_) => "internal",
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for ToolError {}

/// 工具处理函数。
pub type ToolFn = Arc<dyn Fn(&ToolCtx, &Value) -> Result<ToolOutput, ToolError> + Send + Sync>;

/// 一个工具的完整定义 + 实现。
#[derive(Clone)]
pub struct ToolSpec {
    /// 工具名（`snake_case`，同时是 MCP 侧的工具名）。
    pub name: &'static str,
    /// 说明。
    pub description: &'static str,
    /// 参数 schema（根必须 object）。
    pub input_schema: Value,
    /// 副作用等级。
    pub access: Access,
    /// 实现。
    pub handler: ToolFn,
}

impl fmt::Debug for ToolSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolSpec")
            .field("name", &self.name)
            .field("access", &self.access)
            .finish_non_exhaustive()
    }
}

impl ToolSpec {
    /// 建一个只读工具。
    #[must_use]
    pub fn read_only(
        name: &'static str,
        description: &'static str,
        input_schema: Value,
        handler: ToolFn,
    ) -> Self {
        Self {
            name,
            description,
            input_schema,
            access: Access::ReadOnly,
            handler,
        }
    }

    /// 建一个会改数据的工具。
    #[must_use]
    pub fn mutating(
        name: &'static str,
        description: &'static str,
        input_schema: Value,
        handler: ToolFn,
    ) -> Self {
        Self {
            name,
            description,
            input_schema,
            access: Access::Mutating,
            handler,
        }
    }

    /// 投影成两侧共用的 `(name, description, schema)`。
    #[must_use]
    pub fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name.to_string(),
            description: self.description.to_string(),
            input_schema: self.input_schema.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::schema::{non_empty_string, object};
    use serde_json::json;

    fn echo_handler() -> ToolFn {
        Arc::new(|_ctx: &ToolCtx, args: &Value| Ok(ToolOutput::ok(args.clone(), "echo")))
    }

    #[test]
    fn definition_is_a_pure_projection() {
        let spec = ToolSpec::read_only(
            "echo",
            "回显",
            object(&[("text", non_empty_string("文本"))], &["text"]),
            echo_handler(),
        );
        let def = spec.definition();
        assert_eq!(def.name, "echo");
        assert_eq!(def.description, "回显");
        assert_eq!(def.input_schema, spec.input_schema);
        assert_eq!(spec.access, Access::ReadOnly);
    }

    #[test]
    fn confirmation_carries_details() {
        let out = ToolOutput::needs_confirmation(
            "AI 请求修改 x",
            json!({ "path": "x" }),
            Sensitivity::High,
        );
        assert_eq!(out.content["status"], json!("needs_confirmation"));
        let confirmation = out.confirmation.expect("必须有待确认详情");
        assert_eq!(confirmation.sensitivity, Sensitivity::High);
        assert_eq!(confirmation.summary, out.summary);
        assert_eq!(confirmation.details, json!({ "path": "x" }));
    }
}
