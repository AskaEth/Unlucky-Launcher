//! rmcp `ServerHandler`：把同一份注册表挂成 MCP 工具路由。
//!
//! 每次 `tools/call` 都走 [`ToolExecutor`]——schema 校验、确认闸门、YOLO、审计、
//! 事件**与内置客户端完全同一条路径**（02 §1 铁律）。工具级失败返回
//! `CallToolResult::error`（客户端能看到文案），协议级错误只留给「路由不到」的情况
//! （research/02 §3.2：`Err(McpError)` 会被大多数客户端吞成 "internal error"）。

use std::sync::Arc;

use rmcp::handler::server::tool::{ToolCallContext, ToolRoute, ToolRouter};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::Value;

use crate::brain::chat::ToolCall;
use crate::brain::exec::ToolExecutor;
use crate::mcp::projection::tool_attr;
use crate::registry::{Access, ToolRegistry};

/// MCP server 的 `ServerHandler` 实现。
///
/// `Clone` 是 Streamable HTTP 要的：每个会话/请求由工厂造一份（共享同一批 `Arc`，
/// 因此审计与策略仍是同一份）。
#[derive(Clone)]
pub struct RegistryServer {
    router: ToolRouter<RegistryServer>,
    visible: Arc<Vec<Tool>>,
    session_id: String,
}

impl std::fmt::Debug for RegistryServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistryServer")
            .field("session_id", &self.session_id)
            .field("tools", &self.visible.len())
            .finish()
    }
}

impl RegistryServer {
    /// 用注册表 + 执行器装配。
    ///
    /// **`router_registry` 必须是完整注册表**（含写工具），`executor` 才决定「哪些工具真能跑」：
    /// - 只读模式：`-executor` 用 `read_only_subset()` 装配，`tools/list` 只列只读项，
    ///   硬调写工具会路由到执行器 → `unknown_tool`（**tool-level 错误 + 审计**），
    ///   而不是 rmcp 的协议级 `-32602 tool not found`（那个客户端只会显示 "internal error"）。
    /// - 写模式：两者都是完整注册表。
    ///
    /// `session_id` 进审计的 `agent_session_id` 字段（`mcp-http` / `mcp-stdio`），
    /// 让复盘时能区分「内置对话改的」和「外部 MCP 改的」。
    #[must_use]
    pub fn new(
        router_registry: Arc<ToolRegistry>,
        executor: Arc<ToolExecutor>,
        session_id: impl Into<String>,
        read_only: bool,
    ) -> Self {
        let mut router = ToolRouter::new();
        for spec in router_registry.specs() {
            let attr = tool_attr(&spec.definition(), spec.access);
            router.add_route(route(attr, Arc::clone(&executor)));
        }
        let visible: Vec<Tool> = router_registry
            .specs()
            .iter()
            .filter(|spec| !read_only || spec.access == Access::ReadOnly)
            .map(|spec| tool_attr(&spec.definition(), spec.access))
            .collect();
        Self {
            router,
            visible: Arc::new(visible),
            session_id: session_id.into(),
        }
    }

    /// 本 server **对外可见**的工具（`tools/list` 的内容）。
    #[must_use]
    pub fn tools(&self) -> Vec<Tool> {
        (*self.visible).clone()
    }

    /// 审计用的会话标签。
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 对外可见的工具数（启动日志/断言用）。
    #[must_use]
    pub fn tool_count(&self) -> usize {
        self.visible.len()
    }
}

fn route(attr: Tool, executor: Arc<ToolExecutor>) -> ToolRoute<RegistryServer> {
    ToolRoute::new_dyn(attr, move |context: ToolCallContext<'_, RegistryServer>| {
        let executor = Arc::clone(&executor);
        let session_id = context.service.session_id().to_string();
        let name = context.name().to_string();
        let arguments = context
            .arguments
            .clone()
            .map_or_else(|| Value::Object(serde_json::Map::new()), Value::Object);

        Box::pin(async move {
            let call = ToolCall {
                id: format!("mcp-{name}"),
                name,
                arguments,
            };
            let outcome = executor.execute(&session_id, &call, 0).await;
            let text = outcome.to_model_content();
            let content = vec![ContentBlock::text(text)];
            let result = if outcome.ok {
                CallToolResult::structured(outcome.content)
            } else {
                CallToolResult::error(content)
            };
            Ok(result.into())
        })
    })
}

impl ServerHandler for RegistryServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "unlucky-launcher",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Unlucky-Launcher 的工具服务：读写启动器配置、看实例/版本/Java、读游戏日志、跑崩溃诊断。\
                 账户凭据永不经由本服务出口；写配置受 schema 白名单与敏感键确认约束。",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let call_context = ToolCallContext::new(self, request, context);
        self.router.call(call_context).await
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.router.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::MemoryAudit;
    use crate::clock::{FixedClock, SharedClock};
    use crate::confirm::{AutoDenyGate, ConfirmPolicy};
    use crate::event::NoopSink;
    use crate::testing::mock_ports;
    use crate::tools::build_v1_registry;
    use serde_json::json;

    fn server(registry: Arc<ToolRegistry>, read_only: bool) -> (RegistryServer, Arc<MemoryAudit>) {
        let audit = Arc::new(MemoryAudit::new());
        let clock: SharedClock = Arc::new(FixedClock::new(1_793_304_753_123));
        let exposed = if read_only {
            Arc::new(registry.read_only_subset())
        } else {
            Arc::clone(&registry)
        };
        let executor = Arc::new(ToolExecutor::new(
            exposed,
            audit.clone(),
            Arc::new(AutoDenyGate),
            ConfirmPolicy::default(),
            clock,
            Arc::new(NoopSink),
        ));
        (
            RegistryServer::new(registry, executor, "mcp-test", read_only),
            audit,
        )
    }

    fn registry() -> Arc<ToolRegistry> {
        let (ports, _settings) = mock_ports();
        Arc::new(build_v1_registry(&ports).expect("注册表应装配成功"))
    }

    #[test]
    fn lists_every_registered_tool_in_write_mode() {
        let registry = registry();
        let (server, _audit) = server(Arc::clone(&registry), false);
        let tools = server.tools();
        assert_eq!(tools.len(), registry.len());
        assert_eq!(server.tool_count(), registry.len());
        assert_eq!(server.session_id(), "mcp-test");
        assert!(tools.iter().any(|tool| tool.name == "set_config"));
    }

    #[test]
    fn read_only_mode_hides_the_writing_tool_but_keeps_it_routable() {
        let (server, _audit) = server(registry(), true);
        let names: Vec<String> = server.tools().iter().map(|t| t.name.to_string()).collect();
        assert!(!names.iter().any(|n| n == "set_config"), "{names:?}");
        assert!(names.iter().any(|n| n == "get_config"));
        assert_eq!(names.len(), 9);
        // 路由仍然存在（否则 rmcp 会回协议级 -32602，客户端看到的是 "internal error"），
        // 真正拦下来的是只读子集执行器 → tool-level 错误 + 审计。
        assert!(server.router.get("set_config").is_some());
    }

    #[test]
    fn info_advertises_tools_and_a_stable_identity() {
        let (server, _audit) = server(registry(), false);
        let info = server.get_info();
        assert!(info.capabilities.tools.is_some());
        assert_eq!(info.server_info.name, "unlucky-launcher");
    }

    #[test]
    fn get_tool_returns_the_projected_attr() {
        let (server, _audit) = server(registry(), false);
        let attr = server.get_tool("get_config").expect("工具存在");
        assert_eq!(attr.name, "get_config");
        assert_eq!(
            Value::Object((*attr.input_schema).clone())["type"],
            json!("object")
        );
        assert!(server.get_tool("nope").is_none());
    }
}
