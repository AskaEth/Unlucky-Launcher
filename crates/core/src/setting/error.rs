//! setting 层错误类型与结果别名。

use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::platform::error::PlatformError;

use super::schema::{SchemaCompat, SchemaVersion};

/// setting 层的结果别名。
pub type SettingResult<T> = Result<T, SettingError>;

/// 配置读写与迁移可能出的问题。
#[derive(Debug)]
pub enum SettingError {
    /// 文件系统操作失败。
    Io {
        /// 出错路径。
        path: PathBuf,
        /// 底层错误。
        source: io::Error,
    },
    /// JSON 解析/序列化失败。
    Json {
        /// 出错路径。
        path: PathBuf,
        /// 底层错误。
        source: serde_json::Error,
    },
    /// 文件是只读档（`READ_ONLY`），拒绝保存。
    ReadOnly {
        /// 文件路径。
        path: PathBuf,
    },
    /// 文件不可读档（`UNREADABLE`），全程不写。
    Unreadable {
        /// 文件路径。
        path: PathBuf,
        /// 判定依据。
        compat: SchemaCompat,
    },
    /// schema 里没有这个键（AI 白名单外的键也走这里）。
    UnknownKey {
        /// 键。
        key: String,
    },
    /// 值不合法（类型/范围/枚举）。
    InvalidValue {
        /// 键。
        key: String,
        /// 原因。
        reason: String,
    },
    /// 键已废弃，写必须走替代键。
    DeprecatedKey {
        /// 用户/调用方给的旧键。
        key: String,
        /// 替代键。
        replaced_by: &'static str,
    },
    /// 迁移链断链/缺环/顺序错。
    ChainBroken {
        /// 原因。
        reason: String,
    },
    /// 单步迁移失败。
    Migration {
        /// 起始版本。
        from: SchemaVersion,
        /// 目标版本。
        to: SchemaVersion,
        /// 原因。
        reason: String,
    },
    /// platform 层错误透传。
    Platform(PlatformError),
}

impl fmt::Display for SettingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "IO 失败 @ {}: {source}", path.display()),
            Self::Json { path, source } => write!(f, "JSON 失败 @ {}: {source}", path.display()),
            Self::ReadOnly { path } => write!(f, "文件只读，拒绝保存: {}", path.display()),
            Self::Unreadable { path, compat } => write!(
                f,
                "文件不可读（{}），全程不写: {}",
                compat.as_str(),
                path.display()
            ),
            Self::UnknownKey { key } => write!(f, "schema 未定义该键: {key}"),
            Self::InvalidValue { key, reason } => write!(f, "值不合法 [{key}]: {reason}"),
            Self::DeprecatedKey { key, replaced_by } => {
                write!(f, "键 {key} 已废弃，请改用 {replaced_by}")
            }
            Self::ChainBroken { reason } => write!(f, "迁移链不完整: {reason}"),
            Self::Migration { from, to, reason } => {
                write!(f, "迁移 {from} → {to} 失败: {reason}")
            }
            Self::Platform(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for SettingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            Self::Platform(err) => Some(err),
            _ => None,
        }
    }
}

impl From<PlatformError> for SettingError {
    fn from(err: PlatformError) -> Self {
        Self::Platform(err)
    }
}
