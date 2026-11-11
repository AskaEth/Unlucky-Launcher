//! 导入结果：**三态 + 逐项原因**，给 CLI 的 `--json` 与 M109/M113 的页面直接消费。
//!
//! # 三态不是"成功/失败"两态（M111 任务原文要求）
//!
//! | 状态 | 判据 | 退出码（CLI） |
//! |------|------|---------------|
//! | [`ImportStatus::Success`] | 清单里每条文件都落地、没有冲突 | `0` |
//! | [`ImportStatus::Partial`] | 有"搞不到"的文件（[`MissingFile`]）或有保留用户文件的冲突（[`ConflictFile`]） | `3` |
//! | [`ImportStatus::Failed`] | 致命错误（[`ImportError`]），**已整包回滚** | `1` |
//!
//! 把"部分成功"记成 `Success` 是这条管线最容易犯的错：用户会以为 mods 齐了，启动才发现缺
//! 依赖。所以 [`ImportReport::finish_status`] 是**唯一**算状态的地方，字段私有语义由它收敛。
//!
//! # 分文件
//!
//! - [`keys`]：文案 key 常量表（稳定字符串，含 `ALL` 守门表）；
//! - [`items`]：条目类型（一条文件 / 一条冲突 / 一条缺失 / 一个 component / 加载器目标）；
//! - 本文件：三态、阶段、进度、以及总的 [`ImportReport`]。

pub mod items;
pub mod keys;

pub use items::{
    ConflictChoice, ConflictFile, ConflictReason, FileAction, FileSource, ImportIssue,
    ImportedFile, LoaderTarget, MissingFile, MissingReason, RegisteredInstance, RejectedComponent,
};

use std::path::PathBuf;

use serde::Serialize;

use super::error::ImportError;
use super::stage::RollbackOutcome;

/// 导入结果（三态 + 一档"用户取消"）。
///
/// 取消**单列一档、不是第四种失败**（M195）：它有自己的变体、自己的文案 key、自己的退出码，
/// 且 `ImportReport::error` 恒为 `None`。把两者并成一个错误变体，UI 就会把"用户自己点的"
/// 画成红的，脚本也会把"我改主意了"当成"装坏了"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    /// 全部落地、无冲突。
    Success,
    /// 有缺文件或有冲突（内容不完整，但实例可用）。
    Partial,
    /// 致命失败，已整包回滚。
    Failed,
    /// 用户主动取消：管线在检查点停下，目标目录与导入前一致（`error` 为 `None`）。
    Cancelled,
}

impl ImportStatus {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// 文案 key。
    #[must_use]
    pub const fn message_key(self) -> &'static str {
        match self {
            Self::Success => keys::STATUS_SUCCESS,
            Self::Partial => keys::STATUS_PARTIAL,
            Self::Failed => keys::STATUS_FAILED,
            Self::Cancelled => keys::STATUS_CANCELLED,
        }
    }

    /// CLI 退出码：`0` / `3` / `1` / `130`（`2` 留给内部错误，`1` 与用法错误同码但 JSON 里可区分）。
    ///
    /// `130` 是「被用户中断」的惯例码（`128 + SIGINT`）：**与失败不同码**，脚本不会把
    /// 「用户改主意」记成「装坏了」，也不会误判成成功。
    #[must_use]
    pub const fn exit_code(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::Partial => 3,
            Self::Failed => 1,
            Self::Cancelled => 130,
        }
    }
}

/// 导入阶段（进度回调按这个分组）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStage {
    /// 读取清单（开包 + 安全四检 + 探测 + 读成模型）。
    Probe,
    /// 解压 / 三方合并覆盖内容。
    Extract,
    /// 补全并下载远端文件。
    Fetch,
    /// 落地（临时目录 rename 或 journal 合并）。
    Commit,
    /// 登记实例。
    Register,
}

impl ImportStage {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Probe => "probe",
            Self::Extract => "extract",
            Self::Fetch => "fetch",
            Self::Commit => "commit",
            Self::Register => "register",
        }
    }

    /// 文案 key。
    #[must_use]
    pub const fn message_key(self) -> &'static str {
        match self {
            Self::Probe => keys::STAGE_PROBE,
            Self::Extract => keys::STAGE_EXTRACT,
            Self::Fetch => keys::STAGE_FETCH,
            Self::Commit => keys::STAGE_COMMIT,
            Self::Register => keys::STAGE_REGISTER,
        }
    }
}

/// 一条进度事件（同步回调，跨线程安全）。
#[derive(Debug, Clone, Serialize)]
pub struct ImportProgress {
    /// 当前阶段。
    pub stage: ImportStage,
    /// 阶段内已完成条目数。
    pub done: usize,
    /// 阶段内总条目数（未知时为 0）。
    pub total: usize,
    /// 当前条目的实例内路径（阶段刚开始时为 `None`）。
    pub current: Option<String>,
}

/// 进度回调。
pub type ImportProgressSink = std::sync::Arc<dyn Fn(&ImportProgress) + Send + Sync>;

/// 发一条进度事件（`sink` 为 `None` 时什么都不做）。
///
/// 放在这里是因为它是 [`ImportProgress`] 的唯一出口：编排、阶段实现、提交事务都走它，
/// 谁都不许自己拼一条进度事件。
pub(crate) fn tell(
    sink: &Option<ImportProgressSink>,
    stage: ImportStage,
    done: usize,
    total: usize,
    current: Option<String>,
) {
    if let Some(sink) = sink {
        sink(&ImportProgress {
            stage,
            done,
            total,
            current,
        });
    }
}

/// 一次导入的完整结果。
#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    /// 三态。
    pub status: ImportStatus,
    /// 命中的格式 id（`curseforge` / `modrinth` / `multimc`）。
    pub format: Option<String>,
    /// 包名。
    pub pack_name: Option<String>,
    /// 包版本。
    pub pack_version: Option<String>,
    /// 游戏版本。
    pub game_version: Option<String>,
    /// 加载器安装计划输入（没有加载器时 `None` = 纯原版）。
    pub loader: Option<LoaderTarget>,
    /// 实例绑定的版本 id。
    pub version_id: Option<String>,
    /// 目标实例目录。
    pub target: PathBuf,
    /// 落地的文件（含 `SkippedIdentical`）。
    pub files: Vec<ImportedFile>,
    /// 冲突清单。
    pub conflicts: Vec<ConflictFile>,
    /// 缺文件清单（容忍）。
    pub missing: Vec<MissingFile>,
    /// MultiMC 不支持的 component（逐项）。
    pub rejected_components: Vec<RejectedComponent>,
    /// 注记。
    pub warnings: Vec<ImportIssue>,
    /// 致命错误（`status == Failed` 时必有）。
    pub error: Option<ImportError>,
    /// 登记的实例。
    pub registered: Option<RegisteredInstance>,
    /// 回滚结果（只有真的执行过回滚才有）。[`ImportReport::rolled_back`] 只在它**完整**时为真。
    pub rollback: Option<RollbackOutcome>,
    /// `instance.cfg` 里受开关门控的内存上限（MB；仅 MultiMC / Prism 有）。
    pub max_memory_mb: Option<u64>,
    /// 本次写入的字节数。
    pub bytes_written: u64,
    /// 是否已确认"目标目录与导入前一致"：回滚**完整**完成、或从未写过目标目录时为 `true`；
    /// 回滚有失败项时为 `false`（失败项在 [`ImportReport::rollback`] 里）。
    pub rolled_back: bool,
}

impl ImportReport {
    /// 一次尚未发生任何事的空报告（失败路径的起点）。
    #[must_use]
    pub fn empty(target: PathBuf) -> Self {
        Self {
            status: ImportStatus::Success,
            format: None,
            pack_name: None,
            pack_version: None,
            game_version: None,
            loader: None,
            version_id: None,
            target,
            files: Vec::new(),
            conflicts: Vec::new(),
            missing: Vec::new(),
            rejected_components: Vec::new(),
            warnings: Vec::new(),
            error: None,
            registered: None,
            rollback: None,
            max_memory_mb: None,
            bytes_written: 0,
            rolled_back: false,
        }
    }

    /// 真正写了盘的文件数。
    #[must_use]
    pub fn installed(&self) -> usize {
        self.files.iter().filter(|file| file.action.wrote()).count()
    }

    /// 是否"部分成功"。
    ///
    /// 判据是**结果对用户是否干净**：有"搞不到"的文件，或有保留了用户文件的冲突（说明整合包
    /// 与磁盘内容不一致）都不是干净的成功。用户选了"覆盖"的冲突不算——那侧的意图已落实。
    #[must_use]
    pub fn is_partial(&self) -> bool {
        !self.missing.is_empty()
            || self
                .conflicts
                .iter()
                .any(|conflict| conflict.choice == ConflictChoice::KeepUser)
    }

    /// 按 `error` / `missing` / `conflicts` 收敛三态（**唯一**算状态的地方）。
    pub fn finish_status(&mut self) {
        self.status = if self.error.is_some() {
            ImportStatus::Failed
        } else if self.is_partial() {
            ImportStatus::Partial
        } else {
            ImportStatus::Success
        };
    }

    /// 记一条致命错误并把状态落到 `Failed`。
    pub fn fail(&mut self, error: ImportError) {
        self.error = Some(error);
        self.status = ImportStatus::Failed;
    }

    /// 取消收尾：状态落到 [`ImportStatus::Cancelled`]，并说准**目标目录现在的样子**。
    ///
    /// 与 [`ImportReport::fail`] 是**两条路**（M195 的守门条款③）：取消不是失败，`error` 保持
    /// `None`，UI 不弹红、CLI 退出码 `130`。把两者并成一个错误变体，这一条就守不住。
    ///
    /// `outcome` 是取消之前已经做过的回滚动作（落盘之前取消 ⇒ `attempted == 0`，即"没写过"）。
    /// 三档文案与失败路径**同形**：没写过 / 回滚干净 / 回滚没做完——回滚不完整时同样不许宣称
    /// "与导入前一致"。
    pub fn cancel(&mut self, outcome: RollbackOutcome) {
        // 取消前"处理过"的条目一条都没进目标目录（暂存目录已删 / 已回滚）：留着它们会让
        // `installed()` / `bytes_written` 谎报"装了多少"，所以清空并把这笔账写进注记。
        let handled = self.files.len();
        self.files.clear();
        self.conflicts.clear();
        self.missing.clear();
        self.bytes_written = 0;
        self.error = None;
        self.status = ImportStatus::Cancelled;

        if outcome.attempted == 0 {
            // 落盘之前就停了：目标目录一个字节都没动。
            self.rolled_back = true;
            self.warnings.push(ImportIssue::new(
                keys::NOTE_CANCELLED,
                format!(
                    "已取消：目标目录未被写入（{}）；已处理的 {handled} 条文件未落地",
                    self.target.display()
                ),
            ));
            return;
        }

        if outcome.is_complete() {
            self.rolled_back = true;
            self.warnings.push(ImportIssue::new(
                keys::NOTE_CANCELLED,
                format!(
                    "已取消：已回滚 {} 项，目标目录恢复为导入前的样子（{}）",
                    outcome.attempted,
                    self.target.display()
                ),
            ));
        } else {
            // 回滚本身失败：与失败路径同一口径——不许在"可能没回干净"时宣称一致。
            self.rolled_back = false;
            self.warnings.push(ImportIssue::new(
                keys::NOTE_ROLLBACK_INCOMPLETE,
                format!(
                    "取消时回滚有 {} 项未完成（目标目录可能与导入前不同）：{}",
                    outcome.failure_count(),
                    outcome.failed.join("；")
                ),
            ));
            self.warnings.push(ImportIssue::new(
                keys::NOTE_CANCELLED,
                format!("已取消：已处理 {handled} 条文件、回滚未完整（详见 rollback.failed）"),
            ));
        }
        self.rollback = Some(outcome);
    }
}
