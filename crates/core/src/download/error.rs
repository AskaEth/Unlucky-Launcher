//! 下载错误：三态分类（同源重试 / 立刻换源 / 整体失败）+ 失败摘要。
//!
//! core 不引 `thiserror`（P0 阶段少一个依赖），手写 `Display`/`Error` 足够。

use std::fmt;

use serde::{Deserialize, Serialize};

/// 错误分类：决定「同源重试」「立刻换源」还是「整体失败」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadErrorKind {
    /// 请求本身不合法（不重试）。
    InvalidRequest,
    /// 404/410/403：该源没有这个资源，立刻换源。
    NotFound,
    /// 5xx / 429：同源退避重试。
    ServerError,
    /// 超时：同源退避重试，多次后换源。
    Timeout,
    /// 连接/传输层错误。
    Network,
    /// 校验不符：同源重试一次后换源（可能是镜像内容不一致）。
    Integrity,
    /// 服务器不支持 Range（内部用于退化单连接，不向上暴露为失败）。
    RangeUnsupported,
    /// 本地 IO 错误（不重试）。
    Io,
    /// 所有候选源都失败。
    AllSourcesFailed,
}

impl DownloadErrorKind {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::NotFound => "not_found",
            Self::ServerError => "server_error",
            Self::Timeout => "timeout",
            Self::Network => "network",
            Self::Integrity => "integrity",
            Self::RangeUnsupported => "range_unsupported",
            Self::Io => "io",
            Self::AllSourcesFailed => "all_sources_failed",
        }
    }

    /// 是否值得在同源上退避重试。
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::ServerError | Self::Timeout | Self::Network | Self::Integrity
        )
    }
}

/// 下载错误。
#[derive(Debug, Clone)]
pub struct DownloadError {
    /// 分类。
    pub kind: DownloadErrorKind,
    /// 出错的源（整体失败时为 `None`）。
    pub source: Option<String>,
    /// 人类可读描述。
    pub message: String,
    /// 已经尝试的次数（含本次）。
    pub attempts: usize,
    /// 其它候选源的失败摘要，便于诊断「到底哪一层挂了」。
    pub suppressed: Vec<String>,
}

impl DownloadError {
    /// 构造错误。
    #[must_use]
    pub fn new(kind: DownloadErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            source: None,
            message: message.into(),
            attempts: 0,
            suppressed: Vec::new(),
        }
    }

    /// 标注出错的源。
    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// 标注尝试次数。
    #[must_use]
    pub fn with_attempts(mut self, attempts: usize) -> Self {
        self.attempts = attempts;
        self
    }

    /// 汇总其它源的失败摘要。
    #[must_use]
    pub fn with_suppressed(mut self, suppressed: Vec<String>) -> Self {
        self.suppressed = suppressed;
        self
    }
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.source {
            Some(source) => write!(
                f,
                "[{}] {}（源 {}）",
                self.kind.as_str(),
                self.message,
                source
            ),
            None => write!(f, "[{}] {}", self.kind.as_str(), self.message),
        }
    }
}

impl std::error::Error for DownloadError {}
