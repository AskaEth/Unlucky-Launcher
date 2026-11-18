//! 一次启动的结果（[`SessionHandle`]）。
//!
//! 从 `launch_session` 拆出：那边是「跑一局 + 收尾」的流水线，这里是**纯数据 + 展示**。
//! 拆开的直接原因是行数——`launch_session` 加上 D6 的有界等待后越过了 400 行红线。

use std::path::PathBuf;

use serde_json::Value;

use crate::launch::args_table::AssembledCommand;
use crate::launch::crash_scan::CrashArtifact;
use crate::launch::exit_analysis::ExitAnalysis;
use crate::launch::session_index::SessionIndexEntry;

/// 一次启动的结果。
#[derive(Debug, Clone)]
pub struct SessionHandle {
    /// 会话 id。
    pub session_id: String,
    /// 事件流文件。
    pub log_path: PathBuf,
    /// 组装结果（`argv()` 含凭据明文；对外展示/上报请用 [`Self::redacted_argv`]）。
    pub command: AssembledCommand,
    /// 脱敏后的完整 argv（accessToken → `<redacted:token>`，主目录 → `<home>`）。
    redacted_argv: Vec<String>,
    /// 退出判定。
    pub exit: ExitAnalysis,
    /// 诊断结论（M15 产出，已写入事件流）。
    pub diagnostics: Option<Value>,
    /// 索引条目（写入 `index.jsonl` 的那一行）。
    pub index: Option<SessionIndexEntry>,
    /// 崩溃现场文件。
    pub artifacts: Vec<CrashArtifact>,
    /// 游戏存活毫秒数。
    pub uptime_ms: u64,
    /// 日志泵是否超时（watchdog 触发 = 有子进程没关 stdout）。
    pub pump_stalled: bool,
    /// 会话写入错误（磁盘问题；不影响进程结果）。
    pub event_write_errors: Vec<String>,
}

impl SessionHandle {
    /// 构造（`redacted_argv` 只在这里赋值，外部拿不到明文 argv 的那条路）。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        session_id: String,
        log_path: PathBuf,
        command: AssembledCommand,
        redacted_argv: Vec<String>,
        exit: ExitAnalysis,
        diagnostics: Option<Value>,
        index: Option<SessionIndexEntry>,
        artifacts: Vec<CrashArtifact>,
        uptime_ms: u64,
        pump_stalled: bool,
        event_write_errors: Vec<String>,
    ) -> Self {
        Self {
            session_id,
            log_path,
            command,
            redacted_argv,
            exit,
            diagnostics,
            index,
            artifacts,
            uptime_ms,
            pump_stalled,
            event_write_errors,
        }
    }

    /// 脱敏后的完整 argv（UI/日志/上报都用它，别直接外发 `command.argv()`）。
    pub fn redacted_argv(&self) -> &[String] {
        &self.redacted_argv
    }

    /// 脱敏后的一行人读命令行。
    pub fn redacted_display(&self) -> String {
        self.redacted_argv
            .iter()
            .map(|arg| crate::launch::command_builder::quote_for_display(arg))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// 是否“正常且无异常”：退出判定 NORMAL 且没有静默失败标记。
    pub fn succeeded(&self) -> bool {
        !self.exit.exit_type.is_abnormal() && self.exit.silent_failure.is_none()
    }

    /// 是否需要进诊断（等价于 `exit.needs_diagnosis`，保留给上层判空）。
    pub fn needs_diagnosis(&self) -> bool {
        self.exit.needs_diagnosis
    }

    /// 结局标签，与 GUI 的 `LogOutcome` 同词（`Ok` / `Crashed` / `SilentFail`）。
    ///
    /// `launcher.log` 里两条路（GUI / `ull launch`）都写这一套词，见
    /// [`crate::launch::launcher_log::launch_end_line`]。
    pub fn outcome_label(&self) -> &'static str {
        if self.exit.silent_failure.is_some() {
            "SilentFail"
        } else if self.succeeded() {
            "Ok"
        } else {
            "Crashed"
        }
    }

    /// 一句话摘要。
    pub fn summary(&self) -> String {
        let mut text = format!("会话 {}：{}", self.session_id, self.exit.summary());
        if self.pump_stalled {
            text.push_str("；日志泵超时");
        }
        if let Some(top) = &self.diagnostics {
            if let Some(id) = top
                .get("candidates")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("id"))
                .and_then(Value::as_str)
            {
                text.push_str(&format!("；诊断首因 {id}"));
            }
        }
        text
    }
}
