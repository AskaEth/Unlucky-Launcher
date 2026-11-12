//! Unlucky-Launcher 命令行（`unlucky_cli`）：`ull` 二进制的可测试内核。
//!
//! 设计定稿：`DevDocs/PlanDoc/10-cli-design.md`（D18）—— core 是零 UI 纯库，
//! CLI 只是与 Slint GUI 平级的第二个消费者，因此**本 crate 不依赖任何 UI 框架**。
//!
//! 分层：
//! - [`cli`]：手写参数解析（不引 clap）+ 命令模型 + 帮助文案；
//! - [`commands`]：每个子命令一个函数，输入 [`context::Ctx`]，输出 [`output::Report`]；
//! - [`download`]：CLI 侧的下载引擎（设置 store → core 调优，`install`/`java`/`pack` 共用）；
//! - [`context`]：数据根解析（三模式 / `--data-dir` 覆盖）与目录视图；
//! - [`os`]：系统摘要与 D19/D21 支持级别（事实来自 core::platform::os_info，这里只做呈现）；
//! - [`output`]：人类可读 + `--json` 双格式输出；
//! - [`error`]：错误形状与退出码（0 成功 / 1 用户错误 / 2 内部错误）。
//!
//! 可测试性：入口是 [`run`]，参数是 `&[String]`，输出写进两个 `&mut dyn Write`，
//! 返回值就是进程退出码——集成测试因此不需要真的起子进程也能覆盖三条通道。

#![forbid(unsafe_code)]

pub mod cli;
pub mod commands;
pub mod context;
pub mod download;
pub mod error;
pub mod os;
pub mod output;
pub mod runner;

pub use runner::run;
