//! 报告里**一条一条列出来的东西**：一条注记（带级别）与一个「搞不到」的文件（带成因）。
//!
//! ## 这一族放什么
//!
//! **一行一条、可逐条数**的东西：`ImportIssueView`（报告里的一句注记，带级别）与
//! `MissingFileView` / `MissingReasonView`（报告里的一条缺件，带成因）。它们的共同点是
//! 「`ImportReportView` 里那些 `Vec<…>` 的元素」——界面把每一个都画成一行。
//!
//! **不放**整份报告的属性：三态 / 阶段 / 取消三档 / processor 那一段各有自己的文件
//! （`super` 与 `processor.rs`），它们描述的是**整次导入**，不是一行。
//!
//! ## 下一件东西该加在哪
//!
//! 新的一种「逐条」条目（例如将来的「被覆盖的文件」）加在这里；新的一种「整份报告属性」加到
//! `super`（或像 `processor.rs` 那样自成一段）。`ImportReportView` 里出现新的 `Vec<T>` 时，
//! `T` 十有八九属于这里。
//!
//! 搬移说明：`MissingReasonView` / `MissingFileView` 是从 `super`（`dto/pack/import.rs`，加
//! M339 期间正好顶到 400 行）**原样**搬来的，`super` 的 `pub use` 让外部路径一个字都没变。

use serde::{Deserialize, Serialize};

/// 一条注记的**级别**（M339：core 数据里就写着，界面与命令行按它分档）。
///
/// 与 core 的 `ImportIssueLevel` 一一对应（`warning` / `note`）。**界面不许再按 `message_key`
/// 的前缀猜**：`pack.import.note.*` 前缀下既有「加载器没装」（告警）也有「登记成功」（注记）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportIssueLevelView {
    /// 要用户处理：东西没装上 / 可能起不来 / 语义降级。
    Warning,
    /// 事实陈述：成功确认、有意跳过、可选优化。
    Note,
}

impl ImportIssueLevelView {
    /// 稳定 slug（`--json` 与日志用）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }

    /// 从 slug 反解（`None` = 不认识的档位，**不猜**）。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "warning" => Some(Self::Warning),
            "note" => Some(Self::Note),
            _ => None,
        }
    }

    /// 是告警吗（分档标题与计数只认它）。
    #[must_use]
    pub const fn is_warning(self) -> bool {
        matches!(self, Self::Warning)
    }
}

/// 一条注记（级别 + core 写的那句人话）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportIssueView {
    /// 级别。
    pub level: ImportIssueLevelView,
    /// 人读细节。
    pub message: String,
}

impl ImportIssueView {
    /// 一条告警。
    #[must_use]
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            level: ImportIssueLevelView::Warning,
            message: message.into(),
        }
    }

    /// 一条注记。
    #[must_use]
    pub fn note(message: impl Into<String>) -> Self {
        Self {
            level: ImportIssueLevelView::Note,
            message: message.into(),
        }
    }
}

/// 一个文件「搞不到」的成因（**容忍**域）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissingReasonView {
    /// 调用方没给 fetcher（离线导入）。
    Offline,
    /// 条目补不出下载地址（CF 缺 API 客户端 / `downloads` 为空）。
    NoSource,
    /// 尝试过但失败（404 / 网络）。
    FetchFailed,
}

impl MissingReasonView {
    /// 从 core 的 id 反解。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "offline" => Some(Self::Offline),
            "no_source" => Some(Self::NoSource),
            "fetch_failed" => Some(Self::FetchFailed),
            _ => None,
        }
    }
}

/// 一个「搞不到」的文件（容忍，但逐条列出）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingFileView {
    /// 实例内路径（CF 补全失败时没有）。
    pub path: Option<String>,
    /// 人读标识（CF 条目是 `projectID:fileID`，其余是路径）。
    pub display: String,
    /// 为什么搞不到。
    pub reason: MissingReasonView,
    /// 细节（错误原文）。
    pub detail: String,
}
