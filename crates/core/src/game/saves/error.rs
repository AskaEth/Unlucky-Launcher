//! 「目录存在但不可用」的失败形状（带 OS 原文）。
//!
//! 只有**一层**：`<run_dir>/saves` 或 `<run_dir>/screenshots` 本身不可用。项目级的问题
//! （某个世界目录读不出元数据、某个文件不是图片）不是错误，逐条进 `skipped`——列表宁可短，
//! 也不能因为一个坏文件整份读不出来。
//!
//! 为什么不折叠成 `io::Error`：调用方要分得清「这个实例没有存档目录」与「有但读不了」，
//! 前者是空列表加一句说明，后者是带重试按钮的红字。`dir` 字段是这两者的唯一区分依据。

use std::fmt;
use std::path::PathBuf;

/// 一次列举失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavesError {
    /// 出问题的那个目录（`<run_dir>/saves` 或 `<run_dir>/screenshots`）。
    pub dir: PathBuf,
    /// 人读原因（OS 原文或"是符号链接"这类判据）。
    pub detail: String,
}

impl SavesError {
    /// 建一条（内部用；`dir` 一律是**被列的那个目录**，不是它的父目录）。
    pub(super) fn unusable(dir: &std::path::Path, detail: impl Into<String>) -> Self {
        Self {
            dir: dir.to_path_buf(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for SavesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "目录不可用（{}）：{}", self.dir.display(), self.detail)
    }
}

impl std::error::Error for SavesError {}
