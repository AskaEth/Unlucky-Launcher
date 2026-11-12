//! Mod 与整合包：加载器安装（Fabric/Quilt/NeoForge/Forge）、整合包双格式导入导出、
//! 本地 Mod 管理、CurseForge / Modrinth 客户端。
//!
//! 设计定稿：`DevDocs/PlanDoc/08-mod-and-recommend.md`（决策 D10）。
//! 安全要点：整合包路径穿越双检；AI 推荐走「结构化过滤在前 + LLM 排序在后 + 来源链接防幻觉」。
//!
//! # 当前公共面
//!
//! - [`loader`]（P2 · M32）：加载器安装——官方元数据 → 带 `inheritsFrom` 的 loader 版本 JSON
//!   → 补库清单，落盘与实例登记复用 `core::game::install`（M27）。
//! - [`pack`]（P2 · M110）：整合包的**格式无关模型**与**格式扩展点**——CF / Modrinth / MultiMC
//!   三种格式的探测与读取、CF / mrpack 的渲染与导出、zip 安全读取（穿越双检 / 符号链接 /
//!   条数体积上限）。
//! - [`import`]（P2 · M111）：**导入管线**——三种格式 → 实例目录：跨段落地、overrides 三方
//!   合并（保用户改动）、两阶段补全与失败分级、临时目录 + rename/journal 回滚、结构化报告。
//!   导出（M112）另起模块。
//! - [`export`]（P2 · M112）：**导出管线**——实例目录 → `.mrpack`（优先）/ CF `manifest.json`。
//!   逐文件判定「能否被远程替代」（Modrinth SHA-1 + CF 指纹），判定不了的**内联进 `overrides/`**
//!   并逐条进报告；`.disabled` 用路径后缀；`game`/`dependencies` 从实例快照取。
//!   MultiMC/Prism **只导入不导出**（有意省略）。
//!
//! - [`online`]（P2 · M162）：**在线内容源**——先只做匿名可用的 Modrinth（`/v2/search` /
//!   `/v2/project` / `/v2/project/{id}/version` / `/v2/version_file/{hash}`）。网络出口只有
//!   `Transport` 一个 trait（生产实现走 `core::download` 的引擎）；下载走引擎并**复核
//!   sha1 + sha512**；远端条目补全按 sha512 反查 sha1，补不出照旧如实计数。
//!   CurseForge 需要 `x-api-key`，本轮不碰（见模块文档的「留的缝」）。
//!
//! - [`mods`]（P2 · M224）：**本地 Mod 列表与启停**——`.disabled` / `.old` 后缀的唯一判据
//!   （导出扫描与导入计划两侧委派到这里）、`<实例运行目录>/mods/` 的单根列表、启停 = 在
//!   `mods/` 里改名（五条守卫、幂等、不覆盖）。
//!
//! 规划中：CurseForge 客户端（§5）。

pub mod export;
pub mod import;
pub mod loader;
pub mod mods;
pub mod online;
pub mod pack;

pub use export::{
    export_instance, ExportFile, ExportReport, ExportRequest, FileClass, FileReason, RemoteCatalog,
    RemoteHit,
};
pub use import::{
    import_pack, scan_pack, scan_pack_online, ConflictChoice, ConflictDecisions, ConflictFile,
    ConflictPolicy, ImportConflict, ImportError, ImportErrorKind, ImportProgress,
    ImportProgressSink, ImportReport, ImportRequest, ImportScan, ImportStage, ImportStatus,
    ImportedFile, MissingFile, PackBaseline, PackFetcher, RejectedComponent,
};
pub use loader::{
    LoaderError, LoaderErrorKind, LoaderInstallPlan, LoaderKind, LoaderPlanInput, LoaderRelease,
    LoaderSource, LoaderSupport,
};
pub use mods::{
    ModEntry, ModError, ModScan, ModSkip, ModToggle, SkipReason, SuffixVariant as ModSuffixVariant,
    MODS_DIR,
};
pub use online::{
    download_verified, ArtifactFetcher, ContentType, Downloaded, EngineArtifactFetcher,
    EngineTransport, Modrinth, OnlineError, OnlineErrorKind, ProjectDetail, ProjectHit, SearchPage,
    SearchQuery, SortIndex, Transport, Version, VersionFile, VersionQuery, MODRINTH_API,
};
pub use pack::{
    EnvSupport, PackArchive, PackError, PackErrorKind, PackFile, PackFormat, PackLimits,
    PackLoader, PackManifest, PackOverrides, PackRegistry, PackResult, PackSide,
};

#[cfg(test)]
pub(crate) mod temp;
#[cfg(test)]
mod tests;
