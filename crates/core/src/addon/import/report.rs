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
//! - [`issue`] / [`items`]：一条注记（**级别** + 稳定 key + 人读细节，M339）与条目类型（文件 / 冲突 / 缺失 / component / 加载器目标 / processor 步骤）；
//! - [`progress`]：进度事件与它的唯一出口；
//! - 本文件：三态、阶段、以及总的 [`ImportReport`]。

pub mod issue;
pub mod items;
pub mod keys;
pub mod progress;

pub use issue::{ImportIssue, ImportIssueLevel};
pub use items::{
    ConflictChoice, ConflictFile, ConflictReason, FileAction, FileSource, ImportedFile,
    LoaderTarget, MissingFile, MissingReason, ProcessorStep, ProcessorSummary, RegisteredInstance,
    RejectedComponent, MISSING_OFFLINE_DETAIL,
};
pub(crate) use progress::{tell, tell_to};
pub use progress::{ImportProgress, ImportProgressSink};

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
    /// 装清单声明的加载器（M311 第六跳：官方元数据 → 原版前置 → 库 → 版本 JSON）。
    ///
    /// **排在 `Commit` 之前**：装载器只写共享分区（`versions/` `libraries/` `assets/`），
    /// 与实例内容目录无关，所以它先跑（见 `super::before_commit`）。枚举的**声明顺序 = 实际
    /// 发射顺序**，`Ord` 就是"阶段往前推进"这个事实，界面步号按它算。
    Loader,
    /// 落地（临时目录 rename 或 journal 合并）。
    Commit,
    /// 登记实例。
    Register,
}

impl ImportStage {
    /// 六个阶段按**实际执行顺序**排列（等号左=声明顺序，右边=管线真跑的顺序）。
    ///
    /// 这是顺序判据的唯一真相源：`addon::import::loader::tests` 会拿真实发射出来的阶段序列
    /// 与它逐条比对（`Loader` 在 `Commit` 之前）。往管线里插新阶段、或把某段挪位置时，
    /// **这里不跟着改就必红**。
    pub const ALL: [ImportStage; 6] = [
        ImportStage::Probe,
        ImportStage::Extract,
        ImportStage::Fetch,
        ImportStage::Loader,
        ImportStage::Commit,
        ImportStage::Register,
    ];

    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Probe => "probe",
            Self::Extract => "extract",
            Self::Fetch => "fetch",
            Self::Loader => "loader",
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
            Self::Loader => keys::STAGE_LOADER,
            Self::Commit => keys::STAGE_COMMIT,
            Self::Register => keys::STAGE_REGISTER,
        }
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
    /// 清单声明的加载器**这次真的装上了**吗（M311 第六跳）。
    ///
    /// `false` 有两种来源，都由注记说清：①清单没有加载器（纯原版包，没什么可装）；
    /// ②清单声明了加载器但这次没装（没接安装器），此时 [`ImportReport::loader`] 为 `Some`
    /// 而这一位为 `false` —— 界面据此提示「这个版本的加载器还没装」，不许画成一切正常。
    /// 装不上的情形**不落在这里**：那是致命失败（`error` 的 `import_loader`），实例压根不登记。
    pub loader_installed: bool,
    /// 第六跳 processor 链的逐步骤结果（M321）：`None` = 这次没有 processor 段，
    /// `Some` 时**每一步都在里面**（含跳过的步）。「链跑完了却什么也不说」正是本 mission
    /// 要消灭的缺口，界面与 `--json` 都读这一位。
    pub processors: Option<ProcessorSummary>,
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
    /// 注记（**每条带级别**：`Warning` = 要用户处理，`Note` = 事实陈述；M339）。
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
            loader_installed: false,
            processors: None,
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

    /// 记一条「清单声明的加载器**真的装好了**」（第六跳成功时由管线调）。
    ///
    /// 机器位与文案**同源**：`loader_installed` 只在这里与
    /// [`ImportReport::note_loader_not_installed`] 里改写，于是「界面看到的」与「报告里的」
    /// 不可能是两个答案。
    pub fn note_loader_installed(&mut self, target: &LoaderTarget, detail: &str) {
        self.loader_installed = true;
        self.warnings.push(ImportIssue::note(
            keys::NOTE_LOADER_INSTALLED,
            format!(
                "已装好清单声明的加载器：{detail}（启动时会用 {}）",
                target.version_id
            ),
        ));
    }

    /// 记一条「清单要求加载器、**这次没装**」的注记。
    ///
    /// 这条注记就是 M311 要消灭的那个假功能的反面：以前报告里只有一行「加载器：fabric 0.15.11」
    /// 看着像装好了，点启动才报 `VersionManifest`。现在没装就明说没装、说清是哪个版本没装。
    pub fn note_loader_not_installed(&mut self, target: &LoaderTarget) {
        self.loader_installed = false;
        self.warnings.push(ImportIssue::warning(
            keys::NOTE_LOADER_NOT_INSTALLED,
            format!(
                "清单要求 {} {}，但这次导入没装加载器（只登记了计划）：{} 还启动不了，先把它装上",
                target.kind.label(),
                target.loader_version,
                target.version_id
            ),
        ));
    }

    /// 记一条「加载器解析期 / 计划期」的告警（`release.warnings` 与 `plan.warnings()`）。
    ///
    /// M321 之前这两份被整包丢弃：导入照样报成功，「某个库缺下载地址、已降级」这类事实从不到用户眼前。现在逐条进 [`ImportReport::warnings`]（人读与 `--json` 同一出口），**级别是告警**（M339：库装不全，用户得知道）。
    pub fn note_loader_warnings(&mut self, warnings: &[String]) {
        for warning in warnings {
            let issue = ImportIssue::warning(keys::NOTE_LOADER_WARNING, warning.clone());
            self.warnings.push(issue);
        }
    }

    /// 记下第六跳 processor 链的结局：`processors` 只在这里改写（界面与报告不可能是两个答案），
    /// processor 期的告警逐条铺进 [`ImportReport::warnings`]（`NOTE_PROCESSOR_WARNING`）。
    ///
    /// **同一来源只许进一次**：`ProcessorRun::warnings` 归这里，计划期的归
    /// [`ImportReport::note_loader_warnings`]——两条都铺一遍会让同一条告警出现两次。
    pub fn note_processors(&mut self, summary: ProcessorSummary) {
        for warning in &summary.warnings {
            self.warnings.push(ImportIssue::warning(
                keys::NOTE_PROCESSOR_WARNING,
                warning.clone(),
            ));
        }
        self.processors = Some(summary);
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
            self.warnings.push(ImportIssue::note(
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
            self.warnings.push(ImportIssue::note(
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
            self.warnings.push(ImportIssue::warning(
                keys::NOTE_ROLLBACK_INCOMPLETE,
                format!(
                    "取消时回滚有 {} 项未完成（目标目录可能与导入前不同）：{}",
                    outcome.failure_count(),
                    outcome.failed.join("；")
                ),
            ));
            self.warnings.push(ImportIssue::note(
                keys::NOTE_CANCELLED,
                format!("已取消：已处理 {handled} 条文件、回滚未完整（详见 rollback.failed）"),
            ));
        }
        self.rollback = Some(outcome);
    }
}
