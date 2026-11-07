//! 整合包**导入**（M111）：CurseForge `manifest.json` / Modrinth `.mrpack` / MultiMC·Prism 实例
//! → 一个可启动的实例目录。
//!
//! 设计依据：`DevDocs/PlanDoc/08-mod-and-recommend.md` §2（D10）、
//! `DevDocs/PlanDoc/research/18-modpack-formats*.md`（字段清单，A/B 级来源）、
//! `DevDocs/SubAgentsTalk/addon-整合包导入导出对接要点.md`（施工口径）。
//! 格式模型与安全闸是 M110 的 [`crate::addon::pack`]：本模块**只消费，不重写**。
//!
//! # 三条硬要求落在哪
//!
//! | 要求 | 落点 |
//! |------|------|
//! | overrides 哈希**三方合并**（不静默覆盖用户改动） | [`merge`] 的判据 + [`baseline`] 的 base 侧 |
//! | 两阶段补全 + **失败分级**（仅"缺文件"可容忍） | [`pipeline`] 的顺序 + [`report::MissingFile`] 与 [`error::ImportError`] 的分工 |
//! | 路径穿越**双检** | 开包时 M110 [`crate::addon::pack::PackArchive::open`]，落地时 [`stage`] 的 `normalize_file` + `starts_with` 复核 |
//!
//! # 三种格式的语义（tower 定案，与 HMCL 相反的两条要记住）
//!
//! - **mrpack**：`overrides/` 与 `client-overrides/` 都解压，**后者胜**（Modrinth 官方客户端语义；
//!   HMCL 恰好相反）；`server-overrides/` 客户端**不解压**；其余同名路径无官方优先级 ⇒ 按侧
//!   优先级落地，不自创排序。**同一实例路径被本包多份声明命中时**（跨侧或同侧重复），
//!   逐条算清楚：胜者按上面这条规则定，被遮蔽的那条不读字节、只留一条
//!   `pack.import.note.override_shadowed` 注记说明「谁赢了 / 两份内容是否逐字节相同」
//!   （见 [`plan::Shadowed`]）。一路径一行，不再出现"两条 Installed、前一条没落地"。
//! - **CF**：溢出目录名取自清单 `overrides` 字段；`files[]` 只有 `projectID/fileID`，要 CF API
//!   才能补出文件名/地址——仓库里还没有 CF 客户端（D10 §5），因此这部分条目的**当前正确行为**
//!   是记成 `missing`（`Partial`），**绝不猜 `edge.forgecdn.net` 那类 URL**。
//! - **MultiMC / Prism**：`mmc-pack.json` 的 `formatVersion` 必须恰为 `1`（M110 的读者会拒绝
//!   其余值）；`.minecraft/` 直拷；非加载器 component（`org.lwjgl3` 等）**逐项**进
//!   [`report::RejectedComponent`]，不静默丢；只有导入，没有导出。
//!
//! # `.disabled` 只按路径后缀
//!
//! `x.jar` / `x.jar.disabled` / `x.jar.old` / `x.jar.disabled.old` 四种变体都识别，后缀**原样
//! 保留**；`.old` 只识别与保留（不清除、不改名、不当回滚语义）。`env.client=optional` **不是**
//! 禁用——"可选"的语义是"安装时可取消勾选"，装回来默认仍然启用，两者不是一回事。
//!
//! # 用法
//!
//! ```ignore
//! let engine = /* core::download::DownloadEngine */;
//! let fetcher = DownloadEngineFetcher::new(&engine);
//! let request = ImportRequest::new(pack_path, instance_dir)
//!     .with_fetcher(&fetcher)
//!     .with_registration(&launcher_layout, None);
//! let report = download::blocking::block_on(import_pack(&request));
//! match report.status { /* Success / Partial / Failed */ }
//! ```
//!
//! # 刻意不做的事（分工边界）
//!
//! - **不删除**整合包升级后不再包含的文件：磁盘上的 `mods/` 里可能有用户自己装的模组，
//!   删旧文件是"清理"而不是"导入"（要做得先有"这个文件是整合包装的"的完整账本 + 用户确认）；
//! - **不装加载器**：只把清单翻译成 [`report::LoaderTarget`]（`kind` + 原版版本 + 加载器版本
//!   + 版本 id），真正的元数据解析/库下载仍是 `addon::loader` + `game::install` 的既有路径；
//! - **不碰 UI 类型**：core 零 UI 依赖（`core-no-ui` 门禁），报告只出现 `String` / `&'static str`；
//! - **不绕过下载引擎**：所有网络只经 [`fetch::PackFetcher`]，生产实现只有
//!   [`fetch::DownloadEngineFetcher`]（走 `core::download`）。
//!
//! # 没验的部分（真实世界的欠账，报告里同样登记）
//!
//! 真实 CF / MultiMC 导出物（夹具是手造的）、超大包（>2G）、Windows 之外的平台、断网中途、
//! 磁盘满、真实 CurseForge `files[]` 补全（缺 CF 客户端，见 [`fetch`] 的说明）。

pub mod baseline;
pub mod error;
pub mod fetch;
pub mod merge;
pub mod phase;
pub mod pipeline;
pub mod plan;
pub mod report;
pub mod stage;

pub use baseline::{PackBaseline, BASELINE_FILE};
pub use error::{ImportError, ImportErrorKind};
pub use fetch::{
    CurseFileMeta, DownloadEngineFetcher, FetchError, FetchErrorKind, FetchFuture, FetchRequest,
    FetchedFile, PackFetcher,
};
pub use merge::{MergeAction, MergeDecision};
pub use pipeline::baseline_relative;
pub use plan::{ImportPlan, PlanOverride, PlanRemote, Shadowed, SuffixVariant};
pub use report::{
    keys, ConflictChoice, ConflictFile, ConflictReason, FileAction, FileSource, ImportIssue,
    ImportProgress, ImportProgressSink, ImportReport, ImportStage, ImportStatus, ImportedFile,
    LoaderTarget, MissingFile, MissingReason, RegisteredInstance, RejectedComponent,
};
pub use stage::{RollbackOutcome, Staging};

use std::path::Path;

use crate::addon::pack::PackLimits;
use crate::platform::layout::LauncherLayout;

/// 冲突处置策略（默认保留用户文件）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictPolicy {
    /// 保留用户那一侧（不覆盖、不补回被删的文件），冲突逐条进报告。
    #[default]
    KeepUser,
    /// 用包内内容覆盖（用户明确要求"整合包说了算"）。
    Overwrite,
}

/// 一次导入请求。
pub struct ImportRequest<'a> {
    /// 包路径：zip（`.zip` / `.mrpack`）或 MultiMC 实例目录。
    pub pack: &'a Path,
    /// 目标实例的游戏目录（`mods/` 直接在其下）。
    pub target: &'a Path,
    /// 远端补全 + 下载；`None` = 离线导入（远端条目全部记成 `missing`）。
    pub fetcher: Option<&'a dyn PackFetcher>,
    /// 数据根布局；给了才登记实例（`profiles.json`）。
    pub register: Option<&'a LauncherLayout>,
    /// 实例展示名（缺省用包名）。
    pub instance_name: Option<&'a str>,
    /// 冲突策略。
    pub conflict_policy: ConflictPolicy,
    /// 包读取限额（条数 / 单条体积 / 解压总量）。
    pub limits: PackLimits,
    /// 进度回调。
    pub progress: Option<ImportProgressSink>,
}

impl<'a> ImportRequest<'a> {
    /// 最小请求（离线、不登记、保留用户文件、默认限额）。
    #[must_use]
    pub fn new(pack: &'a Path, target: &'a Path) -> Self {
        Self {
            pack,
            target,
            fetcher: None,
            register: None,
            instance_name: None,
            conflict_policy: ConflictPolicy::KeepUser,
            limits: PackLimits::default(),
            progress: None,
        }
    }

    /// 挂上远端补全 / 下载。
    #[must_use]
    pub fn with_fetcher(mut self, fetcher: &'a dyn PackFetcher) -> Self {
        self.fetcher = Some(fetcher);
        self
    }

    /// 挂上实例登记（`name` 为 `None` 时用包名）。
    #[must_use]
    pub fn with_registration(mut self, layout: &'a LauncherLayout, name: Option<&'a str>) -> Self {
        self.register = Some(layout);
        self.instance_name = name;
        self
    }

    /// 改冲突策略。
    #[must_use]
    pub fn with_conflict_policy(mut self, policy: ConflictPolicy) -> Self {
        self.conflict_policy = policy;
        self
    }

    /// 改读取限额。
    #[must_use]
    pub fn with_limits(mut self, limits: PackLimits) -> Self {
        self.limits = limits;
        self
    }

    /// 挂进度回调。
    #[must_use]
    pub fn with_progress(mut self, sink: ImportProgressSink) -> Self {
        self.progress = Some(sink);
        self
    }
}

/// 导入一个整合包。**永不返回 `Err`**：三态与逐项原因都在 [`ImportReport`] 里。
pub async fn import_pack(request: &ImportRequest<'_>) -> ImportReport {
    pipeline::run(request).await
}
