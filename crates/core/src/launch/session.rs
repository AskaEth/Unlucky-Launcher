//! JSONL 会话事件流：`<sessions_root>/game/<session-id>.jsonl`。
//!
//! schema 见 `DevDocs/SubAgentsTalk/launch-接口约定.md` §2/§3（v1）——M15 诊断采集器与
//! M16 `read_game_log`/`list_game_sessions` 消费同一份文件，是**单一真相源**。
//!
//! 两条纪律写死在实现里：
//! 1. 脱敏在写入前做（[`Redactor`]），`cmd_built.argv` 也不例外（命令行必含 accessToken）；
//! 2. 「launch 报事实、M15 下结论」：`diagnosis` 事件由 M15 产出、由本模块落盘。

use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::launch::args_table::AssembledCommand;
use crate::launch::error::{AssemblyError, LaunchError, Result};
use crate::launch::exit_analysis::ExitAnalysis;
use crate::launch::natives::NativesReport;
use crate::launch::pump::{LogLine, Stream};
use crate::launch::redact::Redactor;
use crate::launch::session_index::{self, SessionIndexEntry};
use crate::launch::session_payload as payload;
use crate::launch::time::{self, SessionClock};
use crate::launch::window_detect::Detection;

/// 事件 schema 版本。
pub const SCHEMA_VERSION: u8 = 1;

/// 事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// 会话开始。
    SessionStart,
    /// Java 选择结果。
    JavaSelected,
    /// 命令行组装完成。
    CmdBuilt,
    /// natives 解压结果。
    NativesExtracted,
    /// 启动前失败（没有进程）。
    AssemblyFailed,
    /// 进程已启动。
    ProcStart,
    /// stdout 行。
    Stdout,
    /// stderr 行。
    Stderr,
    /// 检测到游戏窗口。
    WindowDetected,
    /// 进程退出。
    Exit,
    /// 发现崩溃报告 / hs_err。
    CrashFound,
    /// 诊断结论（M15 产出）。
    Diagnosis,
    /// 被主动结束。
    Killed,
    /// 会话结束。
    SessionEnd,
}

impl EventKind {
    /// JSONL 里的 `kind` 字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::JavaSelected => "java_selected",
            Self::CmdBuilt => "cmd_built",
            Self::NativesExtracted => "natives_extracted",
            Self::AssemblyFailed => "assembly_failed",
            Self::ProcStart => "proc_start",
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
            Self::WindowDetected => "window_detected",
            Self::Exit => "exit",
            Self::CrashFound => "crash_found",
            Self::Diagnosis => "diagnosis",
            Self::Killed => "killed",
            Self::SessionEnd => "session_end",
        }
    }
}

/// 会话结束原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// 进程自然退出。
    Exit,
    /// 被主动结束。
    Killed,
    /// 启动前失败（组装/组件检查）。
    AssemblyFailed,
    /// 进程派生失败。
    SpawnFailed,
}

impl EndReason {
    /// JSONL 里的 `reason` 字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exit => "exit",
            Self::Killed => "killed",
            Self::AssemblyFailed => "assembly_failed",
            Self::SpawnFailed => "spawn_failed",
        }
    }
}

pub use crate::launch::session_meta::SessionMeta;
pub use crate::launch::session_sink::SessionSink;

#[derive(Serialize)]
struct EventLine<'a> {
    v: u8,
    seq: u64,
    ts: String,
    t_ms: u64,
    kind: &'a str,
    session_id: &'a str,
    payload: &'a Value,
}

/// 会话事件流写入器。
#[derive(Debug)]
pub struct SessionWriter {
    session_id: String,
    clock: SessionClock,
    file: BufWriter<File>,
    path: PathBuf,
    index_path: PathBuf,
    seq: u64,
    event_count: u64,
    redactor: Redactor,
    meta: SessionMeta,
    started_at: String,
    window_detected: bool,
    crash_found: bool,
    exit_code: Option<i32>,
    exit_type: Option<String>,
    silent_failure: bool,
    diagnosis_top: Option<String>,
    finished: bool,
}

impl SessionWriter {
    /// 新建会话（id 由时间 + pid 生成）。
    pub fn create(sessions_root: &Path, meta: SessionMeta, redactor: Redactor) -> Result<Self> {
        let session_id = time::make_session_id(time::now(), std::process::id());
        Self::create_with_id(sessions_root, &session_id, meta, redactor)
    }

    /// 用指定 id 新建会话（测试与“续写”场景）。
    pub fn create_with_id(
        sessions_root: &Path,
        session_id: &str,
        meta: SessionMeta,
        redactor: Redactor,
    ) -> Result<Self> {
        let dir = sessions_root.join("game");
        fs::create_dir_all(&dir).map_err(|source| LaunchError::Io {
            op: "create_session_dir",
            path: dir.clone(),
            source,
        })?;
        let path = dir.join(format!("{session_id}.jsonl"));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|source| LaunchError::Io {
                op: "create_session_file",
                path: path.clone(),
                source,
            })?;
        let clock = SessionClock::start_new();
        let started_at = clock.started_at_iso();
        Ok(Self {
            session_id: session_id.to_string(),
            clock,
            file: BufWriter::new(file),
            path,
            index_path: session_index::index_path(sessions_root),
            seq: 0,
            event_count: 0,
            redactor,
            meta,
            started_at,
            window_detected: false,
            crash_found: false,
            exit_code: None,
            exit_type: None,
            silent_failure: false,
            diagnosis_top: None,
            finished: false,
        })
    }

    /// 会话 id。
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 事件文件路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 已写入事件数。
    pub fn event_count(&self) -> u64 {
        self.event_count
    }

    /// 是否已结束。
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// 写一条事件。
    pub fn record(&mut self, kind: EventKind, payload: Value) -> Result<()> {
        let seq = self.seq;
        let line = EventLine {
            v: SCHEMA_VERSION,
            seq,
            ts: self.clock.now_iso(),
            t_ms: self.clock.elapsed_ms(),
            kind: kind.as_str(),
            session_id: &self.session_id,
            payload: &payload,
        };
        let serialized = serde_json::to_string(&line).map_err(|err| LaunchError::Session {
            path: self.path.clone(),
            reason: err.to_string(),
        })?;
        self.file
            .write_all(serialized.as_bytes())
            .and_then(|()| self.file.write_all(b"\n"))
            .and_then(|()| self.file.flush())
            .map_err(|source| LaunchError::Io {
                op: "write_session_event",
                path: self.path.clone(),
                source,
            })?;
        self.seq += 1;
        self.event_count += 1;
        Ok(())
    }

    /// `session_start`。
    pub fn session_start(&mut self) -> Result<()> {
        let payload = payload::session_start(&self.meta, SCHEMA_VERSION);
        self.record(EventKind::SessionStart, payload)
    }

    /// `cmd_built`：完整 argv（已脱敏）+ classpath 摘要。
    pub fn cmd_built(&mut self, command: &AssembledCommand) -> Result<()> {
        let payload = payload::cmd_built(command, &self.redactor);
        self.record(EventKind::CmdBuilt, payload)
    }

    /// `assembly_failed`：启动前失败，附结构化缺失清单（M15 据此下结论）。
    pub fn assembly_failed(&mut self, error: &AssemblyError) -> Result<()> {
        let payload = payload::assembly_failed(error);
        self.record(EventKind::AssemblyFailed, payload)
    }

    /// `proc_start`。
    pub fn proc_start(&mut self, pid: u32) -> Result<()> {
        let payload = payload::proc_start(pid, &self.clock.now_iso());
        self.record(EventKind::ProcStart, payload)
    }

    /// `natives_extracted`：natives 解压结果（启动期 `UnsatisfiedLinkError` 的第一嫌疑）。
    pub fn natives_extracted(&mut self, report: &NativesReport) -> Result<()> {
        let payload = payload::natives_extracted(report, &self.redactor);
        self.record(EventKind::NativesExtracted, payload)
    }

    /// `stdout` / `stderr` 行事件。
    pub fn line(&mut self, line: &LogLine) -> Result<()> {
        let kind = match line.stream {
            Stream::Stdout => EventKind::Stdout,
            Stream::Stderr => EventKind::Stderr,
        };
        let text = self.redactor.scrub(&line.text).into_owned();
        let payload = payload::log_line(&text, line.level());
        self.record(kind, payload)
    }

    /// `window_detected`。
    pub fn window_detected(&mut self, detection: &Detection) -> Result<()> {
        self.window_detected = true;
        let payload = payload::window_detected(detection, &self.redactor);
        self.record(EventKind::WindowDetected, payload)
    }

    /// 是否已检测到游戏窗口（索引摘要用）。
    pub fn is_window_detected(&self) -> bool {
        self.window_detected
    }

    /// `killed`。
    pub fn killed(&mut self, by: &str, signal: Option<&str>) -> Result<()> {
        let payload = payload::killed(by, signal);
        self.record(EventKind::Killed, payload)
    }

    /// `exit`。
    pub fn exit(&mut self, analysis: &ExitAnalysis) -> Result<()> {
        self.exit_code = Some(analysis.code);
        self.exit_type = Some(analysis.exit_type.as_str().to_string());
        self.silent_failure = analysis.silent_failure.is_some();
        let payload = payload::exit(analysis);
        self.record(EventKind::Exit, payload)
    }

    /// `crash_found`：以 `path` 为权威字段（M15 自己读盘）。
    pub fn crash_found(&mut self, source: &str, path: &Path, excerpt: Option<&str>) -> Result<()> {
        self.crash_found = true;
        let payload = payload::crash_found(source, path, excerpt, &self.redactor);
        self.record(EventKind::CrashFound, payload)
    }

    /// `diagnosis`：M15 产出的结构化结论，由 launch 脱敏后落盘（单一真相源）。
    ///
    /// M15 会按 `crash_found.path` 自己读盘再组 payload，崩溃报告/`latest.log` 里的
    /// 用户主目录路径与 token 会由此带进结论，所以这里必须再过一遍 [`Redactor`]
    /// （只改字符串，字段名/数值/布尔不动——`launch-接口约定.md` §9.3）。
    pub fn diagnosis(&mut self, record: Value) -> Result<()> {
        let record = self.redactor.scrub_json(&record);
        if self.diagnosis_top.is_none() {
            self.diagnosis_top = record
                .get("candidates")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        self.record(EventKind::Diagnosis, record)
    }

    /// `session_end` + 追加索引，返回索引条目。
    pub fn finish(&mut self, reason: EndReason) -> Result<SessionIndexEntry> {
        let duration_ms = self.clock.elapsed_ms();
        let payload = payload::session_end(reason.as_str(), duration_ms);
        self.record(EventKind::SessionEnd, payload)?;
        self.finished = true;
        let entry = SessionIndexEntry {
            v: SCHEMA_VERSION,
            session_id: self.session_id.clone(),
            instance_id: self.meta.instance_id.clone(),
            instance_name: self.meta.instance_name.clone(),
            game_version: self.meta.game_version.clone(),
            started_at: self.started_at.clone(),
            ended_at: self.clock.now_iso(),
            duration_ms,
            exit_code: self.exit_code,
            exit_type: self.exit_type.clone(),
            silent_failure: self.silent_failure,
            window_detected: self.window_detected,
            event_count: self.event_count,
            log_path: format!("game/{}.jsonl", self.session_id),
            crash_found: self.crash_found,
            diagnosis_top: self.diagnosis_top.clone(),
        };
        session_index::append(&self.index_path, &entry).map_err(|source| LaunchError::Io {
            op: "append_session_index",
            path: self.index_path.clone(),
            source,
        })?;
        Ok(entry)
    }
}
