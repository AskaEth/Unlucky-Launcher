//! 进程入口的可测试形态：解析 → 分发 → 打印 → 退出码。
//!
//! `main` 只做两件事：把真实 stdio 传进来、把返回码交给进程。

use std::io::Write;

use crate::cli;
use crate::commands;
use crate::error::EXIT_OK;
use crate::output;

/// 跑一条命令行，返回进程退出码（0 / 1 / 2）。
///
/// `args` 不含 argv[0]。`--json` 与 `--quiet` 在解析失败时也要生效，
/// 因此这里先扫一遍原始参数，而不是依赖解析结果。
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let json = args.iter().any(|arg| arg == "--json");
    let quiet = args.iter().any(|arg| arg == "--quiet" || arg == "-q");

    let invocation = match cli::parse(args) {
        Ok(invocation) => invocation,
        Err(error) => {
            let _ = output::print_error(&error, json, out, err);
            return error.exit_code();
        }
    };

    // `ull agent serve` 提前分流：stdio 形态下 stdout 是 JSON-RPC 流，
    // 任何一行额外输出（包括返回后的 Report）都会让客户端解析失败。
    if let cli::Command::AgentServe(serve_args) = &invocation.command {
        return match crate::commands::agent::run_serve_with(&invocation.globals, serve_args, err) {
            Ok(code) => code,
            Err(error) => {
                let _ = output::print_error(&error, json, out, err);
                error.exit_code()
            }
        };
    }

    match commands::run(&invocation) {
        Ok(outcome) => {
            if let Err(write_err) = output::print_report(&outcome, json, quiet, out, err) {
                // 输出写不出去（管道早闭）不再折腾，直接按内部错误退出。
                let _ = writeln!(err, "错误: 写输出失败：{write_err}");
                return crate::error::EXIT_INTERNAL;
            }
            EXIT_OK
        }
        Err(error) => {
            let _ = output::print_error(&error, json, out, err);
            error.exit_code()
        }
    }
}
