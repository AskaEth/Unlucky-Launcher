//! 对外 MCP server（02 文档 §1「双形态共享一个工具内核」的第二形态）。
//!
//! 传输定论（§0）：**Streamable HTTP on 127.0.0.1 是主力**（外部 agent 起 stdio 会变成
//! 双实例抢数据目录锁），**stdio 形态保留**给 llama.cpp server 的内置 MCP client。
//!
//! 与内置客户端的关系：两侧都只吃 [`crate::registry::ToolRegistry`] 的 `(name, description,
//! input_schema)`，并且**都必须走** [`crate::brain::exec::ToolExecutor`]——schema 校验、
//! 分级确认、YOLO、审计一个字都不少。所以「用户在聊天里让 AI 改配置」与「Claude Code
//! 通过 MCP 改配置」是同一条路径（02 §1 的铁律）。
//!
//! 安全边界（MCP 场景的差别只有一条）：
//! - 敏感键（`Sensitivity::High`）默认**拒绝**而不是弹窗——外部客户端没有我们的 UI；
//!   拒绝文案引导用户去设置页确认，或在设置里打开 `ai.yolo`（YOLO 下照记 `yolo=true` 审计）；
//! - 写工具默认不暴露（`agent.mcp.allow_write=false`）：只读子集注册进注册表，
//!   调写工具会得到「没有这个工具 + 可用工具清单」（并留一条 `unknown_tool` 审计）。
//!
//! 依赖面纪律：整个 crate 只有本模块允许 `use rmcp::`（与 `brain::rig` 对 rig 的规则同形）。

pub mod projection;
pub mod sample;
pub mod serve;
pub mod server;

use std::fmt;

pub use sample::{config_samples, ConfigSamples};
pub use serve::{bind, serve_http, serve_http_on, serve_stdio};
pub use server::RegistryServer;

/// 一次 MCP server 运行的配置（由设置键 `agent.mcp.*` + 命令行覆盖合成）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpConfig {
    /// 监听地址。**只允许回环地址**（默认 `127.0.0.1`）：MCP 工具能改配置，不能对外开。
    pub host: String,
    /// Streamable HTTP 端口（默认 47823，设置键 `agent.mcp.port`）。
    pub port: u16,
    /// 访问令牌（`Authorization: Bearer <token>`）；`None` = 不校验（仅限回环监听时允许）。
    pub token: Option<String>,
    /// 是否只暴露只读工具（默认 **true**；`agent.mcp.allow_write=true` 才放开写侧）。
    pub read_only: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 47823,
            token: None,
            read_only: true,
        }
    }
}

impl McpConfig {
    /// `host:port` 形态的监听地址（IPv6 字面量自动加方括号，`::1` 不会拼成 `::1:47823`）。
    #[must_use]
    pub fn socket_addr(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// MCP 端点 URL（客户端配置样例里用的那串）。
    #[must_use]
    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.socket_addr())
    }

    /// 拒绝非回环监听——这是安全铁律的**代码化**，不是文档里的建议。
    ///
    /// # Errors
    /// `host` 不是回环地址时返回 [`McpServeError::NotLoopback`]。
    pub fn ensure_loopback(&self) -> Result<(), McpServeError> {
        let host = self.host.trim();
        if host == "127.0.0.1" || host == "localhost" || host == "::1" {
            return Ok(());
        }
        Err(McpServeError::NotLoopback(host.to_string()))
    }
}

/// MCP server 启动/运行期错误（文案面向用户，可直接进 CLI 输出）。
#[derive(Debug)]
pub enum McpServeError {
    /// 监听地址不是回环地址。
    NotLoopback(String),
    /// 端口被占用（最常见：另一个 `ull agent serve` 已经在跑）。
    PortInUse {
        /// 监听地址（`127.0.0.1:47823`）。
        addr: String,
    },
    /// 其他 IO 失败（权限、端口不足、传输中断）。
    Io(String),
}

impl fmt::Display for McpServeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotLoopback(host) => write!(
                f,
                "拒绝在 `{host}` 上监听：MCP server 只允许回环地址（127.0.0.1 / localhost / ::1）"
            ),
            Self::PortInUse { addr } => write!(
                f,
                "端口已被占用：{addr}。可能另一个 `ull agent serve` 正在运行；\
                 用 `--port <端口>` 或设置键 `agent.mcp.port` 换一个"
            ),
            Self::Io(message) => write!(f, "MCP server 传输失败：{message}"),
        }
    }
}

impl std::error::Error for McpServeError {}

impl From<std::io::Error> for McpServeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_hosts_are_accepted() {
        let mut cfg = McpConfig::default();
        assert!(cfg.ensure_loopback().is_ok());
        cfg.host = "localhost".to_string();
        assert!(cfg.ensure_loopback().is_ok());
        cfg.host = "::1".to_string();
        assert_eq!(cfg.url(), "http://[::1]:47823/mcp");
        assert!(cfg.ensure_loopback().is_ok());
        cfg.host = "0.0.0.0".to_string();
        let err = cfg.ensure_loopback().unwrap_err();
        assert!(matches!(err, McpServeError::NotLoopback(_)));
        assert!(err.to_string().contains("回环地址"));
    }

    #[test]
    fn default_is_read_only_on_the_documented_port() {
        let cfg = McpConfig::default();
        assert!(cfg.read_only);
        assert_eq!(cfg.port, 47823);
        assert_eq!(cfg.url(), "http://127.0.0.1:47823/mcp");
    }

    #[test]
    fn port_in_use_message_is_actionable() {
        let err = McpServeError::PortInUse {
            addr: "127.0.0.1:47823".to_string(),
        };
        let text = err.to_string();
        assert!(text.contains("--port"), "{text}");
        assert!(text.contains("agent.mcp.port"), "{text}");
    }
}
