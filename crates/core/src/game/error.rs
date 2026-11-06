//! `core::game` 的错误类型。
//!
//! 采用手写 `Display`/`Error` 而不用 `thiserror`：core 的依赖面越窄越好（D3 模块纪律），
//! 且这里的变体数量固定、不需要派生宏的收益。

use std::fmt;
use std::path::PathBuf;

/// `core::game` 的失败集合。
#[derive(Debug)]
pub enum GameError {
    /// JSON 解析/序列化失败（版本清单、版本 JSON、profiles）。
    Json {
        /// 出错的文档种类，用于给用户看上下文（如 `version_manifest_v2`）。
        context: String,
        /// 底层 serde_json 错误。
        source: serde_json::Error,
    },
    /// 清单或版本 JSON 缺少必需的 `id`。
    MissingVersionId,
    /// `inheritsFrom` 指向的父版本在本仓库中不存在。
    MissingParent {
        /// 缺失的父版本 id。
        parent: String,
        /// 发起继承的子版本 id。
        required_by: String,
    },
    /// `inheritsFrom` 成环。
    CircularInheritance {
        /// 从根到环的 id 链（首尾为同一个 id）。
        chain: Vec<String>,
    },
    /// Maven 坐标（`group:artifact:version[:classifier][@ext]`）不合法。
    InvalidCoordinate {
        /// 原始字符串。
        raw: String,
        /// 不合法的原因。
        reason: &'static str,
    },
    /// 资源对象哈希不合法（长度不足以做两级分片）。
    InvalidAssetHash {
        /// 原始哈希。
        hash: String,
    },
    /// 文件系统错误。
    Io {
        /// 出错的路径。
        path: PathBuf,
        /// 底层 IO 错误。
        source: std::io::Error,
    },
    /// 已有草稿持有写权，同一仓库同时只允许一个写会话。
    DraftBusy,
    /// 实例隔离档位是 `custom`，但没有可用的自定义目录（不猜默认值）。
    CustomIsolationWithoutDir {
        /// 实例 id（可能为空：调用方没给实例上下文）。
        instance: String,
    },
}

impl fmt::Display for GameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json { context, source } => write!(f, "解析 {context} 失败: {source}"),
            Self::MissingVersionId => write!(f, "版本 JSON 缺少 id"),
            Self::MissingParent {
                parent,
                required_by,
            } => write!(
                f,
                "版本 {required_by} 继承的父版本 {parent} 不存在（缺失前置版本）"
            ),
            Self::CircularInheritance { chain } => {
                write!(f, "版本继承成环: {}", chain.join(" -> "))
            }
            Self::InvalidCoordinate { raw, reason } => {
                write!(f, "Maven 坐标不合法（{reason}）: {raw}")
            }
            Self::InvalidAssetHash { hash } => {
                write!(f, "资源哈希不合法（短于两个字符）: {hash}")
            }
            Self::Io { path, source } => write!(f, "IO 失败 {}: {source}", path.display()),
            Self::DraftBusy => write!(f, "已有草稿持有写权（单写者约束）"),
            Self::CustomIsolationWithoutDir { instance } => {
                if instance.is_empty() {
                    write!(f, "隔离档位为 custom，但没有指定自定义运行目录")
                } else {
                    write!(
                        f,
                        "实例 {instance} 的隔离档位为 custom，但没有指定自定义运行目录"
                    )
                }
            }
        }
    }
}

impl std::error::Error for GameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl GameError {
    /// 包装 serde_json 错误并带上文档上下文。
    pub fn json(context: impl Into<String>, source: serde_json::Error) -> Self {
        Self::Json {
            context: context.into(),
            source,
        }
    }

    /// 包装 IO 错误并带上路径。
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
