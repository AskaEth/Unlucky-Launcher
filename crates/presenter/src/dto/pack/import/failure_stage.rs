//! 致命失败发生在哪一步（M161：`rolled_back` 一个人说不了两件事）。
//!
//! 从父模块拆出来只为行数；判据与文案口径原样。

use serde::{Deserialize, Serialize};

/// 致命失败的**阶段**（M161：`rolled_back` 一个人说不了两件事）。
///
/// core 的 `ImportReport::rolled_back` 语义是「目标目录已确认与导入前一致」——它同时覆盖
/// 「回滚完整完成」和「**从未写过**目标目录」两种情形（见 `core::addon::import::pipeline` 的
/// `record_rollback` / `abandoned`）。UI 拿一个 bool 判不出该说哪一句，于是预检失败也会被
/// 说成「已回滚」，而实际上一个字节都没写。这个枚举把两种情形重新分开，并补上第三种
/// （回滚有失败项，目标目录可能与导入前不同）。
///
/// 只在 `status == Failed` 时有值：成功/部分成功的报告没有「失败阶段」可言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportFailureStageView {
    /// 预检/装配阶段就失败了：**目标目录一个字节都没写**（用户该去修包/修配置再试）。
    NotStarted,
    /// 已经写过目标目录，回滚**完整**完成（零残留）。
    RolledBack,
    /// 写过了，但回滚有失败项：目标目录可能与导入前不同（`warnings` 里逐条列了）。
    RollbackIncomplete,
}

impl ImportFailureStageView {
    /// 稳定 slug（日志 / 测试用；**不要改名**）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::RolledBack => "rolled_back",
            Self::RollbackIncomplete => "rollback_incomplete",
        }
    }

    /// 本次失败到底动没动目标目录。
    #[must_use]
    pub const fn touched_target(self) -> bool {
        !matches!(self, Self::NotStarted)
    }
}
