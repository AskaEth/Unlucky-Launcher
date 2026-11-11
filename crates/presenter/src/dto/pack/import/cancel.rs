//! 一次导入与「取消」的关系（**真实结局**，不是"用户点没点过取消"）。
//!
//! 从父模块拆出来只为行数（父文件已到 400 硬限附近），判据与文案口径原样。
//!
//! 三档各有各的话要说，混成一句就是撒谎（M195 r3）：
//! - [`ImportCancelView::RolledBack`]：停手了，目标目录与导入前一致；
//! - [`ImportCancelView::RollbackIncomplete`]：停手了，但回滚没做完，目标目录可能与导入前不同；
//! - [`ImportCancelView::TooLate`]：**没来得及停**，管线已经跑完——实例会保留，同名文件可能已被覆盖。

use serde::{Deserialize, Serialize};

/// 一次导入与「取消」的关系（**真实结局**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ImportCancelView {
    /// 这次运行没被取消过。
    #[default]
    NotRequested,
    /// 取消生效，管线停手并**完整**回滚（目标目录与导入前一致，实例没登记）。
    RolledBack,
    /// 取消生效，但回滚没做完（目标目录可能与导入前不同，逐条在 `warnings` 里）。
    RollbackIncomplete,
    /// 取消到得太晚：管线已经跑完，实例保留，同名文件可能已被覆盖。
    TooLate,
}

impl ImportCancelView {
    /// 稳定 slug。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::NotRequested => "not_requested",
            Self::RolledBack => "rolled_back",
            Self::RollbackIncomplete => "rollback_incomplete",
            Self::TooLate => "too_late",
        }
    }

    /// 用户这次取消有没有**真的生效**（前两档是真停手了）。
    #[must_use]
    pub const fn stopped(self) -> bool {
        matches!(self, Self::RolledBack | Self::RollbackIncomplete)
    }
}
