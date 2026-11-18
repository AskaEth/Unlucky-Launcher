//! processors 执行器的错误分类（CLI 据此给稳定错误码 `processor_*`）。
//!
//! 与 [`crate::addon::loader::LoaderError`] 分开是刻意的：加载器**解析**阶段的失败
//! （元数据坏了、版本不存在）与**执行**阶段的失败（java 跑挂了、产物没落盘）对用户
//! 是完全不同的两件事——前者换版本/换线路，后者多半是 Java 或磁盘问题。
//! 失败分流也是 mission 的硬要求：`processor_failed` 必须能单独被脚本识别。

use std::fmt;

/// processor 执行失败的大类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessorErrorKind {
    /// 规划期：install profile 形状不对（缺 `jar`、坐标不合法、变量缺定义）。
    Plan,
    /// 找不到可用的 Java（M33 的解析链一个候选都没有）。
    Java,
    /// processor 自己要用的 jar/依赖不在盘上（下载没覆盖到）。
    Library,
    /// 外调 java 失败（非零退出），消息里带脱敏后的 stderr 尾部。
    Failed,
    /// processor 跑完了但产物没落盘。
    Output,
    /// 本地 IO（解包 installer 条目、建目录、复制 MC jar、起进程失败）。
    Io,
}

impl ProcessorErrorKind {
    /// 稳定字符串 id（进 `--json` 的 `error.code`）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "processor_plan",
            Self::Java => "processor_java_missing",
            Self::Library => "processor_library_missing",
            Self::Failed => "processor_failed",
            Self::Output => "processor_output_missing",
            Self::Io => "processor_io",
        }
    }
}

/// processor 执行错误：大类 + 人读说明 + 可选提示 + 可选 stderr 尾部（已脱敏）。
#[derive(Debug, Clone)]
pub struct ProcessorError {
    kind: ProcessorErrorKind,
    message: String,
    hint: Option<String>,
    stderr_tail: Option<String>,
}

impl ProcessorError {
    /// 构造。
    #[must_use]
    pub fn new(kind: ProcessorErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            hint: None,
            stderr_tail: None,
        }
    }

    /// 规划期失败（install profile 形状/变量问题）。
    #[must_use]
    pub fn plan(message: impl Into<String>) -> Self {
        Self::new(ProcessorErrorKind::Plan, message)
    }

    /// 找不到可用的 Java。
    #[must_use]
    pub fn java_missing(message: impl Into<String>) -> Self {
        Self::new(ProcessorErrorKind::Java, message)
    }

    /// processor 依赖不在盘上。
    #[must_use]
    pub fn library_missing(message: impl Into<String>) -> Self {
        Self::new(ProcessorErrorKind::Library, message)
    }

    /// processor 产物没落盘。
    #[must_use]
    pub fn output_missing(message: impl Into<String>) -> Self {
        Self::new(ProcessorErrorKind::Output, message)
    }

    /// 本地 IO 失败。
    #[must_use]
    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ProcessorErrorKind::Io, message)
    }

    /// 外调 java 非零退出：消息 = 说明 + 脱敏后的 stderr 尾部。
    #[must_use]
    pub fn failed(label: &str, exit: &str, stderr_tail: &str) -> Self {
        let mut message = format!("processor {label} 执行失败（退出码 {exit}）");
        if !stderr_tail.trim().is_empty() {
            message.push_str("；stderr 尾部：\n");
            message.push_str(stderr_tail.trim_end());
        }
        let mut error = Self::new(ProcessorErrorKind::Failed, message);
        error.stderr_tail = Some(stderr_tail.trim_end().to_string());
        error
    }

    /// 补一句提示。
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// 大类。
    #[must_use]
    pub const fn kind(&self) -> ProcessorErrorKind {
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

    /// 脱敏后的 stderr 尾部（只有 `processor_failed` 会有）。
    #[must_use]
    pub fn stderr_tail(&self) -> Option<&str> {
        self.stderr_tail.as_deref()
    }
}

impl fmt::Display for ProcessorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ProcessorError {}

#[cfg(test)]
mod tests {
    use super::{ProcessorError, ProcessorErrorKind};

    #[test]
    fn stable_codes_are_distinct_and_prefixed() {
        let kinds = [
            ProcessorErrorKind::Plan,
            ProcessorErrorKind::Java,
            ProcessorErrorKind::Library,
            ProcessorErrorKind::Failed,
            ProcessorErrorKind::Output,
            ProcessorErrorKind::Io,
        ];
        for kind in kinds {
            assert!(kind.as_str().starts_with("processor_"), "{}", kind.as_str());
        }
        assert_eq!(ProcessorErrorKind::Failed.as_str(), "processor_failed");
    }

    #[test]
    fn failed_carries_the_stderr_tail_in_message_and_accessor() {
        let error = ProcessorError::failed("g:a:1", "1", "boom\nCaused by: x");
        assert_eq!(error.kind(), ProcessorErrorKind::Failed);
        assert!(error.message().contains("退出码 1"), "{}", error.message());
        assert!(error.message().contains("Caused by: x"));
        assert_eq!(error.stderr_tail(), Some("boom\nCaused by: x"));
    }

    #[test]
    fn empty_stderr_keeps_the_message_clean() {
        let error = ProcessorError::failed("g:a:1", "255", "");
        assert!(!error.message().contains("stderr 尾部"));
        assert_eq!(error.stderr_tail(), Some(""));
    }
}
