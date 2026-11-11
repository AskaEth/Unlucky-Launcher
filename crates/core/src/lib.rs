//! Unlucky-Launcher 领域库（`unlucky_core`）。
//!
//! 本 crate **零 UI 依赖**：UI 框架（Slint）只允许出现在 `crates/app`。
//! 这是对 HMCL「HMCLCore 34 处 import javafx」架构债的正面修正，
//! 由 `.github/workflows/guards.yml` 的 `core-no-ui` job 在 CI 守门。
//!
//! core 对 UI 的回调只有三条通道（见 `DevDocs/PlanDoc/01-tech-stack-and-architecture.md` §3）：
//! 1. 不可变快照发布；2. 事件流（启动/下载/诊断 JSONL 事件）；3. 注入的显示服务接口（进度/确认）。
//!
//! 设计定稿索引：`DevDocs/PlanDoc/00-master-architecture.md`。
//! 各模块的设计文档在对应 `mod.rs` 的模块注释里点名。
//!
//! 当前状态：P0 脚手架，只有模块骨架与设计注释，实现按 M12–M15/M18 分批落地。

#![forbid(unsafe_code)]

pub mod addon;
pub mod auth;
pub mod diagnose;
pub mod download;
pub mod game;
pub mod launch;
pub mod platform;
pub mod setting;
pub mod update;
