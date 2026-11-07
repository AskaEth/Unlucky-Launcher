//! Java 管理模块的错误形状：稳定分类 + 人读说明 + 下一步提示。
//!
//! 与 `core::game::install::InstallError` 同一个套路（分类进 `--json` 的 `error.code`，
//! 说明给人看，提示告诉用户下一步做什么），只是没有复用那个类型——安装层的错误语义
//! 绑在版本本体上，JDK 下载的失败面（API 不可达 / 无对应产物 / 解压后跑不起来）不一样。

use std::fmt;

use crate::download::{DownloadError, DownloadErrorKind};

/// 失败大类（CLI 映射成稳定错误码，脚本可分流）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaErrorKind {
    /// 网络侧：Adoptium API 不可达，或所有候选镜像源都失败。
    Network,
    /// 元数据不认识：API 返回体缺字段、空列表、要的大版本没有 GA 产物。
    Metadata,
    /// 摘要校验不符（镜像给了错内容）或 API 没给可用的 sha256。
    Integrity,
    /// 本地 IO：建目录 / 解压 / 落盘。
    Io,
    /// 当前平台没有对应的 Temurin 产物（架构不认识等）。
    Unsupported,
    /// 装完探测不到可用 java（解压成功但跑不起来 / 版本不符）。
    Probe,
}

impl JavaErrorKind {
    /// 稳定字符串 id（进 `--json` 的 `error.code`）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Network => "java_network",
            Self::Metadata => "java_metadata",
            Self::Integrity => "java_integrity",
            Self::Io => "java_io",
            Self::Unsupported => "java_unsupported",
            Self::Probe => "java_probe",
        }
    }
}

/// Java 管理期错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaError {
    kind: JavaErrorKind,
    message: String,
    hint: Option<String>,
}

impl JavaError {
    /// 构造。
    #[must_use]
    pub fn new(kind: JavaErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            hint: None,
        }
    }

    /// 补一句「下一步怎么办」。
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// 分类。
    #[must_use]
    pub const fn kind(&self) -> JavaErrorKind {
        self.kind
    }

    /// 稳定错误码。
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.kind.as_str()
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

    /// 在说明前面加一段上下文（保留分类与提示）。
    #[must_use]
    pub fn with_context(mut self, context: impl fmt::Display) -> Self {
        self.message = format!("{context}：{}", self.message);
        self
    }
}

impl fmt::Display for JavaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JavaError {}

impl From<DownloadError> for JavaError {
    fn from(error: DownloadError) -> Self {
        let kind = match error.kind {
            DownloadErrorKind::Integrity => JavaErrorKind::Integrity,
            DownloadErrorKind::Io => JavaErrorKind::Io,
            DownloadErrorKind::InvalidRequest => JavaErrorKind::Metadata,
            DownloadErrorKind::NotFound
            | DownloadErrorKind::ServerError
            | DownloadErrorKind::Timeout
            | DownloadErrorKind::Network
            | DownloadErrorKind::RangeUnsupported
            | DownloadErrorKind::AllSourcesFailed => JavaErrorKind::Network,
        };
        let mut message = error.message.clone();
        if let Some(source) = &error.source {
            message.push_str(&format!("（源 {source}）"));
        }
        // 逐源失败摘要：没有这一段，「全部失败」在现场只能靠复现去猜（本轮真机验收就踩到了）。
        if !error.suppressed.is_empty() {
            message.push_str(&format!("；逐源明细：{}", error.suppressed.join(" / ")));
        }
        Self::new(kind, message)
    }
}

/// IO 错误包装（带上「在做什么 / 哪个路径」，日志里才追得动）。
#[must_use]
pub fn io_error(op: &'static str, path: impl fmt::Display, source: &std::io::Error) -> JavaError {
    JavaError::new(JavaErrorKind::Io, format!("{op} 失败（{path}）：{source}"))
}
