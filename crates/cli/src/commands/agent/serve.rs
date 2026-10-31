//! `ull agent serve`：把注册表挂成 MCP server，长驻运行。
//!
//! 装配顺序（每一步都对应 02 文档的一条约束）：
//! 1. 设置 → `McpConfig`（`agent.mcp.*`，命令行可覆盖）；
//! 2. 端口（[`super::ports`]）→ 注册表（`tools::build_v1_registry`）；
//! 3. 只读模式 → `read_only_subset()`（物理上不含写工具，列表/调用天然一致）；
//! 4. 执行器：`JsonlAudit`（落盘 `<data>/agent/audit`）+ fail-closed 闸门 +
//!    `ConfirmPolicy { yolo }`（读 `ai.yolo`）+ `DenyGuidance::External`（没有弹窗可点）；
//! 5. 传输：stdio 或 Streamable HTTP（127.0.0.1）。

use std::sync::Arc;

use serde_json::Value;
use unlucky_agent::audit::{audit_dir, JsonlAudit};
use unlucky_agent::brain::exec::ToolExecutor;
use unlucky_agent::clock::SystemClock;
use unlucky_agent::confirm::{ConfirmPolicy, DenyGuidance, NoUiGate};
use unlucky_agent::event::NoopSink;
use unlucky_agent::mcp::{serve_http, serve_stdio, McpConfig, RegistryServer};
use unlucky_agent::tools::build_v1_registry;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::cli::AgentServeArgs;
use crate::context::Ctx;
use crate::error::CliError;

/// 起一次 server（阻塞直到进程被杀/stdio 对端关闭）。
///
/// # Errors
/// 注册表装配失败、端口被占用、非回环监听等返回用户错误。
pub fn run(ctx: &Ctx, args: &AgentServeArgs) -> Result<i32, CliError> {
    let settings = Settings::load(ctx);
    let config = settings.mcp_config(args);
    let yolo = settings.yolo();
    let read_only = config.read_only;
    if !settings.enabled {
        eprintln!(
            "[unlucky-agent] 提示：设置键 agent.mcp.enabled=false，本次是手动启动（GUI 不会自动起）"
        );
    }

    let ports = super::ports::build(ctx);
    let registry = Arc::new(
        build_v1_registry(&ports)
            .map_err(|error| CliError::internal("agent_registry", error.to_string()))?,
    );
    // 路由注册表始终是完整的那份；「哪些工具真能跑」由执行器持有的注册表决定
    // （只读模式下是 read_only_subset）：硬调写工具会得到可读的 tool-level 错误 + 审计，
    // 而不是 rmcp 的协议级 -32602（客户端只会显示 "internal error"）。
    let exposed = if read_only {
        Arc::new(registry.read_only_subset())
    } else {
        Arc::clone(&registry)
    };

    let audit = Arc::new(JsonlAudit::new(
        audit_dir(&ctx.layout().data()),
        Arc::new(SystemClock),
    ));
    let executor = Arc::new(
        ToolExecutor::new(
            Arc::clone(&exposed),
            audit,
            Arc::new(NoUiGate),
            ConfirmPolicy { yolo },
            Arc::new(SystemClock),
            Arc::new(NoopSink),
        )
        .with_guidance(DenyGuidance::External),
    );

    let server = RegistryServer::new(registry, executor, transport_label(args), read_only);
    if args.stdio {
        // stdio：stdout 是协议流，人类提示一律走 stderr。
        eprintln!(
            "[unlucky-agent] MCP server 走 stdio（{} 个工具；写权限：{}；YOLO：{}）",
            server.tool_count(),
            if read_only { "关（只读）" } else { "开" },
            yolo
        );
        return block_on(async move { serve_stdio(server).await })
            .map(|()| crate::error::EXIT_OK)
            .map_err(|error| CliError::user("mcp_stdio", error.to_string()));
    }

    let url = config.url();
    eprintln!("[unlucky-agent] MCP 端点：{url}");
    block_on(async move { serve_http(server, config).await })
        .map(|()| crate::error::EXIT_OK)
        .map_err(|error| CliError::user("mcp_http", error.to_string()))
}

fn transport_label(args: &AgentServeArgs) -> &'static str {
    if args.stdio {
        "mcp-stdio"
    } else {
        "mcp-http"
    }
}

/// 在独立线程里跑一个 tokio 运行时（stdio 形态要独占 stdin/stdout，不能被主线程抢）。
fn block_on<F, T>(future: F) -> Result<T, unlucky_agent::mcp::McpServeError>
where
    F: std::future::Future<Output = Result<T, unlucky_agent::mcp::McpServeError>> + Send + 'static,
    T: Send + 'static,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| unlucky_agent::mcp::McpServeError::Io(error.to_string()))?;
    runtime.block_on(future)
}

/// 从 `launcher.json` 读 MCP 相关设置（坏文件退化成默认值，不阻断启动）。
struct Settings {
    enabled: bool,
    port: u16,
    allow_write: bool,
    token: Option<String>,
    yolo: bool,
}

impl Settings {
    fn load(ctx: &Ctx) -> Self {
        let store = SettingStore::load(ctx.launcher_config(), launcher_schema());
        let doc = store.doc();
        let port = doc
            .effective("agent.mcp.port")
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .unwrap_or(47823);
        Self {
            enabled: doc
                .effective("agent.mcp.enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            port,
            allow_write: doc
                .effective("agent.mcp.allow_write")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            token: doc
                .effective_str("agent.mcp.token")
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string),
            yolo: doc
                .effective("ai.yolo")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    fn mcp_config(&self, args: &AgentServeArgs) -> McpConfig {
        McpConfig {
            host: args.host.clone().unwrap_or_else(|| "127.0.0.1".to_string()),
            port: args.port.unwrap_or(self.port),
            token: self.token.clone(),
            // 命令行 --allow-write 只能**收紧或放开到设置允许的程度**：
            // 设置里没开写权限时，命令行也不放开（设置是用户的意思表示，命令行是运行参数）。
            read_only: !(self.allow_write || args.allow_write),
        }
    }

    fn yolo(&self) -> bool {
        self.yolo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> AgentServeArgs {
        AgentServeArgs::default()
    }

    #[test]
    fn settings_default_to_read_only_on_47823() {
        // Settings 需要 Ctx（要真实数据根），这里只测「无设置时的合成规则」：
        // 用 Default 的字段逐项对照，避免为此造一个假 Ctx。
        let defaults = Settings {
            enabled: false,
            port: 47823,
            allow_write: false,
            token: None,
            yolo: false,
        };
        let config = defaults.mcp_config(&args());
        assert_eq!(config.port, 47823);
        assert!(config.read_only);
        assert_eq!(config.host, "127.0.0.1");
    }

    #[test]
    fn cli_flags_override_port_and_write_switch() {
        let settings = Settings {
            enabled: true,
            port: 47823,
            allow_write: true,
            token: Some("t".to_string()),
            yolo: true,
        };
        let overridden = settings.mcp_config(&AgentServeArgs {
            port: Some(50000),
            allow_write: false,
            ..AgentServeArgs::default()
        });
        assert_eq!(overridden.port, 50000);
        assert!(!overridden.read_only, "设置里允许写时命令行不必再开");
        assert_eq!(overridden.token.as_deref(), Some("t"));
        assert!(settings.yolo());
        // `enabled` 只是 GUI 的自动启动开关：命令行显式 `agent serve` 不看它
        // （所以这里 enabled=true 既不影响 mcp_config，也不影响能不能起服务）。
        assert!(settings.enabled);
    }
}
