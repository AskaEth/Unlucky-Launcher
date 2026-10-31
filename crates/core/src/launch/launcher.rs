//! 顶层启动管线：组装 → 会话 → 进程 → 退出分析 → 诊断 → 索引。
//!
//! 对应 `DevDocs/PlanDoc/06-launch-and-diagnostics.md` §1 的 10 阶段：
//! 1~2（快照/账户）由调用方与 `adapter` 完成，3（Java 选择）由 D13 给出 `JavaSelection`，
//! 4（组件就绪检查）由 download/game 给出 `missing_components`，6（环境准备的 natives 段）
//! 由 [`crate::launch::natives`] 在本模块收口，5、7~10 见下。
//!
//! 两个入口：
//! - [`Launcher::dry_run`]（D18 接线点）：只组装、不 spawn、不写会话文件，返回完整命令行；
//! - [`Launcher::launch`]：完整跑一局，返回 [`SessionHandle`]（退出判定 + 诊断结论 + 索引条目）。
//!
//! 阶段 B 说明：diagnose（M15）尚未合并，诊断走 [`DiagnosisHook`] 接缝，默认 [`NoopDiagnosis`]。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::launch::args_table::{assemble, AssembledCommand};
use crate::launch::diagnosis_hook::{DiagnosisHook, NoopDiagnosis};
use crate::launch::error::{AssemblyError, Result};
use crate::launch::launch_session::{LaunchSession, SessionHandle};
use crate::launch::natives::{self, NativesReport};
use crate::launch::platform::PlatformCtx;
use crate::launch::process::{ProcessConfig, RunningProcess};
use crate::launch::redact::Redactor;
use crate::launch::request::LaunchRequest;
use crate::launch::session::{EndReason, SessionMeta, SessionSink, SessionWriter};

/// 启动器配置。
#[derive(Debug, Clone)]
pub struct LauncherConfig {
    /// 会话文件根目录（`<data_root>/agent/sessions`）。
    pub sessions_root: PathBuf,
    /// 平台上下文（命令行分支 + 退出码语义）。
    pub platform: PlatformCtx,
    /// 进程管理配置（看门狗/宽限期/环形缓冲）。
    pub process: ProcessConfig,
    /// 启动器名（`${launcher_name}`，请求里非空时以请求为准）。
    pub launcher_name: String,
    /// 启动器版本。
    pub launcher_version: String,
    /// 用户主目录（脱敏用）。
    pub home_dir: Option<PathBuf>,
    /// 结束后是否扫描崩溃现场文件。
    pub scan_crash_reports: bool,
    /// 现场文件最多收几个。
    pub max_crash_artifacts: usize,
}

impl LauncherConfig {
    /// 以会话根目录构造（其余取默认）。
    pub fn new(sessions_root: impl Into<PathBuf>) -> Self {
        let platform = PlatformCtx::current();
        Self {
            sessions_root: sessions_root.into(),
            home_dir: platform.home_dir.clone(),
            platform,
            process: ProcessConfig::default(),
            launcher_name: "UnluckyLauncher".to_string(),
            launcher_version: "0.1.0".to_string(),
            scan_crash_reports: true,
            max_crash_artifacts: 4,
        }
    }

    /// 补启动器名/版本。
    pub fn with_launcher(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.launcher_name = name.into();
        self.launcher_version = version.into();
        self
    }
}

/// `--dry-run` 的输出（D18）：完整命令行，但**不 spawn、不写会话文件**。
///
/// 会做的事与真实启动**一致到「环境准备」为止**：natives 解压照做（幂等，`natives` 字段
/// 回报结果），因为「装完能不能起」的一半答案就在「native 库有没有落地」上。
#[derive(Debug, Clone)]
pub struct DryRun {
    /// 实际要执行的程序。
    pub program: PathBuf,
    /// 参数（已脱敏：accessToken 一定是 `<redacted:token>`）。
    pub argv: Vec<String>,
    /// 人读单行（可直接 `println!`，含空格的参数带引号）。
    pub display: String,
    /// 组装告警。
    pub warnings: Vec<String>,
    /// classpath 条目数。
    pub classpath_entries: usize,
    /// 日志解码编码。
    pub encoding: String,
    /// natives 解压结果（没有 natives 目录、即请求未配置时为 `None`）。
    pub natives: Option<NativesReport>,
}

impl DryRun {
    /// 一行命令行（D18 打印用）。
    pub fn line(&self) -> &str {
        &self.display
    }
}

/// 启动器。
#[derive(Clone)]
pub struct Launcher {
    config: LauncherConfig,
    hook: Arc<dyn DiagnosisHook>,
}

impl std::fmt::Debug for Launcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Launcher")
            .field("config", &self.config)
            .field("hook", &"<diagnosis hook>")
            .finish()
    }
}

impl Launcher {
    /// 构造（无诊断挂钩）。
    pub fn new(config: LauncherConfig) -> Self {
        Self {
            config,
            hook: Arc::new(NoopDiagnosis),
        }
    }

    /// 接上诊断挂钩（M15 适配器）。
    pub fn with_diagnosis_hook(mut self, hook: Arc<dyn DiagnosisHook>) -> Self {
        self.hook = hook;
        self
    }

    /// 配置。
    pub fn config(&self) -> &LauncherConfig {
        &self.config
    }

    /// 只组装命令行（纯计算，可单测/可透传）。
    pub fn plan(&self, request: &LaunchRequest) -> Result<AssembledCommand> {
        assemble(request, &self.config.platform)
    }

    /// 环境准备（管线第 6 段）：natives 解压。
    ///
    /// 目标目录取 `request.dirs.natives_dir`（与 `${natives_directory}` 同一真相源）；
    /// 目录为空视为调用方不需要，返回 `Ok(None)`。
    fn prepare_environment(&self, request: &LaunchRequest) -> Result<Option<NativesReport>> {
        if request.dirs.natives_dir.as_os_str().is_empty() {
            return Ok(None);
        }
        natives::extract(&request.dirs.natives_dir, &request.natives)
            .map(Some)
            .map_err(|err| AssemblyError::NativesExtraction {
                message: err.message(),
            })
            .map_err(Into::into)
    }

    /// D18：组装完整命令行并返回，**不 spawn 进程、不写会话文件**。
    ///
    /// 供 `unlucky-launcher --dry-run <instance>` 打印一行命令行后退出；
    /// accessToken 已脱敏（绝不把凭据打到终端/日志里）。
    pub fn dry_run(&self, request: &LaunchRequest) -> Result<DryRun> {
        let natives = self.prepare_environment(request)?;
        let command = self.plan(request)?;
        let redactor = self.redactor(request);
        let warnings = command
            .warnings
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        Ok(DryRun {
            program: command.program.clone(),
            argv: redactor.scrub_all(&command.argv()),
            display: redactor.scrub(&command.display()).into_owned(),
            warnings,
            classpath_entries: command.classpath.len(),
            encoding: command.encoding.clone(),
            natives,
        })
    }

    /// 启动一局并返回会话（可中途 [`LaunchSession::stop`]）。
    pub fn spawn(&self, request: &LaunchRequest) -> Result<LaunchSession> {
        let redactor = self.redactor(request);
        let meta = self.session_meta(request);
        let mut writer = SessionWriter::create(&self.config.sessions_root, meta, redactor.clone())?;
        let session_id = writer.session_id().to_string();
        let log_path = writer.path().to_path_buf();
        writer.session_start()?;

        // 阶段 6「环境准备」：natives 必须落到 java.library.path 指向的目录，且在组装前完成
        // （HMCL 同序），失败即启动前失败——native 库缺了游戏只会给 UnsatisfiedLinkError。
        let natives = match self.prepare_environment(request) {
            Ok(report) => report,
            Err(err) => {
                let _ = writer.assembly_failed(&launch_error_assembly(&err));
                let _ = writer.finish(EndReason::AssemblyFailed);
                return Err(err);
            }
        };
        if let Some(report) = &natives {
            writer.natives_extracted(report)?;
        }

        let command = match self.plan(request) {
            Ok(command) => command,
            Err(err) => {
                // 启动前失败也要留下证据：assembly_failed + session_end + 索引。
                let _ = writer.assembly_failed(&launch_error_assembly(&err));
                let _ = writer.finish(EndReason::AssemblyFailed);
                return Err(err);
            }
        };
        writer.cmd_built(&command)?;

        let sink = Arc::new(SessionSink::with_start(writer, Instant::now()));
        let working_dir = if request.dirs.game_dir.as_os_str().is_empty() {
            None
        } else {
            Some(request.dirs.game_dir.as_path())
        };
        let process =
            match RunningProcess::spawn(&command, working_dir, sink.clone(), self.config.process) {
                Ok(process) => process,
                Err(err) => {
                    let _ = sink.with_writer(|writer| writer.finish(EndReason::SpawnFailed));
                    return Err(err);
                }
            };
        let pid = process.pid();
        if let Some(result) = sink.with_writer(|writer| writer.proc_start(pid)) {
            result?;
        }
        Ok(LaunchSession::new(
            self.config.clone(),
            self.hook.clone(),
            redactor,
            request.clone(),
            session_id,
            log_path,
            command,
            pid,
            process,
            sink,
        ))
    }

    /// 启动并阻塞到进程结束（P0 主入口）。
    pub fn launch(&self, request: &LaunchRequest) -> Result<SessionHandle> {
        self.spawn(request)?.finish()
    }

    fn redactor(&self, request: &LaunchRequest) -> Redactor {
        let mut redactor = Redactor::new().with_secret(request.auth.access_token.clone());
        if let Some(home) = &self.config.home_dir {
            redactor = redactor.with_home(home);
        }
        if !request.auth.uuid.is_empty() {
            redactor = redactor.with_secret(request.auth.uuid.clone());
        }
        redactor
    }

    fn session_meta(&self, request: &LaunchRequest) -> SessionMeta {
        let launcher_name = if request.launcher.name.is_empty() {
            self.config.launcher_name.clone()
        } else {
            request.launcher.name.clone()
        };
        let launcher_version = if request.launcher.version.is_empty() {
            self.config.launcher_version.clone()
        } else {
            request.launcher.version.clone()
        };
        SessionMeta::new(
            request.instance.id.clone(),
            request.instance.name.clone(),
            request.instance.version.raw().to_string(),
        )
        .with_launcher(launcher_name, launcher_version)
        .with_platform(&self.config.platform)
    }
}

/// 把错误拆成 `assembly_failed` 事件需要的结构化原因。
pub(crate) fn launch_error_assembly(
    error: &crate::launch::error::LaunchError,
) -> crate::launch::error::AssemblyError {
    match error {
        crate::launch::error::LaunchError::Assembly(assembly) => assembly.clone(),
        other => crate::launch::error::AssemblyError::MalformedArgument {
            entry: other.to_string(),
            reason: "非组装类失败",
        },
    }
}
