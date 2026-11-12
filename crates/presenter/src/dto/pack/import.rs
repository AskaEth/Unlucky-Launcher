//! 导入侧 DTO：请求、进度、三态、逐项原因、探测诊断。
//!
//! 状态档（`Success` / `Partial` / `Failed` / `Cancelled`）与 core 一一对应，**`Partial` 不是
//! `Success`**、**取消不是失败**；容忍域与致命域分属不同类型，UI 不可能混成一个列表。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::dto::InstanceId;

use super::conflict::{ConflictDecisionsView, ConflictFileView, ImportScanView};
use super::format::PackFormatView;

mod cancel;
mod failure_stage;

pub use cancel::ImportCancelView;
pub use failure_stage::ImportFailureStageView;

/// 导入阶段（与 core 的 `ImportStage` 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ImportStageView {
    /// 读取清单（开包 + 安全四检 + 探测）。
    Probe,
    /// 解压 / 三方合并覆盖内容。
    Extract,
    /// 补全并下载远端文件。
    Fetch,
    /// 落地。
    Commit,
    /// 登记实例。
    Register,
}

impl ImportStageView {
    /// 五个阶段的声明顺序（= 进度条上的先后）。
    pub const ALL: [ImportStageView; 5] = [
        ImportStageView::Probe,
        ImportStageView::Extract,
        ImportStageView::Fetch,
        ImportStageView::Commit,
        ImportStageView::Register,
    ];

    /// 稳定 slug。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Probe => "probe",
            Self::Extract => "extract",
            Self::Fetch => "fetch",
            Self::Commit => "commit",
            Self::Register => "register",
        }
    }

    /// 从 core 的 id 反解。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|stage| stage.slug() == slug)
    }

    /// 这一阶段有没有「逐文件」的进度（`Probe` 只看清单，没条目可数）。
    #[must_use]
    pub const fn counts_files(self) -> bool {
        !matches!(self, Self::Probe)
    }
}

/// 导入三态（`Partial` **不是** `Success`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportStatusView {
    /// 全部落地、无冲突。
    Success,
    /// 有缺文件或有冲突（内容不完整，但实例可用）。
    Partial,
    /// 致命失败。
    ///
    /// **「动没动过目标目录」不在这里**（M161）：同一个 `Failed` 既可能是预检没过（一个字节都没写），
    /// 也可能是写完又回滚干净，还可能是回滚没做完——三种由
    /// [`ImportReportView::failure_stage`] 分开说，界面按它选标题与图标。
    Failed,
    /// **用户取消，管线停手并回滚**（M195 r3）：不是失败，也不是「部分成功」。
    /// 「回滚干净 / 没做完」由 [`ImportReportView::cancel`] 再分；取消**太晚**（管线已跑完）
    /// 不落这一档——那时实例真装上了，状态是 `Success`/`Partial`，只在 `cancel` 记 `TooLate`。
    Cancelled,
}

impl ImportStatusView {
    /// 稳定 slug。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// 从 core 的 id 反解。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "success" => Some(Self::Success),
            "partial" => Some(Self::Partial),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// 实例可用吗（`Success` / `Partial` 是「装上了」；取消与失败都没装上）。
    #[must_use]
    pub const fn installed(self) -> bool {
        matches!(self, Self::Success | Self::Partial)
    }
}

/// 一条落地文件本次被怎么处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileActionView {
    /// 新写入。
    Installed,
    /// 覆盖写入（用户没改过）。
    Replaced,
    /// 与包内一致，没动。
    SkippedIdentical,
    /// 用户改过 / 删过，按策略保留了用户那一侧。
    KeptUser,
}

impl FileActionView {
    /// 从 core 的 id 反解。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "installed" => Some(Self::Installed),
            "replaced" => Some(Self::Replaced),
            "skipped_identical" => Some(Self::SkippedIdentical),
            "kept_user" => Some(Self::KeptUser),
            _ => None,
        }
    }

    /// 是否真的写了盘。
    #[must_use]
    pub const fn wrote(self) -> bool {
        matches!(self, Self::Installed | Self::Replaced)
    }
}

/// 一次导入的请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRequestView {
    /// 包路径：zip（`.zip` / `.mrpack`）或 MultiMC 实例目录。
    pub pack: PathBuf,
    /// 目标实例（`None` = 按包名新建一个）。
    pub instance: Option<InstanceId>,
    /// 新实例的展示名（`None` = 用包名）。
    pub instance_name: Option<String>,
    /// 冲突决策（**缺省 [`ConflictDecisionsView::KeepAll`]** = 用户已有文件一个都不动）。
    ///
    /// 页面上：`KeepAll` 是缺省与「全部保留」，`OverwriteAll` 是「全部覆盖」，
    /// `PerItem` 是逐项勾选的结果。只有用户**显式点过**之后才会出现后两者。
    pub decisions: ConflictDecisionsView,
}

/// 一条导入进度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportProgressView {
    /// 阶段。
    pub stage: ImportStageView,
    /// 阶段内已完成条目数。
    pub done: u32,
    /// 阶段内总条目数（`0` = 未知）。
    pub total: u32,
    /// 当前条目的实例内路径（阶段刚开始时为 `None`）。
    pub current: Option<String>,
    /// 包名（面板标题那一行）。
    pub pack: String,
}

impl ImportProgressView {
    /// 空进度（刚派发，后端还没回第一条）。
    #[must_use]
    pub fn new(pack: impl Into<String>) -> Self {
        Self {
            stage: ImportStageView::Probe,
            done: 0,
            total: 0,
            current: None,
            pack: pack.into(),
        }
    }

    /// 计数已知吗（`false` = 不画确定态进度条，避免「静止的假进度」）。
    #[must_use]
    pub const fn is_known(&self) -> bool {
        self.total > 0
    }

    /// 阶段内完成比例 0.0~1.0（未知时 `0.0`）。
    #[must_use]
    pub fn ratio(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        (self.done as f32 / self.total as f32).clamp(0.0, 1.0)
    }
}

/// 一条落地文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedFileView {
    /// 实例内相对路径。
    pub path: String,
    /// 本次处置。
    pub action: FileActionView,
    /// 字节数。
    pub bytes: u64,
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

/// 一次致命失败（是不是「已整包回滚」看 [`ImportReportView::failure_stage`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportErrorView {
    /// 稳定码（`pack_traversal` / `import_integrity` …，进日志与 `--json`）。
    pub code: String,
    /// 人读大类名。
    pub label: String,
    /// 具体是什么。
    pub message: String,
    /// 补救建议（core 有就给）。
    pub hint: Option<String>,
}

/// 一次导入的完整结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReportView {
    /// 三态。
    pub status: ImportStatusView,
    /// 命中的格式（`None` = 探测都没过）。
    pub format: Option<PackFormatView>,
    /// 包名。
    pub pack_name: Option<String>,
    /// 包版本。
    pub pack_version: Option<String>,
    /// 游戏版本。
    pub game_version: Option<String>,
    /// 加载器（人读串，如 `fabric-loader 0.15.11`）。
    pub loader: Option<String>,
    /// 实例绑定的版本 id。
    pub version_id: Option<String>,
    /// 目标实例目录。
    pub target: PathBuf,
    /// 落地的文件。
    pub files: Vec<ImportedFileView>,
    /// 冲突清单。
    pub conflicts: Vec<ConflictFileView>,
    /// 缺文件清单（容忍）。
    pub missing: Vec<MissingFileView>,
    /// MultiMC 不支持的 component（逐项）。
    pub rejected: Vec<String>,
    /// 注记。
    pub warnings: Vec<String>,
    /// 致命错误（`status == Failed` 时必有）。
    pub error: Option<ImportErrorView>,
    /// 登记的实例名。
    pub registered: Option<String>,
    /// 本次写入的字节数。
    pub bytes_written: u64,
    /// 回滚是否完整（`false` = 目标目录可能与导入前不一致，逐条在 `warnings` 里）。
    ///
    /// **注意**：`true` 有两种来源——「回滚完整完成」与「从未写过目标目录」（core 的
    /// `abandoned`）。要区分它们看 [`ImportReportView::failure_stage`]；界面上的措辞
    /// **必须**按那个字段选（M161）。
    pub rolled_back: bool,
    /// 失败发生在哪一步（`None` = 这次不是致命失败）。
    pub failure_stage: Option<ImportFailureStageView>,
    /// 这次运行与「取消」的关系（M195 r3：取消的**真实结局**，界面按它选文案）。
    ///
    /// 只靠 `status` 表达不了三档：已回滚 / 回滚没做完都在 [`ImportStatusView::Cancelled`] 里，
    /// 而「取消太晚、已经跑完」的状态是 `Success`/`Partial`——三档见 [`ImportCancelView`]。
    pub cancel: ImportCancelView,
}

impl ImportReportView {
    /// 真正写盘的文件条数。
    #[must_use]
    pub fn written(&self) -> usize {
        self.files.iter().filter(|file| file.action.wrote()).count()
    }

    /// 要用户看一眼的东西条数（冲突 + 缺文件 + 被拒 component）。
    #[must_use]
    pub fn issue_count(&self) -> usize {
        self.conflicts.len() + self.missing.len() + self.rejected.len()
    }
}

/// 探测失败的**具名**诊断：「看起来像 X，但缺 Y」。
///
/// 为什么不由 core 给：core 的 `detect` 只回答「谁认领」，一个都不认领时只回一句
/// `pack_unsupported` + 试过哪些格式。而用户手里那个包往往**差一点点**就是某个已知格式
/// （CF 导出漏了 `manifest.json`、mrpack 被浏览器改名成 `.zip` 后解压丢了 index）。
/// 判断「像哪个」要看包内到底有哪些标记文件，这是宿主层拿着 `PackArchive` 才看得见的事。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeView {
    /// 最像哪个格式（`None` = 三个都不像）。
    pub looks_like: Option<PackFormatView>,
    /// 该格式**缺**的标记文件（逐条，按判据顺序）。
    pub missing: Vec<String>,
    /// 包里实际看到的标记（逐条）。
    pub saw: Vec<String>,
    /// 一句兜底说明（也是 core 的原文：试过哪些格式）。
    pub detail: String,
}

/// 导入侧的宿主状态（`AppState::import` 的全部内容）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportState {
    /// 正在跑的进度（`None` = 没在跑）。
    pub progress: Option<ImportProgressView>,
    /// 用户点了取消、**结果还没回来**（M195 r3 收窄了它的含义）。
    ///
    /// 它只作"取消后还没有结果"的占位：真实结局随后的 `report.cancel` 才是准话（已回滚 /
    /// 回滚没做完 / 取消太晚）。所以 `PackEvent::Report` 一到就清掉这一位，免得它跨到下一次
    /// 导入上替结局下结论。
    pub cancelled: bool,
    /// 最近一次的预检（M160：冲突清单；`None` = 还没预检过或已被新结果顶掉）。
    pub scan: Option<ImportScanView>,
    /// 最近一次的结果（`None` = 还没导过）。
    pub report: Option<ImportReportView>,
    /// 探测失败的具名诊断（有它就让用户看清「像 X 缺 Y」，而不是一句「无法识别」）。
    pub probe: Option<ProbeView>,
}
