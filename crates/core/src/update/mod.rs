//! 自更新内核：版本比较、发布清单、渠道准入、下载校验、原子替换。
//!
//! 更新源已定（D50）：**GitHub Releases 是唯一更新源**，真实实现在
//! [`GitHubReleaseSource`]（清单固定地址 + 产物走 `core::download` 引擎）。
//! 这一层仍然保持源无关：核心只认 [`UpdateSource`] 三个方法，接哪家由上层注入
//! （本地目录源 [`DirSource`] 留作对拍基准、内网手工分发与**测试通道**）。
//!
//! 测试通道不是第二个更新源：`update-test-source` feature（默认关）+ `ULL_UPDATE_DIR` +
//! `ULL_UPDATE_TEST_SOURCE=1` 三档全给才会把 [`default_source`] 换成本地目录（判据、隔离手段与
//! 为什么用编译期 feature 见 `job::test_source_root` 的文档）。
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
//! signature 清单签名信封 + 验签             trust     钉进二进制的公钥表（keyring）
//! ed25519   Ed25519 严格验签（ed25519-dalek::verify_strict 的薄封装）
//! install   原子替换 + 回滚                  replace_io 改名/复制/重试与跨卷判据（注入点）
//! error     分类与文案                      job       检查/应用的一条龙（GUI 与 CLI 共用同一入口）
//! ```
//!
//! # 跨卷（M171）
//!
//! 暂存目录在数据根下、目标 exe 在安装目录，两者可能不在同一卷（便携版放 U 盘、数据根在 D 盘而
//! 程序在 C 盘）。`rename` 不能跨卷，所以 [`install_staged`] 先把新包落成**目标同目录**的
//! `.ullnew-*` 中转名（同卷改名；跨卷才复制到这儿并核对字节数），再由一次同卷改名顶到目标位置。
//! 目标路径上任何时刻都不是半截文件，同一卷路径也**不会多搬一个字节**。
//!
//! # 刻意不做的事
//!
//! * **不碰 UI**：本模块只产出可序列化的判定结果与错误分类，弹窗/进度/文案排版在壳层。
//!   入口在 `crates/presenter`/`crates/app-iced` 接，`crates/core` 保持零 UI 依赖。
//! * **不自己替换正在运行的 exe**：那需要「退出自身 → 另一个进程搬运」（HMCL 的
//!   `--apply-to`），属启动器外壳的编排；本模块只保证搬运本身原子且可回滚。
//! * **不装安装器形态**（msi/dmg/deb/rpm）：交给系统包管理器。
//! * **不做差分/增量**：清单里没有 patch 字段，宁可每次全量，也别先上一套没验过的差分协议。
//!
//! # 信任根（M190）
//!
//! ```text
//! 源只取字节 ──> signature::verify_manifest（清单**原始字节** + `manifest.json.sig`）
//!                │  判据：钉死的公钥（trust::keyring，编译期常量）验 Ed25519
//!                ├─ 过了 ──> manifest::from_json ──> checker/apply
//!                └─ 不过 ──> 五类签名错误（不可重试），**什么都不会被安装**
//! ```
//!
//! 一句话：**先验签、再解析**。清单的 sha256 只保证「包体和清单对上」，清单本身可不可信由签名回答——
//! 两条判据互不替代（`tests/signature_cases.rs` 分别证明）。

pub mod channel;
pub mod checker;
mod ed25519;
pub mod error;
pub mod github;
pub mod install;
pub mod job;
pub mod manifest;
mod replace_io;
mod signature;
pub mod source;
pub mod target;
mod trust;
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
pub use self::job::{
    default_source, process_dest, staging_dir, Applied, Job, UPDATE_DIR_ENV, UPDATE_TEST_SOURCE_ENV,
};
pub use self::manifest::{Artifact, Release, ReleaseManifest};
pub use self::signature::VerifiedSignature;
pub use self::source::{verified_manifest, DirSource, ManifestDocuments, UpdateSource};
pub use self::target::{Arch, PackageFormat, Target};
pub use self::verify::{stage, verify_file, StagedPackage};
pub use self::version::{Stage, Version, VersionError};

#[cfg(test)]
mod tests;
