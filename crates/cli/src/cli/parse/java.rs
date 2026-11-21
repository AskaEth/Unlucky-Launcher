//! `java` 的子命令解析（`list` / `install` / `vendors`）。
//!
//! 从 `parse.rs` 拆出来只为守 400 行硬线（M280 的 `--version` / `--dry-run` 让那边贴近上限）；
//! `list` / `vendors` 是纯搬移，`install` 多了两个旗标：
//!
//! ```text
//! --version <v>   钉死具体版本（缺省 = 该家最新 GA）；能不能钉由发行版决定，
//!                 钉不了的时候由 core 报错（本层不猜、不改写用户给的值）
//! --dry-run       只查版本并打印将要下载的版本串与 URL，不落盘
//! ```

use unlucky_core::game::java::JdkVendor;

use super::super::model::Command;
use super::*;

pub(super) fn parse_java(tail: &[String]) -> Result<Command, CliError> {
    match tail.first().map(String::as_str) {
        None | Some("list") => {
            ensure_no_extra(tail.get(1..).unwrap_or_default(), "java list")?;
            Ok(Command::JavaList)
        }
        Some("vendors") => {
            ensure_no_extra(tail.get(1..).unwrap_or_default(), "java vendors")?;
            Ok(Command::JavaVendors)
        }
        Some("install") => parse_java_install(tail.get(1..).unwrap_or_default()),
        Some(other) => Err(CliError::usage(format!(
            "java 的子命令只有 list|install|vendors，收到 `{other}`"
        ))),
    }
}

/// `java install <major> [--vendor X] [--version V] [--dry-run]`。
fn parse_java_install(args: &[String]) -> Result<Command, CliError> {
    let parsed = parse_flags(args, &["vendor", "version"], &["dry-run"])?;
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
    // 版本串**原样**往下传（`21.0.13+9` / `jdk-21.0.13+9` / `21.0.13` 各有语义）。
    // 空值在这里就拦掉：`--version ""` 到 core 会变成一个没有任何版本能匹配的请求。
    let version = match parsed.value("version") {
        Some(text) if text.trim().is_empty() => {
            return Err(
                CliError::usage("--version 需要一个版本串，例如 --version 21.0.13+9")
                    .with_hint("%prog% help java 看各发行版能不能指定版本"),
            );
        }
        Some(text) => Some(text.trim().to_string()),
        None => None,
    };
    Ok(Command::JavaInstall {
        major,
        vendor,
        version,
        dry_run: parsed.has("dry-run"),
    })
}

#[cfg(test)]
mod tests {
    use unlucky_core::game::java::JdkVendor;

    use crate::cli::{parse, Command};
    use crate::error::CliError;

    fn parse_args(args: &[&str]) -> Result<Command, CliError> {
        let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        parse(&owned).map(|invocation| invocation.command)
    }

    fn command(args: &[&str]) -> Command {
        parse_args(args).expect("应解析")
    }

    /// M280：`--version` 与 `--dry-run` 是 `java install` 的旗标，不是全局 `--version`。
    ///
    /// 变异证明：把全局 `--version` 的 `if rest.is_empty()` 守卫去掉 ⇒ 本用例第一条红
    /// （解析成 `Command::Version`，装版本的需求被吞掉）。
    #[test]
    fn pinned_version_and_dry_run_are_install_flags() {
        assert_eq!(
            command(&["java", "install", "21", "--version", "21.0.13+9"]),
            Command::JavaInstall {
                major: 21,
                vendor: None,
                version: Some("21.0.13+9".to_string()),
                dry_run: false,
            }
        );
        assert_eq!(
            command(&["java", "install", "21", "--dry-run"]),
            Command::JavaInstall {
                major: 21,
                vendor: None,
                version: None,
                dry_run: true,
            }
        );
        assert_eq!(
            command(&[
                "java",
                "install",
                "21",
                "--vendor",
                "oracle",
                "--version",
                "21.0.2",
                "--dry-run",
            ]),
            Command::JavaInstall {
                major: 21,
                vendor: Some(JdkVendor::Oracle),
                version: Some("21.0.2".to_string()),
                dry_run: true,
            }
        );
    }

    /// 空版本串在解析层就拦住（到 core 会变成「没有任何版本能匹配」的请求）。
    #[test]
    fn an_empty_version_is_a_usage_error() {
        assert!(parse_args(&["java", "install", "21", "--version", ""]).is_err());
        assert!(parse_args(&["java", "install", "21", "--version"]).is_err());
    }
}
