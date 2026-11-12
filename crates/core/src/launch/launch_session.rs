//! 运行中的启动会话与收尾流水线。
//!
//! 一次启动的生命周期（`DevDocs/PlanDoc/06-launch-and-diagnostics.md` §1 的第 7~10 阶段）：
//! `spawn` → 用户可能 [`LaunchSession::stop`] → [`LaunchSession::finish`]
//! （**有界**等进程 → 等日志泵（有预算）→ 退出分析 → 扫崩溃现场 → 诊断挂钩 → 写事件 + 索引）。
//!
//! 「有界」是 D6：等进程那一步会按 [`crate::launch::startup_watch`] 的判据停下（窗口从未出现 /
//! 已出现关闭标记），到点**不杀进程**，只结束本次会话并报 [`crate::launch::error::LaunchError::Watchdog`]。
//!
//! 结果是 [`SessionHandle`]（拆到 `session_handle`：那边是纯数据 + 展示）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::launch::args_table::AssembledCommand;
use crate::launch::crash_scan::{self, CrashArtifact};
use crate::launch::diagnosis_hook::{DiagnosisHook, DiagnosisRequest};
use crate::launch::error::{LaunchError, Result};
use crate::launch::exit_analysis::{self, ExitAnalysis, ExitSignals};
use crate::launch::kill_registry;
use crate::launch::launcher::LauncherConfig;
use crate::launch::process::{self, RunningProcess};
use crate::launch::pump::{LineSink, LogLine};
use crate::launch::redact::Redactor;
use crate::launch::request::LaunchRequest;
use crate::launch::session::{EndReason, SessionSink};
use crate::launch::startup_watch::{StartupWatch, WatchVerdict};

pub use crate::launch::session_handle::SessionHandle;

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

/// 有界等待的结果（D6）。
enum WaitOutcome {
    /// 进程已退出。
    Exited(std::process::ExitStatus),
    /// 看门狗判出「活着但没动静」：不杀进程，只是不再等。
    GaveUp(WatchVerdict),
}

impl LaunchSession {
    /// 构造一次会话。
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

    /// 收尾：**有界地**等进程 → 等日志泵 → 退出分析 → 现场扫描 → 诊断 → 事件 + 索引。
    ///
    /// 「有界」是 D6：到点（[`Self::wait_bounded`] 的判据）只**停止等待**，绝不杀进程——
    /// 用户的加载进度不该被启动器清掉。走那条路时先写 `session_end`（`still_running`）
    /// 与一行 `launcher.log`，再返回 [`LaunchError::Watchdog`]。
    pub fn finish(mut self) -> Result<SessionHandle> {
        let status = match self.wait_bounded()? {
            WaitOutcome::Exited(status) => status,
            WaitOutcome::GaveUp(verdict) => return self.abandon(verdict),
        };
        let code = process::exit_code(&status);
        // uptime 必须在等日志泵之前取（泵的等待预算不算进游戏存活时间）。
        let uptime_ms = self.process.uptime_ms();
        let verdict = self.process.supervise();
        let pump_stalled = !verdict.is_complete();
        let lines = self.process.lines();

        let mut signals = ExitSignals::new(code, uptime_ms, &lines);
        // D10：判定「是不是我们杀的」有两个来源——本会话自己的 `stop()`，以及发送侧
        // （`tree_kill`）留下的登记。GUI 从别的线程 kill 的那条路只经过后者。
        let killed = self.killed || kill_registry::take(self.pid);
        signals.killed = killed;
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
        // M211/D3：崩溃结论只认真实证据（权威现场文件 / 非 0 退出 + 错误行），不再由
        // 「有没有 latest.log」推出——那是每局都有的文件。
        let crash_evidence = crash_scan::has_crash_evidence(&artifacts, &exit);
        let end_reason = if killed {
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
            writer.mark_crash(crash_evidence);
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

        Ok(SessionHandle::new(
            self.session_id,
            self.log_path,
            self.command,
            redacted_argv,
            exit,
            diagnostics,
            index,
            artifacts,
            uptime_ms,
            pump_stalled,
            event_write_errors,
        ))
    }

    /// 有界等待：要么进程退出，要么看门狗判出「活着但没动静」（D6）。
    ///
    /// 判据在 [`StartupWatch`]（纯逻辑，`startup_watch` 的测试钉住）；这里只把三个信号喂进去：
    /// 进程是否已退出、窗口是否**出现过**、日志里是否出现了关闭标记。
    fn wait_bounded(&mut self) -> Result<WaitOutcome> {
        let limits = self.config.process;
        let mut watch = StartupWatch::new(limits.startup_timeout, limits.shutdown_grace);
        let started = self.process.started_at();
        loop {
            if let Some(status) = self.process.try_wait()? {
                return Ok(WaitOutcome::Exited(status));
            }
            if self.sink.window_detected() {
                watch.mark_window();
            }
            if let Some(at) = self.sink.last_line_at() {
                watch.mark_activity(at.saturating_duration_since(started));
            }
            if let Some(at) = self.sink.shutdown_seen_at() {
                watch.mark_shutdown(at.saturating_duration_since(started));
            }
            let verdict = watch.verdict(started.elapsed());
            if !verdict.is_wait() {
                return Ok(WaitOutcome::GaveUp(verdict));
            }
            std::thread::sleep(process::POLL_INTERVAL);
        }
    }

    /// 放弃等待：**不杀进程**，把现场写清楚，返回结构化错误。
    ///
    /// 三条落点：① `launcher.log` 一行（含判据、上限、pid）；② 会话事件流写
    /// `session_end`（`still_running`）并追加索引，历史里看得见这一局；③ 错误本身带
    /// 会话路径与可调的环境变量。
    fn abandon(self, verdict: WatchVerdict) -> Result<SessionHandle> {
        let pid = self.pid;
        let spent_ms = verdict.waited_ms();
        crate::launch::launcher_log::record(
            "launch",
            &format!(
                "instance={} session={} 看门狗：{} ⇒ 停止等待（pid {pid} 仍在运行，未结束）",
                self.request.instance.id,
                self.session_id,
                verdict.describe()
            ),
        );
        let _ = self
            .sink
            .with_writer(|writer| writer.finish(EndReason::StillRunning));
        Err(LaunchError::Watchdog {
            pid,
            reason: verdict.code(),
            waited_ms: spent_ms,
            limit_ms: verdict.limit_ms(),
            log_path: self.log_path,
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
