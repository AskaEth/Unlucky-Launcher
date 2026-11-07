//! auth 失败事件的落盘（`.ULL/logs/auth.jsonl`，M38）。
//!
//! 起因：MSA 真机验收两轮 403，`ull accounts add msa` 失败后终端只有一行文案，
//! 验收人拿不到状态码/正文摘要去对账。这里把**失败**写成一行一条 JSONL：
//! 便携模式数据根还没初始化时会先补建 `logs/` 分区，403 的细节不再随终端滚走。
//!
//! 三条纪律：
//! 1. **只记失败**，不记成功（成功路径的凭据载荷一律不落日志）；
//! 2. **记的是诊断字段**（`code` / `detail_code` / `status` / `content_type` / `body_kind` /
//!    已脱敏的 `server_message`、`digest`），没有任何 token/密码字段；
//! 3. **落盘是尽力而为**：写日志失败绝不让登录本身失败（调用方忽略返回值）。
//!
//! 数据根从哪来：默认走 `platform::data_dir` 的 **Portable** 解析（exe 同目录的 `.ULL`，
//! 跟随 `redirect.json`）。宿主若用 `--data-dir` 等非便携模式，应显式
//! [`AuthJournal::in_logs_dir`] 后交给 [`crate::auth::MicrosoftAuth::with_journal`]。

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::auth::body::BodyKind;
use crate::auth::error::AuthError;
use crate::launch::redact::Redactor;
use crate::platform::data_dir::{DataDirRequest, Resolver};
use crate::platform::layout::LauncherLayout;
use crate::platform::paths::Platform;

/// 认证失败事件文件名（在 `.ULL/logs/` 下）。
pub const AUTH_LOG_FILE: &str = "auth.jsonl";

/// 一行认证失败事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthFailureEvent {
    /// Unix 毫秒。
    pub ts_ms: u64,
    /// 触发入口（`device_code_login` / `refresh` / `validate`）。
    pub scope: String,
    /// 链上的步骤（`AuthError::Response` 才有）。
    pub step: Option<String>,
    /// 粗分流码（[`AuthError::code`]）。
    pub code: String,
    /// 细分码（[`AuthError::detail_code`]）。
    pub detail_code: String,
    /// HTTP 状态码（链上拒绝才有）。
    pub status: Option<u16>,
    /// `Content-Type`（链上拒绝才有）。
    pub content_type: Option<String>,
    /// 正文形状（链上拒绝才有）。
    pub body_kind: Option<BodyKind>,
    /// 服务端文案（已脱敏）。
    pub server_message: Option<String>,
    /// 正文摘要（已脱敏，≤200 字符）。
    pub digest: Option<String>,
    /// 是否可重试（[`AuthError::is_transient`]）。
    pub transient: bool,
    /// 人读一句话（已脱敏，不含凭据）。
    pub message: String,
}

impl AuthFailureEvent {
    /// 从一条错误构造（`scope` 说明是哪个入口失败的）。
    #[must_use]
    pub fn of(scope: &str, error: &AuthError) -> Self {
        let detail = match error {
            AuthError::Response(detail) => Some(detail.as_ref()),
            _ => None,
        };
        let scrub = |text: &str| Redactor::new().scrub(text).into_owned();
        Self {
            ts_ms: crate::auth::microsoft::now_ms(),
            scope: scope.to_string(),
            step: detail.map(|detail| detail.step.clone()),
            code: error.code().to_string(),
            detail_code: error.detail_code(),
            status: detail.map(|detail| detail.status),
            content_type: detail.and_then(|detail| detail.content_type.clone()),
            body_kind: detail.map(|detail| detail.kind),
            server_message: detail
                .and_then(|detail| detail.server_message.as_deref())
                .map(scrub),
            digest: detail.map(|detail| scrub(&detail.digest)),
            transient: error.is_transient(),
            message: scrub(&error.message()),
        }
    }

    /// JSON 形状（字段名稳定，给验收/排障工具读）。
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "ts_ms": self.ts_ms,
            "level": "error",
            "event": "auth_failure",
            "scope": self.scope,
            "step": self.step,
            "code": self.code,
            "detail_code": self.detail_code,
            "status": self.status,
            "content_type": self.content_type,
            "body_kind": self.body_kind.map(BodyKind::as_str),
            "server_message": self.server_message,
            "digest": self.digest,
            "transient": self.transient,
            "message": self.message,
        })
    }
}

/// 认证失败日志（JSONL，一行一条）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthJournal {
    path: PathBuf,
}

impl AuthJournal {
    /// 落在某个 `logs/` 分区下（`<logs>/auth.jsonl`）。
    #[must_use]
    pub fn in_logs_dir(logs: impl Into<PathBuf>) -> Self {
        Self {
            path: logs.into().join(AUTH_LOG_FILE),
        }
    }

    /// 直接指定文件路径。
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 便携模式默认位置（exe 同目录 `.ULL/logs/auth.jsonl`，跟随 `redirect.json`）。
    ///
    /// 拿不到 exe 目录（极少数嵌入场景）时返回 `None`——没有可写的日志根就不写。
    #[must_use]
    pub fn portable_default() -> Option<Self> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))?;
        let resolver = Resolver::new(Platform::current(), Some(exe_dir));
        let resolved = resolver.resolve_default(&DataDirRequest::default()).ok()?;
        Some(Self::in_logs_dir(
            LauncherLayout::new(resolved.launcher_home).logs(),
        ))
    }

    /// 日志文件路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 追加一条事件（父目录不存在时先建）。
    ///
    /// # Errors
    ///
    /// 建目录或追加失败；调用方按「尽力而为」处理即可。
    pub fn record(&self, event: &AuthFailureEvent) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut line = serde_json::to_string(&event.to_json()).unwrap_or_default();
        line.push('\n');
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(line.as_bytes())?;
        file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthFailureEvent, AuthJournal};
    use crate::auth::body::{BodyKind, ResponseDetail};
    use crate::auth::error::AuthError;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ull-auth-journal-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    fn blocked_error() -> AuthError {
        AuthError::Response(Box::new(ResponseDetail {
            step: "login_with_xbox".to_string(),
            what: "Minecraft 登录被拒".to_string(),
            status: 403,
            content_type: Some("text/html".to_string()),
            kind: BodyKind::Html,
            server_message: None,
            digest: "<html>blocked</html>".to_string(),
        }))
    }

    #[test]
    fn event_carries_structured_diagnostics() {
        let event = AuthFailureEvent::of("device_code_login", &blocked_error());
        assert_eq!(event.scope, "device_code_login");
        assert_eq!(event.step.as_deref(), Some("login_with_xbox"));
        assert_eq!(event.code, "denied");
        assert_eq!(event.detail_code, "login_with_xbox_non_json");
        assert_eq!(event.status, Some(403));
        assert_eq!(event.content_type.as_deref(), Some("text/html"));
        assert_eq!(event.body_kind, Some(BodyKind::Html));
        assert!(event.transient);
        let value = event.to_json();
        assert_eq!(value["event"], "auth_failure");
        assert_eq!(value["level"], "error");
        assert_eq!(value["body_kind"], "html");
        assert!(value["message"]
            .as_str()
            .is_some_and(|text| !text.is_empty()));
    }

    #[test]
    fn non_chain_errors_still_record() {
        let event = AuthFailureEvent::of("refresh", &AuthError::Canceled);
        assert_eq!(event.detail_code, "canceled");
        assert_eq!(event.step, None);
        assert_eq!(event.status, None);
        assert!(!event.transient);
    }

    #[test]
    fn record_initializes_the_logs_dir_and_appends_one_line() {
        let root = temp_dir("init");
        let logs = root.join("logs");
        let journal = AuthJournal::in_logs_dir(&logs);
        assert!(!logs.exists(), "初始不该存在（模拟未初始化的数据根）");
        journal
            .record(&AuthFailureEvent::of("device_code_login", &blocked_error()))
            .expect("写日志");
        journal
            .record(&AuthFailureEvent::of("refresh", &AuthError::Canceled))
            .expect("写日志");
        let text = std::fs::read_to_string(journal.path()).expect("读日志");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "一行一条：{text}");
        for line in lines {
            let value: serde_json::Value = serde_json::from_str(line).expect("每行都是合法 JSON");
            assert_eq!(value["event"], "auth_failure");
        }
        // 日志里不该出现任何凭据字段名。
        for forbidden in ["access_token", "refresh_token", "password", "token"] {
            assert!(!text.contains(forbidden), "日志出现了 {forbidden}：{text}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn journal_paths_are_layout_based() {
        let journal = AuthJournal::in_logs_dir("/tmp/ull-logs");
        assert!(journal.path().ends_with("auth.jsonl"));
        let explicit = AuthJournal::at("/tmp/x/custom.jsonl");
        assert_eq!(explicit.path(), std::path::Path::new("/tmp/x/custom.jsonl"));
    }
}
