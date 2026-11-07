//! 运行中的启动会话与收尾流水线。
//!
//! 一次启动的生命周期（`DevDocs/PlanDoc/06-launch-and-diagnostics.md` §1 的第 7~10 阶段）：
//! `spawn` → 用户可能 [`LaunchSession::stop`] → [`LaunchSession::finish`]
//! （等进程 → 等日志泵（有预算）→ 退出分析 → 扫崩溃现场 → 诊断挂钩 → 写事件 + 索引）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use serde_json::Value;

use crate::launch::args_table::AssembledCommand;
use crate::launch::crash_scan::{self, CrashArtifact};
use crate::launch::diagnosis_hook::{DiagnosisHook, DiagnosisRequest};
use crate::launch::error::Result;
use crate::launch::exit_analysis::{self, ExitAnalysis, ExitSignals};
use crate::launch::launcher::LauncherConfig;
use crate::launch::process::{self, RunningProcess};
use crate::launch::pump::{LineSink, LogLine};
use crate::launch::redact::Redactor;
use crate::launch::request::LaunchRequest;
use crate::launch::session::{EndReason, SessionSink};
use crate::launch::session_index::SessionIndexEntry;

/// 一次运行中的启动会话。
pub struct LaunchSession {
    config: LauncherConfig,
    hook: Arc<dyn DiagnosisHook>,
    redactor: Redactor,
    request: LaunchRequest,
    session_id: String,
    log_path: PathBuf,
    command: AssembledCommand,
    pid: u32,
    process: RunningProcess,
    sink: Arc<SessionSink>,
    started_wall: SystemTime,
    killed: bool,
}

impl std::fmt::Debug for LaunchSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchSession")
            .field("session_id", &self.session_id)
            .field("pid", &self.pid)
            .field("log_path", &self.log_path)
            .finish()
    }
}

impl LaunchSession {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        config: LauncherConfig,
        hook: Arc<dyn DiagnosisHook>,
        redactor: Redactor,
        request: LaunchRequest,
        session_id: String,
        log_path: PathBuf,
        command: AssembledCommand,
        pid: u32,
        process: RunningProcess,
        sink: Arc<SessionSink>,
    ) -> Self {
        Self {
            config,
            hook,
            redactor,
            request,
            session_id,
            log_path,
            command,
            pid,
            process,
            sink,
            started_wall: SystemTime::now(),
            killed: false,
        }
    }

    /// 进程号。
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// 会话 id。
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 事件流文件路径。
    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    /// 组装结果。
    pub fn command(&self) -> &AssembledCommand {
        &self.command
    }

    /// 是否已请求停止。
    pub fn is_killed(&self) -> bool {
        self.killed
    }

    /// 日志 sink（可再叠加 UI/测试消费者，见 [`crate::launch::sink`]）。
    pub fn sink(&self) -> Arc<dyn LineSink> {
        self.sink.clone() as Arc<dyn LineSink>
    }

    /// 已运行毫秒数。
    pub fn uptime_ms(&self) -> u64 {
        self.process.uptime_ms()
    }

    /// 用户/看门狗请求停止：三段式回收进程树，并记 `killed` 事件。
    pub fn stop(&mut self) -> Result<()> {
        if self.killed {
            return Ok(());
        }
        self.killed = true;
        if let Some(result) = self.sink.with_writer(|writer| writer.killed("user", None)) {
            let _ = result;
        }
        self.process.stop()
    }

    /// 收尾：等进程 → 等日志泵 → 退出分析 → 现场扫描 → 诊断 → 事件 + 索引。
    pub fn finish(mut self) -> Result<SessionHandle> {
        let status = self.process.wait()?;
        let code = process::exit_code(&status);
        // uptime 必须在等日志泵之前取（泵的等待预算不算进游戏存活时间）。
        let uptime_ms = self.process.uptime_ms();
        let verdict = self.process.supervise();
        let pump_stalled = !verdict.is_complete();
        let lines = self.process.lines();

        let mut signals = ExitSignals::new(code, uptime_ms, &lines);
        signals.killed = self.killed;
        signals.is_posix = self.config.platform.os.is_posix();
        let exit = exit_analysis::analyze(&signals);

        let artifacts = self.collect_artifacts();
        // 脱敏只算一次：诊断包与 `SessionHandle::redacted_argv()` 共用。
        let redacted_argv = self.redactor.scrub_all(&self.command.argv());
        let diagnosis_request = self.build_diagnosis_request(
            &exit,
            &lines,
            &artifacts,
            uptime_ms,
            redacted_argv.clone(),
        );
        let should_diagnose = exit.needs_diagnosis || !artifacts.is_empty();
        let diagnosis = if should_diagnose {
            self.hook.analyze(&diagnosis_request)
        } else {
            None
        };

        let mut event_write_errors = Vec::new();
        let mut diagnostics = None;
        let mut index = None;
        let end_reason = if self.killed {
            EndReason::Killed
        } else {
            EndReason::Exit
        };
        let completed = self.sink.with_writer(|writer| {
            if let Err(err) = writer.exit(&exit) {
                event_write_errors.push(err.to_string());
            }
            for artifact in &artifacts {
                let result = writer.crash_found(
                    artifact.source.as_str(),
                    &artifact.path,
                    artifact.excerpt.as_deref(),
                );
                if let Err(err) = result {
                    event_write_errors.push(err.to_string());
                }
            }
            if let Some(record) = diagnosis {
                if let Err(err) = writer.diagnosis(record.clone()) {
                    event_write_errors.push(err.to_string());
                }
                diagnostics = Some(record);
            }
            match writer.finish(end_reason) {
                Ok(entry) => index = Some(entry),
                Err(err) => event_write_errors.push(err.to_string()),
            }
        });
        if completed.is_none() {
            event_write_errors.push("会话写入器不可用（锁中毒）".to_string());
        }

        Ok(SessionHandle {
            session_id: self.session_id,
            log_path: self.log_path,
            command: self.command,
            redacted_argv,
            exit,
            diagnostics,
            index,
            artifacts,
            uptime_ms,
            pump_stalled,
            event_write_errors,
        })
    }

    fn collect_artifacts(&self) -> Vec<CrashArtifact> {
        if !self.config.scan_crash_reports {
            return Vec::new();
        }
        let run_dir = self.run_dir();
        crash_scan::scan(&run_dir, self.started_wall, self.config.max_crash_artifacts)
    }

    fn build_diagnosis_request(
        &self,
        exit: &ExitAnalysis,
        lines: &[LogLine],
        artifacts: &[CrashArtifact],
        uptime_ms: u64,
        argv: Vec<String>,
    ) -> DiagnosisRequest {
        DiagnosisRequest {
            session_id: self.session_id.clone(),
            instance_id: self.request.instance.id.clone(),
            instance_name: self.request.instance.name.clone(),
            game_version: self.request.instance.version.raw().to_string(),
            exit: exit.clone(),
            uptime_ms,
            log_lines: lines
                .iter()
                .map(|line| LogLine::new(line.stream, self.redactor.scrub(&line.text).into_owned()))
                .collect(),
            artifacts: artifacts.to_vec(),
            argv,
            memory_mb: self.command.memory.max_mb,
            java_major: self.request.java.major,
            java_binary: self.request.java.binary.clone(),
            game_dir: self.run_dir(),
        }
    }

    fn run_dir(&self) -> PathBuf {
        if self.request.dirs.game_dir.as_os_str().is_empty() {
            std::env::current_dir().unwrap_or_default()
        } else {
            self.request.dirs.game_dir.clone()
        }
    }
}

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
