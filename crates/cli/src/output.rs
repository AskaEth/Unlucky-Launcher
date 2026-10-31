//! 双格式输出：人类可读行 + `--json` 机器可读载荷。
//!
//! 一个命令只产出 [`Report`]（`data` 给脚本，`human` 给人）；怎么打印由 [`print_report`]
//! 一处决定，命令里不出现 `println!`——这样集成测试能直接断言输出形状，
//! 也能保证 `--json` 覆盖到每个命令（10-cli-design.md §3 全局参数）。

use std::io::Write;

use serde_json::{json, Value};

use crate::error::CliError;

/// 一个命令的结果。
#[derive(Debug, Clone)]
pub struct Report {
    /// `--json` 模式下的 `data` 载荷。
    pub data: Value,
    /// 人类可读输出（一行一条）。
    pub human: Vec<String>,
}

impl Report {
    /// 构造。
    #[must_use]
    pub fn new(data: Value, human: Vec<String>) -> Self {
        Self { data, human }
    }
}

/// 命令结果 + 环境提示（D19/D21 低于基线时的升级指引之类，不混进 `data`）。
#[derive(Debug, Clone)]
pub struct Outcome {
    /// 命令结果。
    pub report: Report,
    /// 环境级提示（人类模式进 stderr，JSON 模式进信封的 `warnings`）。
    pub warnings: Vec<String>,
}

impl Outcome {
    /// 无提示的结果。
    #[must_use]
    pub fn new(report: Report) -> Self {
        Self {
            report,
            warnings: Vec::new(),
        }
    }

    /// 带上环境提示。
    #[must_use]
    pub fn with_warnings(report: Report, warnings: Vec<String>) -> Self {
        Self { report, warnings }
    }
}

/// 打印成功结果。
///
/// # Errors
///
/// 写 stdout/stderr 失败时返回 IO 错误（管道被关掉等）。
pub fn print_report(
    outcome: &Outcome,
    json: bool,
    quiet: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> std::io::Result<()> {
    if json {
        // 信封固定三个键，脚本按 `ok` 分流后直接吃 `data`。
        let envelope = json!({
            "ok": true,
            "data": outcome.report.data,
            "warnings": outcome.warnings,
        });
        writeln!(out, "{}", serde_json::to_string_pretty(&envelope)?)?;
        return Ok(());
    }
    if !quiet {
        for line in &outcome.report.human {
            writeln!(out, "{line}")?;
        }
    }
    for warning in &outcome.warnings {
        writeln!(err, "提示: {warning}")?;
    }
    Ok(())
}

/// 打印错误（人类模式进 stderr，JSON 模式进 stdout 的错误信封）。
///
/// # Errors
///
/// 写 stdout/stderr 失败时返回 IO 错误。
pub fn print_error(
    error: &CliError,
    json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> std::io::Result<()> {
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&error.to_json())?)?;
        return Ok(());
    }
    writeln!(err, "错误: {}", error.message())?;
    if let Some(hint) = error.hint() {
        writeln!(err, "提示: {hint}")?;
    }
    Ok(())
}

/// 长任务的实时进度行（**只进 stderr**，不参与 [`Report`]）。
///
/// 命令的正经输出仍然全部走 [`Report`]（stdout、可断言的形状）；下载/安装这类分钟级任务
/// 需要无缓冲的实时反馈，才单独开这条通道。行内不含 ANSI，`--no-color` 无需处理。
///
/// `--json` / `--quiet` 的抑制由**调用方**决定（`InstallArgs::progress` 已按全局选项算好），
/// 这样这条通道不需要知道全局选项长什么样。
///
/// 写不出去（管道早闭）就静默放弃：进度永远不该让命令失败。
pub fn progress_line(text: &str) {
    let mut stderr = std::io::stderr();
    let _ = writeln!(stderr, "{text}");
    let _ = stderr.flush();
}

#[cfg(test)]
mod tests {
    use super::progress_line;

    #[test]
    fn progress_line_never_panics() {
        // 只验证"写不出去也不抛"（真实 stderr 在测试里通常可写）。
        progress_line("测试进度 1/2");
        progress_line("");
    }
}
