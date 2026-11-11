//! 整合包失败分类：**可诊断**比"报了个错"重要。
//!
//! 每一类都对应导入链路上一个**不同的处置动作**（M111 据此分级）：
//!
//! | 大类 | 稳定码 | 谁该负责 |
//! |------|--------|----------|
//! | [`PackErrorKind::Unsupported`] | `pack_unsupported` | 换格式 / 让用户手工处理（没格式认领、或模型表达不了该格式） |
//! | [`PackErrorKind::Metadata`] | `pack_metadata` | 包本身坏了（JSON 形状不符、哈希缺失），重试无意义 |
//! | [`PackErrorKind::Traversal`] | `pack_traversal` | **拒绝**，且不写任何文件（恶意包） |
//! | [`PackErrorKind::Symlink`] | `pack_symlink` | **拒绝**（符号链接条目在 Windows 上无合法用途） |
//! | [`PackErrorKind::Limits`] | `pack_limits` | **拒绝**（条数/体积超限，zip 炸弹防线） |
//! | [`PackErrorKind::Io`] | `pack_io` | 环境问题，可重试 |
//!
//! 文案与日志分离：`message()` 给人看，`kind().as_str()` 进事件流与 `--json`。

use std::fmt;

/// 整合包失败的大类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackErrorKind {
    /// 没有格式认领这份包，或模型里的信息不足以写成该格式。
    Unsupported,
    /// 清单 JSON 坏了 / 形状不符 / 必需字段缺失。
    Metadata,
    /// 包内路径穿越（绝对路径、`..` 上跳、盘符、ADS、反斜杠分隔）。
    Traversal,
    /// zip 里有符号链接条目。
    Symlink,
    /// 条目数 / 解压体积超过限额。
    Limits,
    /// 本地 IO。
    Io,
}

impl PackErrorKind {
    /// 稳定字符串 id（进 `--json` 的 `error.code` 与日志，**不要改名**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "pack_unsupported",
            Self::Metadata => "pack_metadata",
            Self::Traversal => "pack_traversal",
            Self::Symlink => "pack_symlink",
            Self::Limits => "pack_limits",
            Self::Io => "pack_io",
        }
    }
}

/// 整合包错误：大类 + 人读说明 + 可选下一步提示。
#[derive(Debug, Clone)]
pub struct PackError {
    kind: PackErrorKind,
    message: String,
    hint: Option<String>,
}

impl PackError {
    /// 构造。
    #[must_use]
    pub fn new(kind: PackErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            hint: None,
        }
    }

    /// 没格式认领 / 模型表达不了该格式。
    #[must_use]
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(PackErrorKind::Unsupported, message)
    }

    /// 清单形状不对 / 必填字段缺失。
    #[must_use]
    pub fn metadata(message: impl Into<String>) -> Self {
        Self::new(PackErrorKind::Metadata, message)
    }

    /// 路径穿越。
    #[must_use]
    pub fn traversal(message: impl Into<String>) -> Self {
        Self::new(PackErrorKind::Traversal, message)
    }

    /// 符号链接条目。
    #[must_use]
    pub fn symlink(message: impl Into<String>) -> Self {
        Self::new(PackErrorKind::Symlink, message)
    }

    /// 超过限额。
    #[must_use]
    pub fn limits(message: impl Into<String>) -> Self {
        Self::new(PackErrorKind::Limits, message)
    }

    /// 本地 IO。
    #[must_use]
    pub fn io(message: impl Into<String>) -> Self {
        Self::new(PackErrorKind::Io, message)
    }

    /// 补一句提示。
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// 大类。
    #[must_use]
    pub const fn kind(&self) -> PackErrorKind {
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

impl fmt::Display for PackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PackError {}

impl From<std::io::Error> for PackError {
    fn from(error: std::io::Error) -> Self {
        Self::io(error.to_string())
    }
}

impl From<zip::result::ZipError> for PackError {
    fn from(error: zip::result::ZipError) -> Self {
        match error {
            zip::result::ZipError::FileNotFound => Self::io("包内条目不存在"),
            other => Self::metadata(format!("zip 读取失败：{other}")),
        }
    }
}

impl From<serde_json::Error> for PackError {
    fn from(error: serde_json::Error) -> Self {
        Self::metadata(format!("JSON 解析失败：{error}"))
    }
}

/// `pack` 模块内的结果别名。
pub type PackResult<T> = Result<T, PackError>;
