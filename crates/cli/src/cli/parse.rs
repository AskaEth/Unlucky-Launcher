//! 手写解析器：全局选项 → 子命令 → 子命令内的选项。

use std::collections::BTreeMap;
use std::path::PathBuf;

use unlucky_core::game::java::JdkVendor;

use crate::error::CliError;

mod accounts;
mod agent;

use super::model::{
    Command, ConfigGetArgs, ConfigSetArgs, DiagArgs, Globals, InstallArgs, Invocation, LaunchArgs,
    LogsArgs, PackImportArgs,
};

/// `logs` 缺省尾巴行数。
const DEFAULT_LOG_TAIL: usize = 200;
/// `diag` 缺省送入规则引擎的行数。
const DEFAULT_DIAG_TAIL: usize = 2000;
/// `sessions list` 缺省条数。
const DEFAULT_SESSION_LIMIT: usize = 20;

/// 解析整条命令行（不含 argv[0]）。
///
/// # Errors
///
/// 用法错误（未知命令/选项、缺参数、数字解析失败）返回 [`CliError`]（退出码 1）。
pub fn parse(args: &[String]) -> Result<Invocation, CliError> {
    let mut globals = Globals::default();
    let mut rest: Vec<String> = Vec::new();
    let mut help_requested = false;
    let mut version_requested = false;
    let mut index = 0;

    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "--json" => globals.json = true,
            "--no-color" => globals.no_color = true,
            "--quiet" | "-q" => globals.quiet = true,
            // 帮助/版本是「伪命令」，但不提前返回：`--help` 出现在命令前面时，
            // 主题要从后面收完的位置参数里取（`ull --help launch` 与 `ull launch --help` 等价）。
            "--help" | "-h" => help_requested = true,
            "--version" | "-V" => version_requested = true,
            "--data-dir" => {
                index += 1;
                globals.data_dir = Some(PathBuf::from(take_value(args, index, "data-dir")?));
            }
            _ => match arg.strip_prefix("--data-dir=") {
                Some(value) => globals.data_dir = Some(PathBuf::from(value)),
                None => rest.push(arg.to_string()),
            },
        }
        index += 1;
    }

    if help_requested {
        return Ok(Invocation {
            globals,
            command: Command::Help(rest.first().cloned()),
        });
    }
    if version_requested {
        return Ok(Invocation {
            globals,
            command: Command::Version,
        });
    }

    let command = parse_command(&rest, &globals)?;
    Ok(Invocation { globals, command })
}

fn parse_command(rest: &[String], globals: &Globals) -> Result<Command, CliError> {
    let Some(name) = rest.first() else {
        return Ok(Command::Welcome);
    };
    let tail = &rest[1..];
    match name.as_str() {
        "version" => Ok(Command::Version),
        "help" => Ok(Command::Help(tail.first().cloned())),
        "instances" => parse_instances(tail),
        "install" => parse_install(tail, globals),
        "pack" => parse_pack(tail),
        "launch" => parse_launch(tail),
        "sessions" => parse_sessions(tail),
        "logs" => parse_logs(tail),
        "diag" => parse_diag(tail),
        "config" => parse_config(tail),
        "java" => parse_java(tail),
        "accounts" => accounts::parse_accounts(tail),
        "agent" => agent::parse_agent(tail),
        other => {
            Err(CliError::usage(format!("未知命令 `{other}`"))
                .with_hint("%prog% --help 查看命令列表"))
        }
    }
}

fn parse_instances(tail: &[String]) -> Result<Command, CliError> {
    let args = match tail.first().map(String::as_str) {
        None | Some("list") => tail.get(1..).unwrap_or_default(),
        Some(other) => {
            return Err(CliError::usage(format!(
                "instances 的子命令只有 list，收到 `{other}`"
            )));
        }
    };
    let parsed = parse_flags(args, &[], &[])?;
    ensure_no_positionals(&parsed, "instances list")?;
    Ok(Command::InstancesList)
}

fn parse_install(tail: &[String], globals: &Globals) -> Result<Command, CliError> {
    let parsed = parse_flags(tail, &["loader", "loader-version", "name"], &["dry-run"])?;
    let version = require_single_positional(&parsed, "install <version>")?;
    Ok(Command::Install(InstallArgs {
        version,
        loader: parsed.value("loader"),
        loader_version: parsed.value("loader-version"),
        name: parsed.value("name"),
        dry_run: parsed.has("dry-run"),
        // 实时进度进 stderr：人类模式是文本行，`--json` 模式是 JSONL（stdout 保持干净），
        // `--quiet` 一律不打。
        progress: !globals.quiet,
        json_progress: globals.json,
    }))
}

/// `pack import <file> [--target <dir>] [--name N] [--overwrite] [--no-register]`。
fn parse_pack(tail: &[String]) -> Result<Command, CliError> {
    let Some(sub) = tail.first().map(String::as_str) else {
        return Err(CliError::usage("pack 需要子命令 import")
            .with_hint("%prog% pack import <file> [--target <dir>]"));
    };
    if sub != "import" {
        return Err(CliError::usage(format!(
            "pack 的子命令只有 import，收到 `{sub}`"
        )));
    }
    let parsed = parse_flags(
        tail.get(1..).unwrap_or_default(),
        &["target", "name"],
        &["overwrite", "no-register"],
    )?;
    let path = require_single_positional(&parsed, "pack import <file>")?;
    Ok(Command::PackImport(PackImportArgs {
        path: PathBuf::from(path),
        target: parsed.value("target").map(PathBuf::from),
        name: parsed.value("name"),
        overwrite: parsed.has("overwrite"),
        no_register: parsed.has("no-register"),
    }))
}

fn parse_launch(tail: &[String]) -> Result<Command, CliError> {
    let parsed = parse_flags(
        tail,
        &["java", "username", "account", "max-memory"],
        &["dry-run"],
    )?;
    let instance = require_single_positional(&parsed, "launch <instance>")?;
    let max_memory = match parsed.value("max-memory") {
        Some(raw) => Some(
            raw.parse::<u32>()
                .map_err(|_| CliError::usage(format!("--max-memory 需要整数 MB，收到 `{raw}`")))?,
        ),
        None => None,
    };
    Ok(Command::Launch(LaunchArgs {
        instance,
        dry_run: parsed.has("dry-run"),
        java: parsed.value("java").map(PathBuf::from),
        username: parsed.value("username"),
        account: parsed.value("account"),
        max_memory,
    }))
}

fn parse_sessions(tail: &[String]) -> Result<Command, CliError> {
    let args = match tail.first().map(String::as_str) {
        None | Some("list") => tail.get(1..).unwrap_or_default(),
        Some(other) => {
            return Err(CliError::usage(format!(
                "sessions 的子命令只有 list，收到 `{other}`"
            )));
        }
    };
    let parsed = parse_flags(args, &["limit"], &[])?;
    ensure_no_positionals(&parsed, "sessions list")?;
    let limit = match parsed.value("limit") {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| CliError::usage(format!("--limit 需要整数，收到 `{raw}`")))?,
        None => DEFAULT_SESSION_LIMIT,
    };
    Ok(Command::SessionsList { limit })
}

fn parse_logs(tail: &[String]) -> Result<Command, CliError> {
    let parsed = parse_flags(tail, &["tail"], &[])?;
    let session_id = require_single_positional(&parsed, "logs <sid>")?;
    let tail_lines = optional_count(&parsed, "tail", DEFAULT_LOG_TAIL)?;
    Ok(Command::Logs(LogsArgs {
        session_id,
        tail: tail_lines,
    }))
}

fn parse_diag(tail: &[String]) -> Result<Command, CliError> {
    let parsed = parse_flags(tail, &["tail"], &[])?;
    let session_id = require_single_positional(&parsed, "diag <sid>")?;
    let tail_lines = optional_count(&parsed, "tail", DEFAULT_DIAG_TAIL)?;
    Ok(Command::Diag(DiagArgs {
        session_id,
        tail: tail_lines,
    }))
}

fn parse_config(tail: &[String]) -> Result<Command, CliError> {
    let Some(sub) = tail.first().map(String::as_str) else {
        return Err(CliError::usage("config 需要子命令 get|set")
            .with_hint("%prog% config get <key> / %prog% config set <key> <value>"));
    };
    let rest = tail.get(1..).unwrap_or_default();
    match sub {
        "get" => {
            let parsed = parse_flags(rest, &[], &["show-secret"])?;
            if parsed.positionals.len() > 1 {
                return Err(CliError::usage("config get 最多接受一个键路径"));
            }
            Ok(Command::ConfigGet(ConfigGetArgs {
                key: parsed.positionals.first().cloned(),
                show_secret: parsed.has("show-secret"),
            }))
        }
        "set" => {
            let parsed = parse_flags(rest, &[], &[])?;
            if parsed.positionals.len() != 2 {
                return Err(CliError::usage(
                    "config set 需要键路径与值：%prog% config set <key> <value>",
                ));
            }
            Ok(Command::ConfigSet(ConfigSetArgs {
                key: parsed.positionals[0].clone(),
                value: parsed.positionals[1].clone(),
            }))
        }
        other => Err(CliError::usage(format!(
            "config 的子命令只有 get|set，收到 `{other}`"
        ))),
    }
}

fn parse_java(tail: &[String]) -> Result<Command, CliError> {
    match tail.first().map(String::as_str) {
        None | Some("list") => {
            ensure_no_extra(tail.get(1..).unwrap_or_default(), "java list")?;
            Ok(Command::JavaList)
        }
        Some("vendors") => {
            ensure_no_extra(tail.get(1..).unwrap_or_default(), "java vendors")?;
            Ok(Command::JavaVendors)
        }
        Some("install") => {
            let parsed = parse_flags(tail.get(1..).unwrap_or_default(), &["vendor"], &[])?;
            let raw = require_single_positional(&parsed, "java install <major>")?;
            let major = raw
                .parse::<u32>()
                .map_err(|_| CliError::usage(format!("JDK 主版本需要整数，收到 `{raw}`")))?;
            let vendor = match parsed.value("vendor") {
                Some(text) => Some(JdkVendor::parse(&text).ok_or_else(|| {
                    CliError::usage(format!(
                        "--vendor 只有 temurin|oracle|microsoft|zulu，收到 `{text}`"
                    ))
                })?),
                None => None,
            };
            Ok(Command::JavaInstall { major, vendor })
        }
        Some(other) => Err(CliError::usage(format!(
            "java 的子命令只有 list|install|vendors，收到 `{other}`"
        ))),
    }
}

/// 单个位置参数（多一个少一个都算用法错误）。
fn require_single_positional(parsed: &ParsedArgs, usage: &str) -> Result<String, CliError> {
    match parsed.positionals.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(CliError::usage(format!("缺少参数：%prog% {usage}"))),
        _ => Err(CliError::usage(format!("参数过多：%prog% {usage}"))),
    }
}

fn optional_count(parsed: &ParsedArgs, name: &str, default: usize) -> Result<usize, CliError> {
    match parsed.value(name) {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| CliError::usage(format!("--{name} 需要整数，收到 `{raw}`"))),
        None => Ok(default),
    }
}

fn ensure_no_extra(args: &[String], usage: &str) -> Result<(), CliError> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(CliError::usage(format!(
            "{usage} 不接受参数：{}",
            args.join(" ")
        )))
    }
}

fn ensure_no_positionals(parsed: &ParsedArgs, usage: &str) -> Result<(), CliError> {
    ensure_no_extra(&parsed.positionals, usage)
}

/// 解析后的一条子命令参数。
struct ParsedArgs {
    positionals: Vec<String>,
    flags: BTreeMap<String, Option<String>>,
}

impl ParsedArgs {
    fn value(&self, name: &str) -> Option<String> {
        self.flags.get(name).cloned().flatten()
    }

    fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }
}

/// 取值形 flag 的下一 token。
///
/// 下一个 token 若以 `--` 开头，说明用户漏了值（`ull launch x --java --dry-run`），
/// 此时报用法错误，而不是把 flag 当值吞掉后在下游报出费解的「找不到 Java」。
/// 负数（`-1`）不以 `--` 开头，不会被误伤。
fn take_value(args: &[String], index: usize, name: &str) -> Result<String, CliError> {
    let value = args
        .get(index)
        .ok_or_else(|| CliError::usage(format!("--{name} 需要一个值")))?;
    if value.starts_with("--") {
        return Err(CliError::usage(format!(
            "--{name} 需要一个值，但收到选项 `{value}`"
        )));
    }
    Ok(value.clone())
}

/// `--flag` / `--flag=value` / `--flag value` + 位置参数。
fn parse_flags(args: &[String], valued: &[&str], flags: &[&str]) -> Result<ParsedArgs, CliError> {
    let mut parsed = ParsedArgs {
        positionals: Vec::new(),
        flags: BTreeMap::new(),
    };
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let Some(body) = arg.strip_prefix("--") else {
            parsed.positionals.push(arg.clone());
            index += 1;
            continue;
        };
        let (name, inline) = match body.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (body, None),
        };
        if valued.contains(&name) {
            let value = match inline {
                Some(value) => value,
                None => {
                    index += 1;
                    take_value(args, index, name)?
                }
            };
            parsed.flags.insert(name.to_string(), Some(value));
        } else if flags.contains(&name) {
            if inline.is_some() {
                return Err(CliError::usage(format!("--{name} 不接受值")));
            }
            parsed.flags.insert(name.to_string(), None);
        } else {
            return Err(CliError::usage(format!("未知选项 --{name}")));
        }
        index += 1;
    }
    Ok(parsed)
}
