//! 版本与实例仓库：不可变快照 + 单写者草稿、版本 JSON 继承解析、实例目录布局。
//!
//! 设计定稿：`DevDocs/PlanDoc/04-data-and-config.md` §4（实例模型）、
//! `DevDocs/PlanDoc/01-tech-stack-and-architecture.md` §3（Snapshot/Draft 纪律），
//! 源证据 `DevDocs/analyse/01-architecture-overview.md` §3.3。
//!
//! 与其它模块的契约：快照（Snapshot）类型与继承解析结果要与 launch 对齐
//! （`DevDocs/SubAgentsTalk/README.md` 的对齐点；详细约定见 `SubAgentsTalk/game-接口约定.md`）。
//!
//! 公共面（M14）：
//! - 版本清单：[`VersionList`] / [`RemoteVersion`] / [`ReleaseType`] / [`VersionCategory`]，
//!   外加内嵌补充清单 [`unlisted`]；
//! - 版本 JSON 与继承：[`VersionJson`]、[`resolve`]、[`merge`]、[`apply_patches`]；
//! - 参数与库：[`Arguments`] / [`Environment`] / [`Rule`]、[`Library`] / [`MavenCoordinate`]；
//! - 版本 id → 加载器：[`loader`]（[`InstanceLoader::parse`]，全仓唯一解析入口）；
//! - 能力位：[`capability`]（版本 JSON 库表 → `has_vulkan_backend`）；
//! - 实例：目录布局 [`GameLayout`]、档案 [`ProfileList`] / [`InstanceProfile`] / [`ProfileId`]；
//! - 发布：快照仓库 [`Store`] / [`Snapshot`] / [`Draft`]。
//!
//! 安装层（M27，[`install`]）：版本链 → 下载计划 → 分区落盘 → 实例登记。
//! 它把上面这些"模型"串成可执行的流程，但**自己不碰 UI、不拼镜像 URL**：
//! 下载全部交给 `core::download` 的引擎（镜像/健康度/续传/校验都在那边）。
//!
//! Java 管理（M33，[`java`]）：托管 JDK 的查询/下载/解压/探测（`.ULL/data/java-managed/`）
//! 与 MC↔Java 映射表。同样只借 `core::download` 的引擎做下载，自己不拼镜像 URL。
//!
//! 模块分工按"数据形状 / 纯计算 / 状态"切开，任何模块都不碰网络与 UI：
//! 下载归 `core::download`、进程归 `core::launch`、配置落盘归 `core::setting`。

pub mod arguments;
pub mod capability;
pub mod error;
pub mod inherit;
pub mod install;
pub mod isolation;
pub mod java;
pub mod layout;
pub mod library;
pub mod loader;
pub mod manifest;
pub mod profiles;
pub mod snapshot;
pub mod unlisted;
pub mod version;

pub use arguments::{rules_allow, Arguments, Environment, ExpandedArguments, Rule, RuleAction};
pub use capability::{
    is_vulkan_library, library_names_have_vulkan, VersionCapabilities, VULKAN_LIBRARY_ARTIFACT,
    VULKAN_LIBRARY_GROUP,
};
pub use error::GameError;
pub use inherit::{apply_patches, merge, resolve, sorted_patches, VersionSource};
pub use isolation::{
    derive_game_dir, observe_mode, probe_both_sides, probe_side, Derivation, IsolationMode,
    IsolationSource, ModeChange, ModeChangeProbe, ModeChangeWarning, SideContent,
    PROBED_CONTENT_DIRS, SHARED_LAYERS,
};
pub use layout::GameLayout;
pub use library::{Library, MavenCoordinate};
pub use loader::{InstanceLoader, LoaderKind};
pub use manifest::{AssetIndexRef, DownloadInfo, Downloads, JavaVersion, Logging, VersionJson};
pub use profiles::{InstanceProfile, ProfileId, ProfileList, PROFILE_SCHEMA_VERSION};
pub use snapshot::{Draft, Snapshot, Store};
pub use version::{
    LatestVersions, ReleaseType, RemoteVersion, VersionCategory, VersionList,
    VERSION_MANIFEST_V2_URL,
};
