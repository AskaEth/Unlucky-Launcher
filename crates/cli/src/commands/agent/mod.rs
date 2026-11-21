//! `ull agent`：对外 MCP server 的 headless 入口（02 文档 §1 的第二形态）。
//!
//! 两个子命令：
//! - `agent serve`：长驻服务（`--stdio` 或 Streamable HTTP on 127.0.0.1）；
//! - `agent config`：打印 Claude Code / Cursor 的配置样例（含当前端口与令牌状态）。
//!
//! 为什么 `serve` 不能走 `commands::run` 的「返回 Report 再打印」套路：stdio 形态下
//! **stdout 就是 JSON-RPC 流**，任何一行多余输出都会让客户端解析失败。所以 runner
//! 里对 `Command::AgentServe` 提前分流，不走 `output::print_report`。

pub mod ports;
pub mod probe;
pub mod serve;

use serde_json::{json, Value};
use unlucky_agent::mcp::config_samples;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::cli::AgentServeArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

/// `ull agent serve`：起服务（阻塞）。
///
/// # Errors
/// 见 [`serve::run`]。
pub fn run_serve(ctx: &Ctx, args: &AgentServeArgs) -> Result<i32, CliError> {
    serve::run(ctx, args)
}

/// runner 的分流入口：解析数据根 → 起服务。错误由 runner 统一打印。
///
/// # Errors
/// 数据根缺失、平台不达基线、端口占用、非回环监听等返回用户错误。
pub fn run_serve_with(
    globals: &crate::cli::Globals,
    args: &AgentServeArgs,
    err: &mut dyn std::io::Write,
) -> Result<i32, CliError> {
    let ctx = Ctx::resolve(globals)?;
    ctx.ensure_supported()?;
    for note in ctx.notes() {
        let _ = writeln!(err, "提示: {note}");
    }
    run_serve(&ctx, args)
}

/// `ull agent config`：输出两份可粘贴的客户端配置 + 当前设置状态。
///
/// # Errors
/// 配置不可读时返回用户错误（schema 不认识的键不影响）。
pub fn config_report(ctx: &Ctx, port: Option<u16>) -> Result<Report, CliError> {
    let store = SettingStore::load(ctx.launcher_config(), launcher_schema());
    let doc = store.doc();
    let enabled = doc
        .effective("agent.mcp.enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let allow_write = doc
        .effective("agent.mcp.allow_write")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let yolo = doc
        .effective("ai.yolo")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let token = doc
        .effective_str("agent.mcp.token")
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let configured_port = doc
        .effective("agent.mcp.port")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(47823);
    let port = port.unwrap_or(configured_port);
    let url = format!("http://127.0.0.1:{port}/mcp");
    let samples = config_samples(&url, token.as_deref());
    let registry = unlucky_agent::tools::build_v1_registry(&ports::build(ctx))
        .map_err(|error| CliError::internal("agent_registry", error.to_string()))?;
    let read_only_tools = registry.read_only_definitions().len();
    let all_tools = registry.len();

    let data = json!({
        "url": url,
        "enabled": enabled,
        "allow_write": allow_write,
        "yolo": yolo,
        "token_configured": token.is_some(),
        "read_only_tools": read_only_tools,
        "all_tools": all_tools,
        "claude_code": samples.claude_code,
        "cursor": samples.cursor,
    });

    let mut human = vec![
        format!("MCP 端点：{url}"),
        format!(
            "开关：{} · 写权限：{} · YOLO：{} · 令牌：{}",
            if enabled {
                "已启用"
            } else {
                "未启用（serve 仍可手动起）"
            },
            if allow_write {
                "开"
            } else {
                "关（只读）"
            },
            if yolo { "开" } else { "关" },
            if token.is_some() {
                "已设置"
            } else {
                "未设置"
            }
        ),
        "Claude Code（~/.claude.json 或 `claude mcp add --json`）：".to_string(),
    ];
    human.extend(pretty_lines(&samples.claude_code));
    human.push("Cursor（.cursor/mcp.json）：".to_string());
    human.extend(pretty_lines(&samples.cursor));
    human.push(
        "启动：%prog% agent serve（默认 Streamable HTTP） / %prog% agent serve --stdio".to_string(),
    );
    Ok(Report::new(data, human))
}

fn pretty_lines(value: &Value) -> Vec<String> {
    match serde_json::to_string_pretty(value) {
        Ok(text) => text.lines().map(|line| format!("  {line}")).collect(),
        Err(_) => vec!["  （序列化失败）".to_string()],
    }
}
