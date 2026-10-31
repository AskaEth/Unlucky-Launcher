//! `ull accounts` 子命令的解析（从 `parse.rs` 拆出，避免解析器主文件顶到 400 行）。
//!
//! 语法：
//! - `ull accounts [list]`
//! - `ull accounts add offline <name>`
//! - `ull accounts add msa`（别名 `microsoft`）
//! - `ull accounts remove <name>`（别名 `rm`）
//!
//! 只开这三条轨道入口：authlib 的密码登录不走 CLI（密码进 argv 会泄露，见
//! `DevDocs/SubAgentsTalk/auth-接口约定.md` §4）。

use super::{ensure_no_positionals, parse_flags, require_single_positional};
use crate::cli::model::{AccountsAddArgs, AccountsAddKind, Command};
use crate::error::CliError;

/// 解析 `accounts ...`。
pub(super) fn parse_accounts(tail: &[String]) -> Result<Command, CliError> {
    let Some(sub) = tail.first().map(String::as_str) else {
        return Ok(Command::AccountsList);
    };
    let rest = tail.get(1..).unwrap_or_default();
    match sub {
        "list" => {
            let parsed = parse_flags(rest, &[], &[])?;
            ensure_no_positionals(&parsed, "accounts list")?;
            Ok(Command::AccountsList)
        }
        "add" => parse_add(rest),
        "remove" | "rm" => {
            let parsed = parse_flags(rest, &[], &[])?;
            let name = require_single_positional(&parsed, "accounts remove <name>")?;
            Ok(Command::AccountsRemove { name })
        }
        other => Err(CliError::usage(format!(
            "accounts 的子命令只有 list|add|remove，收到 `{other}`"
        ))),
    }
}

fn parse_add(rest: &[String]) -> Result<Command, CliError> {
    let Some(kind) = rest.first().map(String::as_str) else {
        return Err(
            CliError::usage("accounts add 需要轨道：offline|msa").with_hint(
                "ull accounts add offline <玩家名> / ull accounts add msa（设备码交互式登录）",
            ),
        );
    };
    let parsed = parse_flags(rest.get(1..).unwrap_or_default(), &[], &[])?;
    match kind {
        "offline" => {
            let name = require_single_positional(&parsed, "accounts add offline <name>")?;
            Ok(Command::AccountsAdd(AccountsAddArgs {
                kind: AccountsAddKind::Offline,
                name: Some(name),
            }))
        }
        "msa" | "microsoft" => {
            ensure_no_positionals(&parsed, "accounts add msa")?;
            Ok(Command::AccountsAdd(AccountsAddArgs {
                kind: AccountsAddKind::Microsoft,
                name: None,
            }))
        }
        other => Err(CliError::usage(format!(
            "accounts add 的轨道只有 offline|msa，收到 `{other}`"
        ))),
    }
}
