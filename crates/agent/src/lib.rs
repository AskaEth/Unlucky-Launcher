//! Unlucky-Launcher AI Agent（`unlucky_agent`）。
//!
//! 形态定案（决策 D4）：**内置 Agent 客户端 + 对外的 MCP server 双形态，共享同一个工具内核**。
//! 设计定稿：`DevDocs/PlanDoc/02-ai-agent-design.md`。
//!
//! 三条不能破的边界：
//! 1. 工具只定义一次（[`registry`]），内置客户端与 MCP server 都把同一个 `ToolSpec` 投影出去；
//! 2. AI 是 Port 不是 Service——LLM 不可用时规则引擎仍能诊断；
//! 3. AI 永不接触账户凭据；写配置受 schema 白名单约束并留审计。
//!
//! 与 core 的关系：agent 依赖 core 的公共 API，core 不知道 agent 的存在。
//! 落地方式是把需要 core 的能力收成[`port`]里的五个 trait（配置读写 / 会话日志 / 崩溃诊断 /
//! 实例与 Java / 账户），由装配层用 core 的实现填充——这样 M12/M14/M15/M18/M31/M33 未合并时
//! agent 也能独立编译、独立测试。装配处有两条：`crates/app`（GUI）与 `ull agent serve`（headless）。
//!
//! 当前状态：工具内核 + 对话循环 + 审计（P0 定稿）+ **对外 MCP server**（M39）已落地。
//! 两个外部依赖都收口在单文件：[`brain::rig`] 对 rig、[`mcp`] 对 rmcp（3.8.x）——
//! 它们升 breaking 时只有那两个模块编译失败，上层的循环/编排/审计一行不用动。
//!
//! 模块分工：
//! - [`registry`] 工具注册表（唯一真相源 + 双投影载荷）；
//! - [`tools`] 工具 v1 实现（配置读写 / 会话日志 / 崩溃诊断）；
//! - [`brain`] Provider 抽象 + 对话循环 + 工具编排；
//! - [`mcp`] 对外 MCP server（rmcp 双投影 + stdio / Streamable HTTP 传输）；
//! - [`confirm`] 分级确认协议与 YOLO 策略；
//! - [`audit`] 每次工具调用的审计留痕；
//! - [`event`] 面向 UI 的事件流；
//! - [`port`] 与 core 的接触面（依赖倒置）；
//! - [`redact`] 密钥/主目录脱敏；
//! - [`clock`] 可注入时间源（审计/事件用）；
//! - [`testing`] 五个端口的 mock 实现（供本 crate、app 与 MCP 契约测试复用）。

#![forbid(unsafe_code)]

pub mod audit;
pub mod brain;
pub mod clock;
pub mod confirm;
pub mod event;
pub mod mcp;
pub mod port;
pub mod redact;
pub mod registry;
pub mod testing;
pub mod tools;
