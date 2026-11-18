//! `AppCommand` 的**构造器**（从 `dto/events.rs` 搬出：那边 406 行越了 400 行硬线）。
//!
//! **纯搬移**：函数体、文档、可见性一字未改，只是换了文件；`impl AppCommand` 的另一个块，
//! 于是调用点 `AppCommand::set_isolation(..)` / `AppCommand::set_launch_args(..)` 一个字都不用改。
//! 这样 `events.rs` 只留两个枚举本体（`AppCommand` / `CoreEvent`），M224 才能在里面加
//! `Mods` 变体而不越线。

use crate::dto::{AppCommand, LaunchArgsPatch};

impl AppCommand {
    /// 构造「切换隔离档位」指令（磁贴/详情页共用）。
    pub fn set_isolation(id: &str, mode: &str) -> Self {
        Self::SetIsolation {
            id: id.to_string(),
            mode: mode.to_string(),
        }
    }

    /// 构造「写实例启动参数」指令（详情页「启动参数」区）。
    pub fn set_launch_args(id: &str, patch: LaunchArgsPatch) -> Self {
        Self::SetInstanceLaunchArgs {
            id: id.to_string(),
            patch,
        }
    }
}
