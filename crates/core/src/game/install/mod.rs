//! 安装层：版本链 → 下载计划 → 分区落盘 → 实例登记（P2 核心闭环「装一个版本并启动它」）。
//!
//! 设计输入：`DevDocs/analyse/03-version-download.md` §6（逐类资源拆解）、
//! `DevDocs/PlanDoc/04-data-and-config.md` §4（D20 实例模型）、§2（`.ULL/` 四分区）、
//! `DevDocs/PlanDoc/05-download-and-mirrors.md` §2（分层下载策略）、
//! `DevDocs/PlanDoc/research/04-launcher-configs.md` §5（实例键草案）。
//!
//! # 分工（每层只做一件事，谁都不越界）
//!
//! ```text
//! source    网络：版本清单（带 TTL 缓存）→ 版本 JSON 链 → 资源索引      （异步）
//! plan      纯计算：清单 + 索引 → InstallPlan（文件清单 / 总大小 / 角色）  （同步、可 dry-run）
//! execute   落盘：逐文件跳过判定 + 下载引擎 + 原子写版本 JSON            （异步）
//! instance  登记：profiles.json + config/instances/<id>.json（稀疏、幂等）（同步）
//! ```
//!
//! # 三条硬约定
//!
//! 1. **恒共享层**（D20）：`versions/` `assets/` `libraries/` 一律进游戏根，任何模式都不按实例复制；
//! 2. **幂等**：重复装同一版本不重复下载（按 sha1 跳过）、不重复建实例（按 `versionId` 复用档案）；
//! 3. **如实告知**：清单缺件（旧版无 `downloads.client` / 无 `assetIndex`）只降级成计划告警，
//!    不静默假装成功，也不编造 URL。
//!
//! # 同步宿主怎么用
//!
//! `crates/cli` 不依赖 tokio：整段「规划 + 下载」包在一次
//! [`crate::download::blocking::block_on`] 里，见 `ull install` 的实现。

pub mod assets;
pub mod execute;
pub mod instance;
pub mod libraries;
pub mod pack_dir;
pub mod plan;
pub mod plan_view;
mod report;
pub mod source;

use std::fmt;

pub use assets::{AssetIndex, AssetObject, ASSET_OBJECT_BASE};
pub use execute::{execute, execute_with_cancel};
pub use instance::{register, InstanceConfig, Registration};
pub use plan::{FileRole, InstallPlan, PlanFile, PlanInput};
pub use report::{
    FileFailure, FileOutcome, FileStatus, InstallReport, ManifestWrite, ProgressSink,
};
pub use source::{ManifestFreshness, RemoteSource, MANIFEST_TTL_MS};

use crate::download::{DownloadError, DownloadErrorKind};
use crate::game::error::GameError;

/// 安装失败的大类（CLI 据此给稳定错误码，脚本可分流）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallErrorKind {
    /// 版本 id 在官方清单与本地仓库里都不存在。
    VersionNotFound,
    /// 网络侧失败（所有候选源都失败 / 超时 / 连接错误）。
    Network,
    /// 内容校验不符（镜像给了错内容）。
    Integrity,
    /// 本地 IO（写盘失败、目录权限）。
    Io,
    /// 元数据解析失败（清单/版本 JSON/资源索引坏了）。
    Metadata,
    /// 用户取消（`execute_with_cancel` 看到取消标志；不是失败，也不是网络问题）。
    ///
    /// CLI 永远走 [`execute`]（不传取消标志），所以这个大类只可能从 GUI 的取消路径出来。
    Cancelled,
}

impl InstallErrorKind {
    /// 稳定字符串 id（进 `--json` 的 `error.code`）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VersionNotFound => "install_version_not_found",
            Self::Network => "install_network",
            Self::Integrity => "install_integrity",
            Self::Io => "install_io",
            Self::Metadata => "install_metadata",
            Self::Cancelled => "install_cancelled",
        }
    }
}

/// 安装期错误：大类 + 人读说明 + 可选提示。
#[derive(Debug, Clone)]
pub struct InstallError {
    kind: InstallErrorKind,
    message: String,
    hint: Option<String>,
}

impl InstallError {
    /// 构造。
    #[must_use]
    pub fn new(kind: InstallErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            hint: None,
        }
    }

    /// 版本不存在。
    #[must_use]
    pub fn version_not_found(id: &str) -> Self {
        Self::new(
            InstallErrorKind::VersionNotFound,
            format!("版本 {id} 不在版本清单里，也不在本地 versions/ 下"),
        )
        .with_hint("用 `ull --json install <version>` 前先确认版本 id（如 26.3 / 1.21.4）；快照版本 id 形如 24w14a")
    }

    /// 补一句提示。
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// 大类。
    #[must_use]
    pub const fn kind(&self) -> InstallErrorKind {
        self.kind
    }

    /// 人读说明。
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 下一步提示。
    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }
}

impl fmt::Display for InstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for InstallError {}

impl From<DownloadError> for InstallError {
    fn from(error: DownloadError) -> Self {
        let kind = match error.kind {
            DownloadErrorKind::Integrity => InstallErrorKind::Integrity,
            DownloadErrorKind::Io => InstallErrorKind::Io,
            DownloadErrorKind::InvalidRequest => InstallErrorKind::Metadata,
            DownloadErrorKind::NotFound
            | DownloadErrorKind::ServerError
            | DownloadErrorKind::Timeout
            | DownloadErrorKind::Network
            | DownloadErrorKind::RangeUnsupported
            | DownloadErrorKind::AllSourcesFailed => InstallErrorKind::Network,
        };
        let mut message = error.message.clone();
        if let Some(source) = &error.source {
            message.push_str(&format!("（源 {source}）"));
        }
        Self::new(kind, message)
    }
}

impl From<GameError> for InstallError {
    fn from(error: GameError) -> Self {
        let kind = match &error {
            GameError::Io { .. } => InstallErrorKind::Io,
            GameError::Json { .. } | GameError::InvalidAssetHash { .. } => {
                InstallErrorKind::Metadata
            }
            GameError::MissingVersionId
            | GameError::MissingParent { .. }
            | GameError::CircularInheritance { .. }
            | GameError::InvalidCoordinate { .. }
            | GameError::CustomIsolationWithoutDir { .. }
            | GameError::DraftBusy => InstallErrorKind::Metadata,
        };
        Self::new(kind, error.to_string())
    }
}

impl From<crate::platform::error::PlatformError> for InstallError {
    fn from(error: crate::platform::error::PlatformError) -> Self {
        Self::new(InstallErrorKind::Io, error.to_string())
    }
}

#[cfg(test)]
mod tests;
