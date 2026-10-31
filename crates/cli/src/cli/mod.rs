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

pub use model::{
    AccountsAddArgs, AccountsAddKind, AgentServeArgs, Command, ConfigGetArgs, ConfigSetArgs,
    DiagArgs, Globals, InstallArgs, Invocation, LaunchArgs, LogsArgs,
};
pub use parse::parse;
