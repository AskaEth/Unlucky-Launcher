//! 自更新错误：一份分类 + 一句人话 + 出错路径。
//!
//! 分类只服务三件事：给日志/事件一个稳定 id、判「要不要换源」、判「用户的包有没有被动过」。
//! core 不引 `thiserror`，手写 `Display`/`Error`（与 `download::error` 同一取舍）。

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use super::version::VersionError;

/// 自更新失败分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpdateErrorKind {
    /// 版本号解析失败（清单里写了不是 semver 的东西）。
    InvalidVersion,
    /// 清单结构不合法：缺字段、摘要格式错、同一清单里重复版本、`size` 为 0。
    InvalidManifest,
    /// 清单声明的渠道与请求的渠道不符（拿 stable 清单当 beta 用）。
    ChannelMismatch,
    /// 更新源没有这份数据：渠道清单文件、产物文件不存在。
    SourceMissing,
    /// 更新源协议不支持（例如本地目录源里出现了 `http://` 地址）。
    UnsupportedSource,
    /// 远端源取字节失败：连不上、超时、5xx、所有候选源（含镜像）全挂。
    ///
    /// 与 [`Self::Io`] 分开是因为处置不同：网络失败值得隔一会儿重试/换镜像，
    /// 本地 IO 失败（盘满、没权限）重试多少次都一样。
    Network,
    /// 本地 IO 失败：读源、写暂存、替换落盘。
    Io,
    /// 摘要不符：拿到的字节与清单声明的不是同一份。
    DigestMismatch,
    /// 长度不符：典型成因是下载被截断，也可能清单写错。
    SizeMismatch,
}

impl UpdateErrorKind {
    /// 稳定字符串 id（日志、事件流、诊断规则输入用；**不要改名**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidVersion => "invalid_version",
            Self::InvalidManifest => "invalid_manifest",
            Self::ChannelMismatch => "channel_mismatch",
            Self::SourceMissing => "source_missing",
            Self::UnsupportedSource => "unsupported_source",
            Self::Network => "network",
            Self::Io => "io",
            Self::DigestMismatch => "digest_mismatch",
            Self::SizeMismatch => "size_mismatch",
        }
    }

    /// 换个源 / 重下一次是否可能变好（调用方据此决定重试，别在这里写死重试次数）。
    ///
    /// 清单本身不合法、协议不支持、渠道不符这几类换多少源都一样，所以只有字节层
    /// （摘要/长度）、网络（可能是瞬时抖动）与 IO（可能是瞬时占用）算可重试。
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::DigestMismatch | Self::SizeMismatch | Self::Network | Self::Io
        )
    }
}

/// 自更新错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateError {
    /// 分类。
    pub kind: UpdateErrorKind,
    /// 人类可读描述（进日志；用户可见文案由上层按 kind 组织，别直接贴这里）。
    pub message: String,
    /// 出错的文件或目录（能定位到就带上）。
    pub path: Option<PathBuf>,
}

impl UpdateError {
    /// 构造错误。
    #[must_use]
    pub fn new(kind: UpdateErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            path: None,
        }
    }

    /// 清单不合法。
    #[must_use]
    pub fn invalid_manifest(message: impl Into<String>) -> Self {
        Self::new(UpdateErrorKind::InvalidManifest, message)
    }

    /// 标注出错路径。
    #[must_use]
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// 分类是否可重试。
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        self.kind.is_retryable()
    }

    /// 把底层 IO 错误包成「某个路径上的 IO 失败」。
    #[must_use]
    pub fn io_at(path: &Path, source: &io::Error) -> Self {
        Self::new(UpdateErrorKind::Io, source.to_string()).with_path(path)
    }
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.path {
            Some(path) => write!(
                f,
                "[{}] {}（{}）",
                self.kind.as_str(),
                self.message,
                path.display()
            ),
            None => write!(f, "[{}] {}", self.kind.as_str(), self.message),
        }
    }
}

impl std::error::Error for UpdateError {}

impl From<VersionError> for UpdateError {
    fn from(error: VersionError) -> Self {
        Self::new(UpdateErrorKind::InvalidVersion, error.to_string())
    }
}
