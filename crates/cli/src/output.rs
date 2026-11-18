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
    /// 构造（人类可读行在这里就把程序名占位符渲染掉，见 [`crate::cli::program::PLACEHOLDER`]）。
    #[must_use]
    pub fn new(data: Value, human: Vec<String>) -> Self {
        Self {
            data,
            human: human.into_iter().map(|line| fill_program(&line)).collect(),
        }
    }
}

/// 命令结果 + 环境提示（D19/D21 低于基线时的升级指引之类，不混进 `data`）。
#[derive(Debug, Clone)]
pub struct Outcome {
    /// 命令结果。
    pub report: Report,
    /// 环境级提示（人类模式进 stderr，JSON 模式进信封的 `warnings`）。
    pub warnings: Vec<String>,
    /// 命令自定义的退出码；`None` = 0（绝大多数命令）。
    ///
    /// 只有"结果多态且脚本要分流"的命令才用得上：`ull pack import` 的三态是
    /// `0` 成功 / `3` 部分成功 / `1` 失败，光靠 0/1 分不出来。加这个字段而不是让命令
    /// 自己 `std::process::exit`，是为了让**输出照常刷新**（绕过 runner 的输出刷新会把
    /// "输出没刷完就退出"这种 bug 装进 CLI）。
    pub exit_code: Option<i32>,
}

impl Outcome {
    /// 无提示的结果。
    #[must_use]
    pub fn new(report: Report) -> Self {
        Self {
            report,
            warnings: Vec::new(),
            exit_code: None,
        }
    }

    /// 带上环境提示（同样在构造时渲染占位符，JSON 与人类可读两条出口才不会分叉）。
    #[must_use]
    pub fn with_warnings(report: Report, warnings: Vec<String>) -> Self {
        Self {
            report,
            warnings: warnings
                .into_iter()
                .map(|line| fill_program(&line))
                .collect(),
            exit_code: None,
        }
    }

    /// 指定进程退出码（不指定 = 0）。
    #[must_use]
    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
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

/// 渲染文案里的程序名占位符（M82）。
///
/// **只替换占位符**，不对整条文本做词替换：消息里插值进来的账户名 / 路径 / 回显的子命令
/// 一个字节都不动（r1 曾把账户名 `ull` 改成程序名、把 `D:/tmp/ull/.ULL` 改成
/// `D:/tmp/unlucky-launcher/.ULL`）。
fn fill_program(text: &str) -> String {
    crate::cli::program::fill(text)
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
    use super::{print_report, progress_line, Outcome, Report};
    use crate::cli::program::{self, PLACEHOLDER};
    use crate::error::CliError;
    use serde_json::json;

    #[test]
    fn progress_line_never_panics() {
        // 只验证"写不出去也不抛"（真实 stderr 在测试里通常可写）。
        progress_line("测试进度 1/2");
        progress_line("");
    }

    #[test]
    fn json_error_envelope_never_leaks_the_placeholder() {
        // r2 的回归：JSON 分支直接序列化未渲染的 message/hint，`%prog%` 原样漏给脚本。
        let error = CliError::usage(format!("{PLACEHOLDER} 需要一个值"))
            .with_hint(format!("{PLACEHOLDER} --help 查看命令列表"));
        let envelope = serde_json::to_string(&error.to_json()).expect("序列化");
        assert!(!envelope.contains(PLACEHOLDER), "{envelope}");
        assert!(envelope.contains(&program::name()), "{envelope}");
    }

    #[test]
    fn json_warnings_never_leak_the_placeholder() {
        // warnings 是第二条 JSON 出口（以前连人类可读那条都没走到）。
        let report = Report::new(
            json!({ "ok": true }),
            vec![format!("{PLACEHOLDER} instances list 看看现有实例")],
        );
        let outcome =
            Outcome::with_warnings(report, vec![format!("用 {PLACEHOLDER} install 装版本")]);
        let mut out: Vec<u8> = Vec::new();
        let mut err: Vec<u8> = Vec::new();
        print_report(&outcome, true, false, &mut out, &mut err).expect("写 JSON");
        let text = String::from_utf8_lossy(&out).into_owned();
        assert!(!text.contains(PLACEHOLDER), "{text}");
        assert!(text.contains(&program::name()), "{text}");
    }

    #[test]
    fn human_lines_render_the_placeholder_too() {
        let report = Report::new(
            json!({}),
            vec![format!("用 `{PLACEHOLDER} install` 登记实例")],
        );
        assert_eq!(
            report.human[0],
            format!("用 `{} install` 登记实例", program::name())
        );
        // 用户数据不受影响：账户名恰好叫 ull 的那种
        let report = Report::new(json!({}), vec!["找不到账户 `ull`".to_string()]);
        assert_eq!(report.human[0], "找不到账户 `ull`");
    }
}
