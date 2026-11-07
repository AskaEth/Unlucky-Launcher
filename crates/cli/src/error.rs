//! 错误形状与退出码。
//!
//! 约定（10-cli-design.md §3）：**0 成功 / 1 用户错误 / 2 内部错误**。
//! 用户错误 = 用法错、目标不存在、环境不支持、依赖模块未接线；
//! 内部错误 = 不该发生的事（序列化失配等），脚本见到 2 应该当 bug 报。

use std::fmt;

use serde_json::{json, Value};

/// 退出码：成功。
pub const EXIT_OK: i32 = 0;
/// 退出码：用户错误。
pub const EXIT_USER: i32 = 1;
/// 退出码：内部错误。
pub const EXIT_INTERNAL: i32 = 2;

/// 错误大类（决定退出码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// 用户侧问题：改参数/改环境可解决。
    User,
    /// 内部问题：不该发生，应报 bug。
    Internal,
}

impl ErrorKind {
    /// 稳定名（`--json` 输出用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Internal => "internal",
        }
    }

    /// 对应的进程退出码。
    #[must_use]
    pub const fn exit_code(self) -> i32 {
        match self {
            Self::User => EXIT_USER,
            Self::Internal => EXIT_INTERNAL,
        }
    }
}

/// 一条 CLI 错误。
///
/// `code` 是给脚本的稳定机读标识（`usage` / `not_found` / `unsupported_os` …），
/// `message` 是给人看的中文说明，`hint` 是「下一步该做什么」。
#[derive(Debug, Clone)]
pub struct CliError {
    kind: ErrorKind,
    code: &'static str,
    message: String,
    hint: Option<String>,
}

/// 把模板里的程序名占位符渲染掉（M82 r3）。
///
/// 渲染放在**构造时**，而不是打印时：错误有两条出口（人类可读 stderr、`--json` 信封），
/// 只在打印处渲染会漏掉 JSON 那条（r2 就是这么漏的，`%prog%` 直接出现在了 JSON 里）。
fn render(text: impl Into<String>) -> String {
    crate::cli::program::fill(&text.into())
}

impl CliError {
    /// 用法错误（参数不对）。
    #[must_use]
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::User,
            code: "usage",
            message: render(message),
            hint: None,
        }
    }

    /// 用户错误，带稳定 code。
    #[must_use]
    pub fn user(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::User,
            code,
            message: render(message),
            hint: None,
        }
    }

    /// 内部错误，带稳定 code。
    #[must_use]
    pub fn internal(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Internal,
            code,
            message: render(message),
            hint: None,
        }
    }

    /// IO 失败（读配置/读写会话文件）：按用户错误处理，路径进消息。
    #[must_use]
    pub fn io(op: &str, path: &std::path::Path, error: &std::io::Error) -> Self {
        Self::user("io", format!("{op}失败：{}（{error}）", path.display()))
    }

    /// 补一句提示。
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(render(hint));
        self
    }

    /// 错误大类。
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// 稳定错误码。
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    /// 人类可读说明。
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 下一步提示。
    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }

    /// 进程退出码。
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        self.kind.exit_code()
    }

    /// `--json` 里的错误对象。
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut error = json!({
            "code": self.code,
            "kind": self.kind.as_str(),
            "message": self.message,
        });
        if let Some(hint) = &self.hint {
            error["hint"] = Value::String(hint.clone());
        }
        json!({ "ok": false, "error": error })
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for CliError {}
