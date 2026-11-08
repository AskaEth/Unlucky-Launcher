//! `instances` 的两个子命令解析（`list` / `launch-args`）。
//!
//! 从 `parse.rs` 拆出来只为守 400 行硬线（M157 的 `launch-args` 让那边到 415 行）；
//! 纯搬移，语义未变。

use super::super::model::InstanceLaunchArgs;
use super::*;

pub(super) fn parse_instances(tail: &[String]) -> Result<Command, CliError> {
    let args = match tail.first().map(String::as_str) {
        None | Some("list") => tail.get(1..).unwrap_or_default(),
        Some("launch-args") => {
            return parse_instances_launch_args(tail.get(1..).unwrap_or_default())
        }
        Some(other) => {
            return Err(CliError::usage(format!(
                "instances 的子命令只有 list / launch-args，收到 `{other}`"
            )));
        }
    };
    let parsed = parse_flags(args, &[], &[])?;
    ensure_no_positionals(&parsed, "instances list")?;
    Ok(Command::InstancesList)
}

/// `instances launch-args <instance> [--jvm …] [--game …] [--memory …] [--java …] [--backend …] [--clear]`。
///
/// 五个取值选项都接受空串（`--jvm ""`）= 该清空，与 `--clear` 的「先清后改」互补。
fn parse_instances_launch_args(tail: &[String]) -> Result<Command, CliError> {
    let parsed = parse_flags(
        tail,
        &["jvm", "game", "memory", "java", "backend"],
        &["clear"],
    )?;
    let instance = require_single_positional(&parsed, "instances launch-args <instance>")?;
    Ok(Command::InstancesLaunchArgs(InstanceLaunchArgs {
        instance,
        jvm_args: parsed.value("jvm"),
        game_args: parsed.value("game"),
        memory_mb: parsed.value("memory"),
        java: parsed.value("java"),
        backend: parsed.value("backend"),
        clear: parsed.has("clear"),
    }))
}
