//! `agent` 子命令解析（`serve` / `config`）。
//!
//! 与 `parse::accounts` 同样的拆分理由：`parse.rs` 挨着 400 行硬上限，
//! 而且「MCP server 的参数」本来就是一撮独立的东西，混在主解析器里迟早长成一片。

use crate::cli::model::{AgentServeArgs, Command};
use crate::cli::parse::ParsedArgs;
use crate::error::CliError;

use super::{ensure_no_positionals, parse_flags};

/// 解析 `ull agent <sub>`。
///
/// # Errors
/// 缺子命令、未知子命令、未知选项、端口不是 1~65535 时返回用法错误。
pub fn parse_agent(tail: &[String]) -> Result<Command, CliError> {
    let Some(sub) = tail.first().map(String::as_str) else {
        return Err(CliError::usage("agent 需要子命令 serve|config")
            .with_hint("%prog% agent serve / %prog% agent config"));
    };
    let rest = tail.get(1..).unwrap_or_default();
    match sub {
        "serve" => parse_serve(rest),
        "config" => parse_config(rest),
        other => Err(CliError::usage(format!(
            "agent 的子命令只有 serve|config，收到 `{other}`"
        ))),
    }
}

fn parse_serve(rest: &[String]) -> Result<Command, CliError> {
    let parsed = parse_flags(
        rest,
        &["port", "host"],
        &["stdio", "allow-write", "readonly"],
    )?;
    ensure_no_positionals(&parsed, "agent serve")?;
    if parsed.has("allow-write") && parsed.has("readonly") {
        return Err(CliError::usage(
            "--allow-write 与 --readonly 不能同时给（二选一）",
        ));
    }
    Ok(Command::AgentServe(AgentServeArgs {
        stdio: parsed.has("stdio"),
        port: optional_port(&parsed)?,
        host: parsed.value("host"),
        allow_write: parsed.has("allow-write"),
    }))
}

fn parse_config(rest: &[String]) -> Result<Command, CliError> {
    let parsed = parse_flags(rest, &["port"], &[])?;
    ensure_no_positionals(&parsed, "agent config")?;
    Ok(Command::AgentConfig {
        port: optional_port(&parsed)?,
    })
}

fn optional_port(parsed: &ParsedArgs) -> Result<Option<u16>, CliError> {
    match parsed.value("port") {
        Some(raw) => Ok(Some(raw.parse::<u16>().map_err(|_| {
            CliError::usage(format!("--port 需要 1~65535，收到 `{raw}`"))
        })?)),
        None => Ok(None),
    }
}
