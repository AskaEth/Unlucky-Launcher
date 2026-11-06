//! installer processors 段的单测夹具（拆文件守 400 行上限）。
//!
//! 不需要网络、不需要 JDK、不真的起 java：`Harness` 给一个临时数据根 + 进度通道，
//! `Fake` 回放预置的下载/Java/执行结果并记录被调顺序与组出来的命令行。

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use unlucky_core::addon::loader::forge::{InstallProfile, Processor};
use unlucky_core::addon::loader::processors::{
    ProcessorError, ProcessorPlan, ProcessorRun, StepOutcome, StepSink, StepStatus,
};
use unlucky_core::addon::loader::LoaderKind as CoreLoaderKind;
use unlucky_core::download::{DownloadEngine, DownloadErrorKind};
use unlucky_core::game::install::{
    FileFailure, FileOutcome, FileStatus, InstallReport, PlanFile, ProgressSink,
};
use unlucky_core::game::java::{JavaOrigin, JavaRequirement, JavaRuntime};

use super::{Backend, Reporter, StageInput};
use crate::dto::{CreateRequest, LoaderKind};
use crate::host::env::HostEnv;
use crate::host::install_journal::Journal;
use crate::host::job::{channel, HostTask, JobSender};
use crate::host::meter::Meter;

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// 一步的计划视图（序号 / 坐标 / 参数 / classpath 条数）。
pub(super) type StepView = (usize, String, Vec<String>, usize);

/// 一段 cases 的公共环境：临时数据根 + 引擎 + 进度通道。
pub(super) struct Harness {
    dir: PathBuf,
    pub(super) env: HostEnv,
    pub(super) engine: DownloadEngine,
    tx: JobSender,
    rx: Receiver<HostTask>,
    pub(super) meter: Arc<Mutex<Meter>>,
    pub(super) journal: Journal,
    pub(super) cancel: Arc<AtomicBool>,
}

impl Harness {
    pub(super) fn new() -> Self {
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("ull-proc-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let env = HostEnv::at_root(&dir);
        let engine = crate::host::engine::engine(&env);
        let request = CreateRequest {
            version: "1.20.1".to_string(),
            loader: LoaderKind::Forge,
            loader_version: "47.2.0".to_string(),
            name: "proc".to_string(),
        };
        let (tx, rx) = channel();
        Self {
            journal: Journal::open(&env),
            meter: Arc::new(Mutex::new(Meter::new(&request, 1))),
            cancel: Arc::new(AtomicBool::new(false)),
            dir,
            env,
            engine,
            tx,
            rx,
        }
    }

    pub(super) fn reporter(&self) -> Reporter<'_> {
        Reporter {
            tx: &self.tx,
            meter: &self.meter,
            journal: &self.journal,
        }
    }

    /// 进度面板收到的所有文案。
    pub(super) fn messages(&self) -> Vec<String> {
        self.rx
            .try_iter()
            .filter_map(|task| match task {
                HostTask::Install { progress, .. } => Some(progress.message.clone()),
                _ => None,
            })
            .collect()
    }

    /// 装日志全文。
    pub(super) fn log(&self) -> String {
        std::fs::read_to_string(self.journal.path()).unwrap_or_default()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// 注入用的假后端：回放预置结果 + 记录调用顺序与被组出来的命令行。
pub(super) struct Fake {
    pub(super) libraries_ok: bool,
    pub(super) java: Option<JavaRuntime>,
    pub(super) run_error: Option<ProcessorError>,
    calls: RefCell<Vec<&'static str>>,
    steps: RefCell<Vec<StepView>>,
}

impl Fake {
    pub(super) fn new() -> Self {
        Self {
            libraries_ok: true,
            java: Some(java()),
            run_error: None,
            calls: RefCell::new(Vec::new()),
            steps: RefCell::new(Vec::new()),
        }
    }

    pub(super) fn calls(&self) -> Vec<&'static str> {
        self.calls.borrow().clone()
    }

    /// 被组出来的每步（序号 / 坐标 / 参数 / classpath 条数）。
    pub(super) fn steps(&self) -> Vec<StepView> {
        self.steps.borrow().clone()
    }
}

impl Backend for Fake {
    fn download_libraries(
        &self,
        _engine: &DownloadEngine,
        files: &[PlanFile],
        _sink: Option<ProgressSink>,
    ) -> InstallReport {
        self.calls.borrow_mut().push("libraries");
        let ok = self.libraries_ok;
        let outcomes = files
            .iter()
            .map(|file| FileOutcome {
                role: file.role,
                name: file.name.clone(),
                dest: file.dest.clone(),
                status: if ok {
                    FileStatus::Skipped
                } else {
                    FileStatus::Failed
                },
                bytes: 0,
                source: None,
                failure: (!ok).then(|| FileFailure {
                    kind: DownloadErrorKind::Network,
                    message: "连接超时".to_string(),
                }),
            })
            .collect();
        InstallReport {
            files: outcomes,
            version_json: None,
            warnings: Vec::new(),
        }
    }

    fn select_java(&self, _managed: &Path, _requirement: &JavaRequirement) -> Option<JavaRuntime> {
        self.calls.borrow_mut().push("java");
        self.java.clone()
    }

    fn run(
        &self,
        plan: &ProcessorPlan,
        java: &JavaRuntime,
        sink: Option<StepSink>,
    ) -> Result<ProcessorRun, ProcessorError> {
        self.calls.borrow_mut().push("run");
        *self.steps.borrow_mut() = plan
            .steps
            .iter()
            .map(|step| {
                (
                    step.index,
                    step.label.clone(),
                    step.args.clone(),
                    step.classpath.len(),
                )
            })
            .collect();
        if let Some(error) = &self.run_error {
            return Err(error.clone());
        }
        let outcomes: Vec<StepOutcome> = plan
            .steps
            .iter()
            .map(|step| StepOutcome {
                index: step.index,
                label: step.label.clone(),
                status: StepStatus::Executed,
                outputs: step.outputs.len(),
                elapsed_ms: 12,
                stdout_tail: None,
                stderr_tail: None,
            })
            .collect();
        if let Some(sink) = &sink {
            for outcome in &outcomes {
                sink(outcome);
            }
        }
        Ok(ProcessorRun {
            java: java.clone(),
            steps: outcomes,
            warnings: Vec::new(),
        })
    }
}

/// 一个主版本已知的假 Java。
pub(super) fn java() -> JavaRuntime {
    JavaRuntime {
        binary: PathBuf::from("java"),
        origin: JavaOrigin::Managed,
        version: Some("21.0.4".to_string()),
        major: Some(21),
        bits: 64,
        vendor: Some("temurin".to_string()),
    }
}

/// 一个最小可用的 install profile：一个 client 侧 processor、两个字面量参数。
pub(super) fn profile() -> InstallProfile {
    let mut profile = InstallProfile::default();
    profile.processors.push(Processor {
        sides: vec!["client".to_string()],
        jar: Some("net.minecraftforge:installertools:1.3.0".to_string()),
        classpath: vec!["net.minecraftforge:jarsplitter:1.1.4".to_string()],
        args: vec!["--task".to_string(), "'MCP_DATA'".to_string()],
        ..Processor::default()
    });
    profile
}

pub(super) fn input<'a>(profile: &'a InstallProfile, installer: &'a Path) -> StageInput<'a> {
    StageInput {
        kind: CoreLoaderKind::Forge,
        mc_version: "1.20.1",
        version_id: "1.20.1-forge-47.2.0",
        profile: Some(profile),
        installer: Some(installer),
        installed: false,
    }
}

pub(super) fn installer() -> PathBuf {
    PathBuf::from("cache/meta/forge-installer.jar")
}

pub(super) fn outcome(status: StepStatus) -> StepOutcome {
    StepOutcome {
        index: 3,
        label: "net.minecraftforge:installertools:1.3.0".to_string(),
        status,
        outputs: 2,
        elapsed_ms: 1234,
        stdout_tail: None,
        stderr_tail: None,
    }
}
