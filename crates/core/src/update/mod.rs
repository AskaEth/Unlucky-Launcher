//! 自更新内核：版本比较、发布清单、渠道准入、下载校验、原子替换。
//!
//! 更新源已定（D50）：**GitHub Releases 是唯一更新源**，真实实现在
//! [`GitHubReleaseSource`]（清单固定地址 + 产物走 `core::download` 引擎）。
//! 这一层仍然保持源无关：核心只认 [`UpdateSource`] 三个方法，接哪家由上层注入
//! （本地目录源 [`DirSource`] 留作对拍基准与内网手工分发）。
//!
//! 设计输入：`DevDocs/analyse/07-settings-packaging-update-p2.md` §4（HMCL 自更新）与 §6.2
//! （多平台清单扩展）。我们抄骨架、补两处 HMCL 明确放弃的东西：**回滚**与**按平台分发**。
//!
//! # 一条完整的更新链
//!
//! ```text
//! UpdateChecker::check(current, channel)   清单 + 版本 + 渠道 + 目标平台 → 结论
//!         │  Available(candidate)
//!         ├─ UpdateChecker::prepare(candidate, staging)   取包 → 长度/摘要双检
//!         │        │  StagedPackage（校验通过，失败已自动清理）
//!         │        └─ install::install_staged(...)        改名旧版 → 搬入新版（失败回滚）
//!         └─ UpToDate / MissingArtifact
//! ```
//!
//! # 分层
//!
//! ```text
//! version   语义化版本解析与排序（alpha/beta/rc、build metadata）  channel  渠道阶梯与准入
//! manifest  清单模型 + JSON 校验            target   目标平台与包形态
//! source    UpdateSource trait + 本地目录源  github   GitHub Releases 源（D50）
//! verify    暂存 + 长度/摘要校验             checker   选择哪一份发布、急不急
//! install   原子替换 + 回滚                  error     分类与文案
//! job       检查/应用的一条龙（GUI 与 CLI 共用同一入口）
//! ```
//!
//! # 刻意不做的事
//!
//! * **不碰 UI**：本模块只产出可序列化的判定结果与错误分类，弹窗/进度/文案排版在壳层。
//!   入口在 `crates/presenter`/`crates/app-iced` 接，`crates/core` 保持零 UI 依赖。
//! * **不自己替换正在运行的 exe**：那需要「退出自身 → 另一个进程搬运」（HMCL 的
//!   `--apply-to`），属启动器外壳的编排；本模块只保证搬运本身原子且可回滚。
//! * **不装安装器形态**（msi/dmg/deb/rpm）：交给系统包管理器。
//! * **不做差分/增量**：清单里没有 patch 字段，宁可每次全量，也别先上一套没验过的差分协议。

pub mod channel;
pub mod checker;
pub mod error;
pub mod github;
pub mod install;
pub mod job;
pub mod manifest;
pub mod source;
pub mod target;
pub mod verify;
pub mod version;

pub use crate::download::{HashAlgo, Integrity};
pub use crate::platform::os_info::OsFamily;

pub use self::channel::Channel;
pub use self::checker::{CheckOutcome, UpdateCandidate, UpdateChecker, Urgency};
pub use self::error::{UpdateError, UpdateErrorKind};
pub use self::github::GitHubReleaseSource;
pub use self::install::{
    backup_path, install_staged, InstallFailure, InstallReceipt, ReplacePolicy,
};
pub use self::job::{default_source, process_dest, staging_dir, Applied, Job, UPDATE_DIR_ENV};
pub use self::manifest::{Artifact, Release, ReleaseManifest};
pub use self::source::{DirSource, UpdateSource};
pub use self::target::{Arch, PackageFormat, Target};
pub use self::verify::{stage, verify_file, StagedPackage};
pub use self::version::{Stage, Version, VersionError};

#[cfg(test)]
mod tests;
