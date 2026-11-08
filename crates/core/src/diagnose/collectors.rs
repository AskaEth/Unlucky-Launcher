//! 五类信号采集接口：退出码 / 日志环形缓冲 / crash-reports / hs_err / 环境快照（+ mod 清单）。
//!
//! 分工（与 `launch-接口约定.md` 对齐）：
//! - **推送类**（[`ExitCollector`] / [`EnvironmentCollector`]）：launch 已经算出事实，直接塞进 [`SignalBundle`]；
//! - **磁盘类**（[`CrashReportCollector`] / [`HsErrCollector`] / [`LatestLogCollector`]）：本模块自己按
//!   mtime 取最新一份，并可用 [`CollectionContext::started_at`] 过滤历史崩溃；
//! - 日志行由 launch 逐行喂 [`LogBuffer::push`]，diagnose 不解析会话 JSONL（两边零类型耦合）。
//!
//! 采集是**尽力而为**：[`collect_all`] 把单个来源的失败收集成 [`CollectError`] 而不是中断整体——
//! 崩溃现场本来就残缺，少一个来源不该失去全部结论。

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[path = "environment.rs"]
pub mod environment;
#[path = "signals.rs"]
pub mod signals;
#[path = "sources.rs"]
pub mod sources;

pub use environment::{ComponentVersion, EnvironmentSnapshot, ModRecord};
pub use signals::{
    ExitSignal, ExitType, LogBuffer, LogLine, SignalOrigin, CRASH_KEYWORDS, EXIT_ACCESS_VIOLATION,
    EXIT_DLL_NOT_FOUND, EXIT_SIGKILL, EXIT_SIGTERM, EXIT_STACK_BUFFER_OVERRUN, JVM_ERROR_KEYWORDS,
};
pub use sources::{
    CrashReportCollector, EnvironmentCollector, ExitCollector, HsErrCollector, LatestLogCollector,
};

use super::engine::DiagnosisInput;
use super::hs_err::HsErrReport;

/// 信号种类（可观测性 / 采集计划编排用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SignalKind {
    /// 退出码 + 退出类型。
    Exit,
    /// stdout/stderr 环形缓冲。
    Log,
    /// `crash-reports/crash-*.txt`。
    CrashReport,
    /// `hs_err_pid*.log`。
    HsErr,
    /// 环境快照。
    Environment,
    /// mod 清单。
    Mods,
}

impl SignalKind {
    /// 稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exit => "exit",
            Self::Log => "log",
            Self::CrashReport => "crash_report",
            Self::HsErr => "hs_err",
            Self::Environment => "environment",
            Self::Mods => "mods",
        }
    }
}

/// 采集上下文。
#[derive(Debug, Clone, Default)]
pub struct CollectionContext {
    /// 游戏工作目录（`crash-reports/`、`hs_err_pid*.log`、`logs/latest.log` 都在它下面）。
    pub run_dir: PathBuf,
    /// 进程启动时刻：只采集修改时间 ≥ 它的文件，避免把上次崩溃算进本次。
    pub started_at: Option<SystemTime>,
}

impl CollectionContext {
    /// 以工作目录构造。
    #[must_use]
    pub fn new(run_dir: impl Into<PathBuf>) -> Self {
        Self {
            run_dir: run_dir.into(),
            started_at: None,
        }
    }

    /// 带「只认本次会话文件」的时间下界。
    #[must_use]
    pub fn since(mut self, started_at: SystemTime) -> Self {
        self.started_at = Some(started_at);
        self
    }
}

/// 采集失败（尽力而为：单源失败不影响其它来源）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectError {
    /// 出错的信号种类。
    pub kind: SignalKind,
    /// 相关路径（可能为空，例如纯逻辑错误）。
    pub path: Option<PathBuf>,
    /// 人类可读原因。
    pub message: String,
}

impl CollectError {
    fn io(kind: SignalKind, path: &Path, error: &io::Error) -> Self {
        Self {
            kind,
            path: Some(path.to_path_buf()),
            message: error.to_string(),
        }
    }
}

impl fmt::Display for CollectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self
            .path
            .as_deref()
            .map_or_else(String::new, |path| format!("（{}）", path.display()));
        write!(f, "{} 采集失败{path}：{}", self.kind.as_str(), self.message)
    }
}

impl std::error::Error for CollectError {}

/// 采到的源文件列表（诊断包导出 / 可观测性）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectedSources {
    /// 文件路径。
    pub paths: Vec<PathBuf>,
}

impl CollectedSources {
    /// 空。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 记一个源文件。
    pub fn push(&mut self, path: impl Into<PathBuf>) {
        self.paths.push(path.into());
    }

    /// 是否什么都没采到。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

/// 一个来源的采到的文本（含来源文件与时间）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedText {
    /// 来源文件。
    pub path: PathBuf,
    /// 文本。
    pub text: String,
    /// 修改时间。
    pub modified: Option<SystemTime>,
}

/// 采集到的全部信号。
#[derive(Debug, Clone, Default)]
pub struct SignalBundle {
    /// 退出信号。
    pub exit: Option<ExitSignal>,
    /// 日志环形缓冲（launch 逐行喂；`latest.log` 兜底也进这里）。
    pub log: LogBuffer,
    /// 崩溃报告全文。
    pub crash_report: Option<CapturedText>,
    /// hs_err 解析结果。
    pub hs_err: Option<HsErrReport>,
    /// 环境快照。
    pub environment: Option<EnvironmentSnapshot>,
    /// mod 清单。
    pub mods: Vec<ModRecord>,
    /// 本次采到的源文件。
    pub sources: Vec<PathBuf>,
}

impl SignalBundle {
    /// 空包（日志缓冲默认 2000 行）。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否没有任何信号。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.exit.is_none()
            && self.log.is_empty()
            && self.crash_report.is_none()
            && self.hs_err.is_none()
            && self.environment.is_none()
            && self.mods.is_empty()
    }

    /// 转成规则引擎的输入。
    ///
    /// 日志按来源分组成多份待匹配文本（stdout/stderr 分开），这样证据里能保留来源；
    /// crash-report 与 hs_err 各占一份，取证优先级由 `SignalOrigin::evidence_rank` 决定。
    #[must_use]
    pub fn diagnosis_input(&self) -> DiagnosisInput {
        let mut input = DiagnosisInput::new();
        input.exit = self.exit.clone();
        for (origin, text) in grouped_log_text(&self.log) {
            input.push_text(origin, text);
        }
        if let Some(crash_report) = &self.crash_report {
            input.push_text(SignalOrigin::CrashReport, crash_report.text.clone());
        }
        if let Some(report) = &self.hs_err {
            input.push_hs_err(report.clone());
        }
        input
    }
}

fn grouped_log_text(log: &LogBuffer) -> Vec<(SignalOrigin, String)> {
    let mut groups: Vec<(SignalOrigin, String)> = Vec::new();
    for line in log.lines() {
        match groups.iter_mut().find(|(origin, _)| *origin == line.origin) {
            Some((_, text)) => {
                text.push_str(&line.text);
                text.push('\n');
            }
            None => groups.push((line.origin, format!("{}\n", line.text))),
        }
    }
    groups
}

/// 信号采集器。
pub trait SignalCollector {
    /// 采哪种信号。
    fn kind(&self) -> SignalKind;

    /// 采集并写入 `out`；返回本次采到的源文件。
    ///
    /// # Errors
    ///
    /// 读盘失败 / 解析失败时返回 [`CollectError`]；调用方（[`collect_all`]）会收集错误继续。
    fn collect(
        &self,
        ctx: &CollectionContext,
        out: &mut SignalBundle,
    ) -> Result<CollectedSources, CollectError>;
}

/// 按顺序跑一批采集器，单源失败只记录不中断。
pub fn collect_all(
    collectors: &[&dyn SignalCollector],
    ctx: &CollectionContext,
) -> (SignalBundle, Vec<CollectError>) {
    let mut bundle = SignalBundle::new();
    let mut errors = Vec::new();
    for collector in collectors {
        if let Err(error) = collector.collect(ctx, &mut bundle) {
            errors.push(error);
        }
    }
    (bundle, errors)
}
