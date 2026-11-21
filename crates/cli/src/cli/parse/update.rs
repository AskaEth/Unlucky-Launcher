//! `ull update` 子命令的解析（从 `parse.rs` 拆出，避免解析器主文件顶到 400 行）。
//!
//! 语法：
//! - `ull update check`
//! - `ull update apply [--dest <path>]`
//!
//! 两条命令都只是「造一个 `core::update::Job` → 调 `check` / `apply` → 排版输出」，
//! 判定（版本比较、渠道准入、清单校验）与替换（双检 + 原子改名）全在 core。
//! `--dest` 钉替换目标，缺省是本进程的 exe（取证 / 自定义安装位置时才给）。
//!
//! ## 为什么单独一个文件
//!
//! `parse.rs` 在加进这两条之后是 406 行，撞仓库 400 行硬线（`guards.yml` 的 `file-size`
//! 作业按 `git ls-files '*.rs'` + `wc -l > 400` 判红）。拆法与 `parse/{accounts,agent,
//! instances}.rs` 一致：一个子命令族一个文件，主文件只留分发。

use std::path::PathBuf;

use super::{ensure_no_extra, ensure_no_positionals, parse_flags};
use crate::cli::model::{Command, UpdateApplyArgs, UpdateCheckArgs};
use crate::error::CliError;

/// 解析 `update ...`。
pub(super) fn parse_update(tail: &[String]) -> Result<Command, CliError> {
    let Some(sub) = tail.first().map(String::as_str) else {
        return Err(CliError::usage("update 需要子命令 check|apply")
            .with_hint("%prog% update check / %prog% update apply"));
    };
    let rest = tail.get(1..).unwrap_or_default();
    match sub {
        "check" => {
            ensure_no_extra(rest, "update check")?;
            Ok(Command::UpdateCheck(UpdateCheckArgs {}))
        }
        "apply" => {
            let parsed = parse_flags(rest, &["dest"], &[])?;
            ensure_no_positionals(&parsed, "update apply")?;
            Ok(Command::UpdateApply(UpdateApplyArgs {
                dest: parsed.value("dest").map(PathBuf::from),
            }))
        }
        other => Err(CliError::usage(format!(
            "update 的子命令只有 check|apply，收到 `{other}`"
        ))),
    }
}
