//! 安装报告与进度回调的形状（M105 从 `execute.rs` 搬出，守 400 行硬线）。
//!
//! 纯数据 + 纯方法：`execute.rs`（执行）与 CLI（汇总打印）都读它，谁也不改它。
//! 搬移是**逐字**的，只有 `use` 断了回来（原来靠 `execute.rs` 的 import 活着）。

use std::path::PathBuf;
use std::sync::Arc;

use crate::download::DownloadErrorKind;

use super::plan::FileRole;

/// 单个文件的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// 本次真的下载（或从缓存直取）并落盘。
    Downloaded,
    /// 已存在且校验通过，跳过。
    Skipped,
    /// 失败（原因在 [`FileOutcome::failure`]）。
    Failed,
}

impl FileStatus {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Downloaded => "downloaded",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

/// 一次失败的分类信息（调用方据此给 `install_network` / `install_integrity` 等稳定码）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFailure {
    /// 下载错误分类。
    pub kind: DownloadErrorKind,
    /// 人读说明。
    pub message: String,
}

/// 一个文件的执行结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOutcome {
    /// 角色。
    pub role: FileRole,
    /// 人读标识。
    pub name: String,
    /// 落盘路径。
    pub dest: std::path::PathBuf,
    /// 结局。
    pub status: FileStatus,
    /// 实际字节数（跳过时为盘上大小）。
    pub bytes: u64,
    /// 成功的源标识（`bmclapi` / `official` / `cache` / `metadata`）。
    pub source: Option<String>,
    /// 失败详情。
    pub failure: Option<FileFailure>,
}

impl FileOutcome {
    /// 是否失败。
    #[must_use]
    pub fn failed(&self) -> bool {
        self.status == FileStatus::Failed
    }
}

/// 进度回调：每完成一个文件调用一次（可跨线程，由执行器持有 `Arc`）。
///
/// CLI 用它往 stderr 打实时进度；GUI 侧直接消费 `ProgressEvent` 事件流即可，不必用这个。
pub type ProgressSink = Arc<dyn Fn(&FileOutcome) + Send + Sync>;

/// 一次安装的执行结果。
#[derive(Debug, Clone)]
pub struct InstallReport {
    /// 逐文件结果（顺序不保证，按角色/大小汇总即可）。
    pub files: Vec<FileOutcome>,
    /// 版本清单落盘路径；`None` = 有文件失败，故意没写（见 `execute` 的说明）。
    pub version_json: Option<PathBuf>,
    /// 计划阶段带来的告警（缺件降级说明）。
    pub warnings: Vec<String>,
}

impl InstallReport {
    /// 真正下载的文件数。
    #[must_use]
    pub fn downloaded(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.status == FileStatus::Downloaded)
            .count()
    }

    /// 跳过（已存在且校验通过）的文件数。
    #[must_use]
    pub fn skipped(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.status == FileStatus::Skipped)
            .count()
    }

    /// 失败的文件数。
    #[must_use]
    pub fn failed(&self) -> usize {
        self.files.iter().filter(|file| file.failed()).count()
    }

    /// 本次写入的字节数（只算真正下载的）。
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.files
            .iter()
            .filter(|file| file.status == FileStatus::Downloaded)
            .map(|file| file.bytes)
            .sum()
    }

    /// 是否全部就绪（无失败）。
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.failed() == 0
    }

    /// 失败文件列表（最多取前 `limit` 条，给 CLI 拼提示用）。
    #[must_use]
    pub fn failures(&self, limit: usize) -> Vec<&FileOutcome> {
        self.files
            .iter()
            .filter(|file| file.failed())
            .take(limit)
            .collect()
    }

    /// 失败里最"严重"的大类：有校验不符就先报它，其余按网络/IO 归。
    #[must_use]
    pub fn failure_kind(&self) -> Option<DownloadErrorKind> {
        let mut fallback = None;
        for file in self.files.iter().filter(|file| file.failed()) {
            let Some(failure) = &file.failure else {
                continue;
            };
            if failure.kind == DownloadErrorKind::Integrity {
                return Some(DownloadErrorKind::Integrity);
            }
            fallback.get_or_insert(failure.kind);
        }
        fallback
    }
}
