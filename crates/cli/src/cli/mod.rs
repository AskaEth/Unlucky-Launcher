//! 命令行解析（手写，不引 clap —— 子命令集小，省一棵依赖树）。
//!
//! 语法：`ull [全局选项] <命令> [子命令/参数]`。全局选项可以出现在任意位置
//! （脚本里 `ull instances list --json` 与 `ull --json instances list` 等价）。
//!
//! 帮助与版本是「提前返回」的伪命令：`--help` / `-h` 出现在任何位置都立即给帮助，
//! 有位置参数时按该主题给帮助。

pub mod help;
pub mod model;
pub mod parse;
pub mod program;

pub use model::{
    AccountsAddArgs, AccountsAddKind, AgentServeArgs, Command, ConfigGetArgs, ConfigSetArgs,
    DiagArgs, Globals, InstallArgs, InstanceLaunchArgs, Invocation, LaunchArgs, LogsArgs,
    PackImportArgs, UpdateApplyArgs, UpdateCheckArgs,
};
pub use parse::parse;

/// 这条命令行是 CLI 调用，还是「不该由 CLI 接管」（GUI 启动 / 只有全局选项）？
///
/// GUI 二进制（`unlucky-launcher` / `unlucky-launcher-iced`）用它做分发判据，`ull` 自己的
/// `main` 不用——**判据只有这一处**，两个 GUI 各调一次，改语义不会漂。
///
/// 规则（刻意不列 GUI 的 flag 表：cli crate 不该知道 GUI 有什么参数）：
///
/// - 首个 token 是裸词（不以 `-` 开头）→ 一定在子命令位置（GUI 不接受位置参数，`--diff` /
///   `--screenshot` 的取值都排在 flag 之后）→ 接管。未知子命令也在这里，由 CLI 自己报错。
/// - 首个 token 是 flag → 只有当 [`parse`] 解析出一条**真命令**时才算 CLI 调用：`--help` /
///   `--version` / `--json instances list` 接管；`--data-dir X` 解析成 [`Command::Welcome`]
///   （没有子命令）、GUI 专属参数（`--screenshot` / `--renderer` / `--diff` …）根本解析失败，
///   两种情况都不接管。**「解析失败」必须留在调用方**：`--screenshot out.png` 也会让本解析器
///   报错，把它判成 CLI 就会让 GUI 的截图通道失效。
#[must_use]
pub fn looks_like_cli(args: &[String]) -> bool {
    match args.first() {
        None => false,
        Some(first) if !first.starts_with('-') => true,
        Some(_) => match parse(args) {
            Ok(invocation) => !matches!(invocation.command, Command::Welcome),
            Err(_) => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::looks_like_cli;

    fn cli(args: &[&str]) -> bool {
        looks_like_cli(&args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn no_args_is_not_a_cli_call() {
        assert!(!cli(&[]));
    }

    #[test]
    fn gui_flags_are_never_taken_over() {
        // CI 与验收脚本在用这些参数，一条都不能被当成子命令吃掉。
        assert!(!cli(&["--software-renderer"]));
        assert!(!cli(&["--screenshot", "out.png", "--data-dir", "/d/tmp/x"]));
        assert!(!cli(&["--screenshot-theme", "dark"]));
        assert!(!cli(&["--renderer=gpu", "--theme=dark"]));
        assert!(!cli(&["--scale-factor", "1.0"]));
        assert!(!cli(&["--force-state", "hover"]));
        assert!(!cli(&["--diff", "a.png", "b.png"]));
        assert!(!cli(&["--mock"]));
        // 只有 --data-dir 时解析成 Welcome（没有子命令），仍是 GUI 启动。
        assert!(!cli(&["--data-dir", "/d/tmp/x"]));
    }

    #[test]
    fn known_subcommands_are_taken_over() {
        for args in [
            vec!["instances", "list"],
            vec!["install", "1.20.1", "--dry-run"],
            vec!["launch", "1.20.1", "--dry-run"],
            vec!["accounts", "list"],
            vec!["sessions", "list"],
            vec!["logs", "abc", "--tail", "10"],
            vec!["diag", "abc"],
            vec!["config", "get"],
            vec!["java", "list"],
            vec!["agent", "config"],
            // 全局选项可以排在子命令前面
            vec!["--json", "instances", "list"],
        ] {
            assert!(cli(&args), "{args:?} 应该交给 CLI");
        }
    }

    #[test]
    fn help_and_version_are_taken_over() {
        assert!(cli(&["--version"]));
        assert!(cli(&["--help"]));
        assert!(cli(&["-h"]));
        assert!(cli(&["-V"]));
    }

    #[test]
    fn unknown_subcommands_are_left_to_the_cli_to_reject() {
        // 语义定案（README 两版同款）：未知子命令 → CLI 报「未知命令 + <名字> --help」并以 1 退出，
        // 不静默落到 GUI（否则脚本用户会以为参数生效了）。
        assert!(cli(&["lst"]));
        assert!(cli(&["intsances", "list"]));
    }
}
