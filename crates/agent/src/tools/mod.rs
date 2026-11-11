//! 工具集 v1（02 文档 §3）：配置读写 / 会话与日志 / 崩溃诊断 / 实例与 Java / 账户。
//!
//! 分工边界（重要，评审时按这条看）：
//! - **工具**：参数语义 → 端口调用 → 结构化结果；需要确认时**只报价不执行**。
//! - **编排层**（[`crate::brain::exec`]）：schema 校验、确认闸门、YOLO 判定、审计、事件。
//! - **端口**（[`crate::port`]）：真正的 IO 与领域校验（M12/M14/M15/M18/M31/M33 拥有）。
//!
//! 这样「YOLO」只在一个地方（编排层），工具的测试不需要关心 policy。
//!
//! 双投影：这里的注册表是**唯一真相源**，内置客户端投成 rig `DynamicTool`，
//! 对外 MCP server 投成 rmcp `ToolRoute`（[`crate::mcp`]）。工具数量与顺序对外一致。

pub mod accounts;
pub mod config;
pub mod diagnose;
pub mod game;
pub mod schema_tool;
pub mod session;

use crate::port::Ports;
use crate::registry::{RegistryError, ToolRegistry};

/// 组装 v1 的全部工具。注册顺序 = 模型看到的顺序（只读在前，写在后）。
///
/// 顺序不是随便排的：模型在长工具表里更容易选中靠前的项，所以「先看再改」
/// （`get_config*` 在 `set_config` 之前）是刻意的提示。
///
/// # Errors
/// 工具名重复 / schema 非法时返回 [`RegistryError`]（属于编码错误，装配期就该炸）。
pub fn build_v1_registry(ports: &Ports) -> Result<ToolRegistry, RegistryError> {
    let mut registry = ToolRegistry::new();
    config::register(&mut registry, ports.setting.clone())?;
    schema_tool::register(&mut registry, ports.setting.clone())?;
    game::register(&mut registry, ports.game.clone())?;
    accounts::register(&mut registry, ports.accounts.clone())?;
    session::register(&mut registry, ports.sessions.clone())?;
    diagnose::register(&mut registry, ports.diagnose.clone())?;
    Ok(registry)
}
