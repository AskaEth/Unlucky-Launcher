//! `instances` 的子命令解析（`list` / `launch-args` / `scan` / `adopt`）。
//!
//! 从 `parse.rs` 拆出来只为守 400 行硬线（M157 的 `launch-args` 让那边到 415 行）；
//! 纯搬移，语义未变。M221 在这里加了 `scan` 与 `adopt` 两个只读/登记入口。

use std::path::PathBuf;

use unlucky_core::game::AdoptTarget;

use super::super::model::{InstanceLaunchArgs, InstancesAdoptArgs};
use super::*;

pub(super) fn parse_instances(tail: &[String]) -> Result<Command, CliError> {
    let args = match tail.first().map(String::as_str) {
        None | Some("list") => tail.get(1..).unwrap_or_default(),
        Some("launch-args") => {
            return parse_instances_launch_args(tail.get(1..).unwrap_or_default())
        }
        Some("scan") => {
            let parsed = parse_flags(tail.get(1..).unwrap_or_default(), &[], &[])?;
            ensure_no_positionals(&parsed, "instances scan")?;
            return Ok(Command::InstancesScan);
        }
        Some("adopt") => return parse_instances_adopt(tail.get(1..).unwrap_or_default()),
        Some(other) => {
            return Err(CliError::usage(format!(
                "instances 的子命令只有 list / launch-args / scan / adopt，收到 `{other}`"
            )))
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

/// `instances adopt <version-id> [--name N] [--isolation shared|isolated|custom] [--game-dir DIR] [--dry-run]`。
///
/// 运行目录的词表只在这里翻译一次（[`AdoptTarget::parse`] 是唯一判据）：
/// - 什么都不给 ⇒ [`AdoptTarget::InstanceDir`]（ULL 数据区新目录，**不沿用全局 `isolated`**）；
/// - `--isolation custom` 必须配 `--game-dir`；
/// - `--isolation shared|isolated` 不许再配 `--game-dir`（那两个档的运行目录是派生的）。
fn parse_instances_adopt(tail: &[String]) -> Result<Command, CliError> {
    let parsed = parse_flags(tail, &["name", "isolation", "game-dir"], &["dry-run"])?;
    let version_id = require_single_positional(&parsed, "instances adopt <version-id>")?;
    let isolation = parsed.value("isolation");
    let game_dir = match parsed.value("game-dir") {
        Some(raw) => {
            let dir = PathBuf::from(&raw);
            // 用法错误在解析期就报（和 `--data-dir` 同一口径），别把「参数写错」拖成命令期失败。
            if !dir.is_absolute() {
                return Err(CliError::usage(format!(
                    "--game-dir 需要绝对路径，收到 `{raw}`"
                )));
            }
            Some(dir)
        }
        None => None,
    };
    let target = match isolation.as_deref() {
        None => match game_dir {
            // `--game-dir` 单独给 = 就是 custom（少写一个选项，不制造第二种说法）。
            Some(dir) => AdoptTarget::Custom(dir),
            // 缺省**只能**来自 core 的 `AdoptTarget::default()`：这里再写一遍 `InstanceDir`
            // 就等于有了第二处真相源，改缺省时会漏一处。
            None => AdoptTarget::default(),
        },
        Some("custom") => {
            let dir = game_dir.ok_or_else(|| {
                CliError::usage("--isolation custom 需要配 --game-dir <dir>（绝对路径）")
            })?;
            AdoptTarget::Custom(dir)
        }
        Some(other) => {
            if game_dir.is_some() {
                return Err(CliError::usage(
                    "--game-dir 只配 --isolation custom（shared / isolated 的运行目录是派生的）",
                ));
            }
            AdoptTarget::parse(other, None).ok_or_else(|| {
                CliError::usage(format!(
                    "--isolation 只有 shared|isolated|custom，收到 `{other}`"
                ))
                .with_hint("不给 --isolation = 落 ULL 数据区的独立运行目录（推荐）")
            })?
        }
    };
    Ok(Command::InstancesAdopt(InstancesAdoptArgs {
        version_id,
        name: parsed.value("name"),
        target,
        dry_run: parsed.has("dry-run"),
    }))
}
