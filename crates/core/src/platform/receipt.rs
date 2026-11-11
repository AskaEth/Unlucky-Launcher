//! 迁移收据：指纹 + 幂等，防重复迁移 / 半迁移（HMCL `MigrationReceipt` 思想）。
//!
//! 出处：`DevDocs/analyse/07-settings-packaging-update.md` §1.8——
//! 收据里存源文件 `sourceSHA256`，这样「用户删掉新文件、但旧文件没变」时不会重复迁移，旧文件变了才重放。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::atomic;
use super::error::PlatformError;
use super::error::PlatformResult;
use super::hash;

/// 本实现能读懂的收据版本。
pub const RECEIPT_SCHEMA_VERSION: u32 = 1;

/// 收据类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptKind {
    /// 数据目录迁移。
    DataDirMove,
    /// 遗留配置格式迁移。
    LegacyConfig,
}

impl ReceiptKind {
    /// 稳定名。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::DataDirMove => "data_dir_move",
            Self::LegacyConfig => "legacy_config",
        }
    }
}

/// 一次已完成的迁移记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReceipt {
    /// 格式版本。
    pub schema_version: u32,
    /// 收据类型。
    pub kind: ReceiptKind,
    /// 源指纹（普通配置迁移 = 源文件内容 SHA-256；目录迁移 = 源/目标路径签名）。
    pub source_fingerprint: String,
    /// 源路径。
    pub source: PathBuf,
    /// 目标路径。
    pub target: PathBuf,
    /// 完成时间（毫秒 since UNIX_EPOCH）。
    pub applied_at_ms: u64,
}

impl MigrationReceipt {
    /// 构造数据目录迁移收据。
    #[must_use]
    pub fn data_dir_move(source: PathBuf, target: PathBuf, applied_at_ms: u64) -> Self {
        let fingerprint = fingerprint_data_dir_move(&source, &target);
        Self {
            schema_version: RECEIPT_SCHEMA_VERSION,
            kind: ReceiptKind::DataDirMove,
            source_fingerprint: fingerprint,
            source,
            target,
            applied_at_ms,
        }
    }

    /// 构造遗留配置迁移收据。
    #[must_use]
    pub fn legacy_config(
        source: PathBuf,
        target: PathBuf,
        source_fingerprint: impl Into<String>,
        applied_at_ms: u64,
    ) -> Self {
        Self {
            schema_version: RECEIPT_SCHEMA_VERSION,
            kind: ReceiptKind::LegacyConfig,
            source_fingerprint: source_fingerprint.into(),
            source,
            target,
            applied_at_ms,
        }
    }
}

/// 是否该重放这次迁移。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayDecision {
    /// 收据在且源指纹相同 → 已经迁过，跳过（防重放）。
    SkipSameSource,
    /// 收据在但源指纹变了 → 允许重放。
    ReplayChangedSource,
    /// 没有收据 → 正常迁移。
    ReplayNoReceipt,
}

impl ReplayDecision {
    /// 是否需要执行迁移。
    #[must_use]
    pub const fn should_replay(&self) -> bool {
        !matches!(self, Self::SkipSameSource)
    }

    /// 稳定名（日志 / 审计）。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::SkipSameSource => "skip_same_source",
            Self::ReplayChangedSource => "replay_changed_source",
            Self::ReplayNoReceipt => "replay_no_receipt",
        }
    }
}

/// 根据已有收据与当前源指纹决定是否重放。
#[must_use]
pub fn decide(existing: Option<&MigrationReceipt>, fingerprint: &str) -> ReplayDecision {
    match existing {
        None => ReplayDecision::ReplayNoReceipt,
        Some(receipt) if receipt.source_fingerprint == fingerprint => {
            ReplayDecision::SkipSameSource
        }
        Some(_) => ReplayDecision::ReplayChangedSource,
    }
}

/// 读收据；不存在返回 `None`。
///
/// # Errors
/// 读失败或 JSON 损坏、版本不认识时返回 [`PlatformError::ReceiptCorrupt`] / [`PlatformError::Io`]。
pub fn read(path: &Path) -> PlatformResult<Option<MigrationReceipt>> {
    let Some(bytes) = atomic::read_optional(path)? else {
        return Ok(None);
    };
    let receipt: MigrationReceipt =
        serde_json::from_slice(&bytes).map_err(|err| PlatformError::ReceiptCorrupt {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })?;
    if receipt.schema_version > RECEIPT_SCHEMA_VERSION {
        return Err(PlatformError::ReceiptCorrupt {
            path: path.to_path_buf(),
            reason: format!(
                "收据版本 {} 高于本实现支持的 {RECEIPT_SCHEMA_VERSION}",
                receipt.schema_version
            ),
        });
    }
    Ok(Some(receipt))
}

/// 原子写收据。
///
/// # Errors
/// 序列化或写盘失败时返回 [`PlatformError`]。
pub fn write(path: &Path, receipt: &MigrationReceipt) -> PlatformResult<()> {
    let bytes =
        serde_json::to_vec_pretty(receipt).map_err(|err| PlatformError::ReceiptCorrupt {
            path: path.to_path_buf(),
            reason: format!("无法序列化收据: {err}"),
        })?;
    atomic::write_atomic(path, &bytes)
}

/// 内容指纹（SHA-256 十六进制）。
#[must_use]
pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    hash::sha256_hex(bytes)
}

/// 文件内容指纹；文件不存在返回 `None`。
///
/// # Errors
/// 读失败时返回 [`PlatformError::Io`]。
pub fn fingerprint_file(path: &Path) -> PlatformResult<Option<String>> {
    match hash::sha256_hex_file(path) {
        Ok(hex) => Ok(Some(hex)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(PlatformError::Io {
            path: path.to_path_buf(),
            source: err,
        }),
    }
}

/// 目录迁移的源指纹：源/目标路径签名。
///
/// 目录内容的哈希代价太高且天然易变（缓存/日志一直在动），因此目录迁移的抗重放主力是
/// [`super::redirect`] 指针 + 「目标非空则拒绝」，收据负责留痕与「同源同目标不重做」。
#[must_use]
pub fn fingerprint_data_dir_move(source: &Path, target: &Path) -> String {
    let signature = format!(
        "{}|{}",
        super::paths::to_portable_string(source),
        super::paths::to_portable_string(target)
    );
    fingerprint_bytes(signature.as_bytes())
}
