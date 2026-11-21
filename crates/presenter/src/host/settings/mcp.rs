//! 设置页「MCP server」分组（M397 从 `host/settings.rs` 整段搬来，函数体逐字不动）。

use crate::dto::{SettingGroupView, SettingRowView};

use super::super::env::HostEnv;
use super::{flag, int};

/// M39 的对外 MCP server（默认关、默认只读）。
pub(super) fn mcp_group(env: &HostEnv) -> SettingGroupView {
    let port = int(env, "agent.mcp.port", 47_823);
    SettingGroupView::new("mcp", "MCP server（M39）")
        .with(SettingRowView::toggle(
            "agent.mcp.enabled",
            "对外提供 MCP server",
            "127.0.0.1 上的 Streamable HTTP（stdio 需显式启动）",
            flag(env, "agent.mcp.enabled", false),
        ))
        .with(SettingRowView::text(
            "agent.mcp.port",
            "端口",
            "1024~65535，只绑 127.0.0.1",
            &port.to_string(),
        ))
        .with(SettingRowView::toggle(
            "agent.mcp.allow_write",
            "允许外部客户端写配置",
            "默认只读；写仍受 schema 白名单与分级确认约束",
            flag(env, "agent.mcp.allow_write", false),
        ))
        .with(SettingRowView::read_only(
            "客户端地址",
            &format!("http://127.0.0.1:{port}/mcp（令牌见 agent.mcp.token，AI 不可写）"),
        ))
}
