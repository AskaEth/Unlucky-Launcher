//! Forge / NeoForge 的 installer processors 段（M45：GUI 与 CLI 行为对齐）。
//!
//! 顺序与 CLI `ull install --loader forge` 完全一致（`crates/cli/src/commands/install/
//! loader_processors.rs`），只是把进度接到 GUI 的回流通道上：
//!
//! ```text
//! ProcessorPlan::build（纯计算：变量表 + 每步 java -cp + 依赖库清单）
//!   → download_libraries（M27 引擎；进度回流 HostTask::Install）
//!   → select_java（M33 解析链：托管 > JAVA_HOME > PATH > 扫盘）
//!   → run（串行 java -cp；每步一条面板文案 + 一行装日志）
//! ```
//!
//! 四条硬约束（与 CLI 同源，GUI 侧一条都不放宽）：
//!
//! 1. **processors 跑完才允许写版本 JSON**：版本清单存在 = 这版装完了，抢在 processors
//!    前写会留下启动必崩却被报成"已安装"的实例（顺序由 `install_loader` 保证）；
//! 2. **幂等**：版本 JSON 已在盘上 = 上一轮跑完了，整段跳过；
//! 3. **取消在阶段边界生效**：处理器阶段是原子段（串行外调 java，进程内没有取消钩子），
//!    进入前与退出后各查一次 `cancel`；
//! 4. **失败不静默**：结构化文案（带 `processor_*` 大类码）+ 装日志一行一条，由调用方
//!    再补 Toast（`HostTask::InstallDone` 的既有通路）。
//!
//! 执行后端走 [`Backend`]：真实实现 [`CoreBackend`] 直接转调 core，单测注入 fake，
//! 因此「计划怎么组、Java 找不到怎么说、依赖库下漏怎么报」都能在没有网络/没有 JDK
//! 的机器上断言。

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use unlucky_core::addon::loader::forge::InstallProfile;
use unlucky_core::addon::loader::processors::{
    self, ProcessorError, ProcessorPlan, ProcessorPlanInput, ProcessorRun, StepOutcome, StepSink,
    StepStatus,
};
use unlucky_core::addon::loader::LoaderKind as CoreLoaderKind;
use unlucky_core::download::blocking::block_on;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::arguments::Environment;
use unlucky_core::game::install::{InstallReport, PlanFile, ProgressSink};
use unlucky_core::game::java::{JavaRequirement, JavaRuntime};

use crate::dto::InstallStage;

use super::env::HostEnv;
use super::install::{CANCELLED, CONCURRENCY};
use super::install_journal::Journal;
use super::job::{HostTask, JobSender};
use super::meter::{sink_for, Meter};

/// 真实后端（无状态，可以放静态里复用）。
pub(super) const CORE: CoreBackend = CoreBackend;

/// processors 段需要的输入（从 `LoaderRelease` 摘出来，便于单测构造）。
pub(super) struct StageInput<'a> {
    /// 加载器种类（补库时 maven 基址的兜底来源）。
    pub kind: CoreLoaderKind,
    /// 目标原版版本。
    pub mc_version: &'a str,
    /// loader 版本 id（依赖库落点 + 临时目录名）。
    pub version_id: &'a str,
    /// installer 里读出来的 install profile（`requires_installer` 为真时必须有）。
    pub profile: Option<&'a InstallProfile>,
    /// installer jar 的磁盘路径。
    pub installer: Option<&'a Path>,
    /// 版本 JSON 已在盘上 = 上一轮跑完了，整段跳过（幂等）。
    pub installed: bool,
}

/// processors 段的执行后端（真实实现转调 core；单测注入 fake）。
pub(super) trait Backend {
    /// 下载 processor 依赖库（M27 引擎：镜像/健康度/退避/校验）。
    fn download_libraries(
        &self,
        engine: &DownloadEngine,
        files: &[PlanFile],
        sink: Option<ProgressSink>,
    ) -> InstallReport;
    /// 按 M33 的解析链挑一个能跑 installer 的 Java。
    fn select_java(&self, managed: &Path, requirement: &JavaRequirement) -> Option<JavaRuntime>;
    /// 串行跑 processors。
    fn run(
        &self,
        plan: &ProcessorPlan,
        java: &JavaRuntime,
        sink: Option<StepSink>,
    ) -> Result<ProcessorRun, ProcessorError>;
}

/// 真实后端：三件事全部转调 core 的同一个执行器（CLI 用的就是它）。
pub(super) struct CoreBackend;

impl Backend for CoreBackend {
    fn download_libraries(
        &self,
        engine: &DownloadEngine,
        files: &[PlanFile],
        sink: Option<ProgressSink>,
    ) -> InstallReport {
        block_on(processors::download_libraries(
            engine,
            files,
            CONCURRENCY,
            sink,
        ))
    }

    fn select_java(&self, managed: &Path, requirement: &JavaRequirement) -> Option<JavaRuntime> {
        processors::select_java(managed, requirement)
    }

    fn run(
        &self,
        plan: &ProcessorPlan,
        java: &JavaRuntime,
        sink: Option<StepSink>,
    ) -> Result<ProcessorRun, ProcessorError> {
        processors::run(plan, java, sink)
    }
}

/// 段内的进度出口：进度面板（`HostTask::Install`）+ 装日志。
pub(super) struct Reporter<'a> {
    /// 后台线程 → host 的通道。
    pub tx: &'a JobSender,
    /// 进度记账器。
    pub meter: &'a Arc<Mutex<Meter>>,
    /// 装日志。
    pub journal: &'a Journal,
}

impl Reporter<'_> {
    /// 换阶段并推一条进度。
    fn stage(&self, stage: InstallStage, message: &str) {
        let Ok(mut guard) = self.meter.lock() else {
            return;
        };
        guard.stage(stage, message);
        let _ = self
            .tx
            .send(HostTask::Install(Box::new(guard.progress.clone())));
    }

    /// 先把待下的库记账（文件数 + 已知字节），下载回调再逐个累加。
    fn expect(&self, files: usize, bytes: u64) {
        let Ok(mut guard) = self.meter.lock() else {
            return;
        };
        let files = u64::try_from(files).unwrap_or(u64::MAX);
        guard.progress.files_total = guard.progress.files_total.saturating_add(files);
        guard.progress.bytes_total = guard.progress.bytes_total.saturating_add(bytes);
    }

    /// 失败留痕（文案由调用方决定怎么呈现；这里只保证日志里有一条）。
    fn fail(&self, reason: String) -> String {
        self.journal.line(&format!("失败：{reason}"));
        reason
    }
}

/// 跑 Forge / NeoForge 的 installer processors（`Ok(None)` = 这一轮不需要跑）。
///
/// # Errors
///
/// 元数据缺 profile/installer、依赖库没下全、找不到可用 Java、processor 非零退出、
/// 产物没落盘、用户取消时返回给 UI 的文案（已写进装日志）。
pub(super) fn stage(
    env: &HostEnv,
    engine: &DownloadEngine,
    input: &StageInput<'_>,
    backend: &dyn Backend,
    reporter: &Reporter<'_>,
    cancel: &Arc<AtomicBool>,
) -> Result<Option<ProcessorRun>, String> {
    if input.installed {
        reporter.journal.line(&format!(
            "{} 的版本 JSON 已在盘上，跳过 installer processors",
            input.version_id
        ));
        return Ok(None);
    }
    let (Some(profile), Some(installer)) = (input.profile, input.installer) else {
        return Err(reporter.fail(metadata_missing(input)));
    };
    reporter.stage(InstallStage::Installing, "正在组 installer 处理器计划…");
    let scratch = env
        .layout
        .cache()
        .join("processors")
        .join(safe_name(input.version_id));
    let plan = ProcessorPlan::build(ProcessorPlanInput {
        kind: input.kind,
        mc_version: input.mc_version,
        version_id: input.version_id,
        profile,
        layout: &env.game,
        installer,
        scratch: &scratch,
        environment: &Environment::host(),
    })
    .map_err(|error| reporter.fail(diagnostic(&error)))?;
    for warning in &plan.warnings {
        reporter.journal.line(&format!("规划告警：{warning}"));
    }
    if plan.is_empty() {
        reporter
            .journal
            .line("install profile 没有 client 侧 processor，跳过这一段");
        return Ok(None);
    }

    let total = plan.step_count();
    let bytes = plan.libraries.iter().filter_map(|file| file.size).sum();
    reporter.expect(plan.libraries.len(), bytes);
    reporter.stage(
        InstallStage::Downloading,
        &format!("下载 installer 依赖库（{} 个文件）…", plan.libraries.len()),
    );
    if cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED.to_string());
    }
    let sink = sink_for(reporter.tx.clone(), Arc::clone(reporter.meter));
    let libraries = backend.download_libraries(engine, &plan.libraries, Some(sink));
    if !libraries.is_complete() {
        return Err(reporter.fail(libraries_failed(&libraries)));
    }

    let managed = env.layout.java_managed();
    let Some(java) = backend.select_java(&managed, &plan.java_requirement) else {
        return Err(reporter.fail(java_missing(&plan.java_requirement, &managed)));
    };
    reporter.journal.line(&format!(
        "用 {}（{}）跑 {total} 步 processor",
        java.binary.display(),
        java_label(&java)
    ));
    reporter.stage(
        InstallStage::Installing,
        &format!("运行 installer 处理器 0/{total}（{}）…", java_label(&java)),
    );
    if cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED.to_string());
    }

    let steps = step_sink(
        reporter.tx.clone(),
        Arc::clone(reporter.meter),
        reporter.journal.clone(),
        total,
    );
    let run = backend
        .run(&plan, &java, Some(steps))
        .map_err(|error| reporter.fail(diagnostic(&error)))?;
    for warning in &run.warnings {
        reporter.journal.line(&format!("执行告警：{warning}"));
    }
    reporter.journal.line(&format!(
        "processors 完成：执行 {} 步、跳过 {} 步",
        run.executed(),
        run.skipped()
    ));
    reporter.stage(
        InstallStage::Installing,
        &format!(
            "installer 处理器完成（执行 {} 步、跳过 {} 步），正在写入版本清单…",
            run.executed(),
            run.skipped()
        ),
    );
    if cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED.to_string());
    }
    Ok(Some(run))
}

/// 每步完成的回调：面板一行 + 装日志一行。
///
/// `StepSink` 要求 `'static`，所以这里全持所有权（通道/记账器/日志都是克隆）。
fn step_sink(tx: JobSender, meter: Arc<Mutex<Meter>>, journal: Journal, total: usize) -> StepSink {
    Arc::new(move |outcome: &StepOutcome| {
        let line = step_line(outcome, total);
        journal.line(&line);
        let Ok(mut guard) = meter.lock() else {
            return;
        };
        guard.stage(InstallStage::Installing, &line);
        let _ = tx.send(HostTask::Install(Box::new(guard.progress.clone())));
    })
}

/// 一步的进度/日志文案（纯函数，单测直接断言）。
pub(super) fn step_line(outcome: &StepOutcome, total: usize) -> String {
    let suffix = match outcome.status {
        StepStatus::Skipped => "（产物已就绪，跳过）".to_string(),
        StepStatus::Executed => format!("（{} ms）", outcome.elapsed_ms),
    };
    format!(
        "installer 处理器 {}/{}：{}{suffix}",
        outcome.index, total, outcome.label
    )
}

/// processor 错误 → 用户可读文案（保留 `processor_*` 大类码，便于按码搜日志）。
pub(super) fn diagnostic(error: &ProcessorError) -> String {
    let mut text = format!("{}（{}）", error.message(), error.kind().as_str());
    if let Some(hint) = error.hint() {
        text.push_str("；建议：");
        text.push_str(hint);
    }
    text
}

/// 缺 Java 运行时的专用文案（比通用错误多给一条「去哪装」）。
pub(super) fn java_missing(requirement: &JavaRequirement, managed: &Path) -> String {
    format!(
        "跑 installer 需要 {}，但一个可用的 Java 运行时都没找到；请到「Java」页安装托管 JDK，或把 JAVA_HOME 指向可用的 JDK（托管目录 {}）",
        requirement.describe(),
        managed.display()
    )
}

/// 依赖库没下全 → 列前三条失败明细。
pub(super) fn libraries_failed(report: &InstallReport) -> String {
    let details: Vec<String> = report
        .failures(3)
        .iter()
        .filter_map(|outcome| {
            outcome
                .failure
                .as_ref()
                .map(|failure| format!("{}：{}", outcome.name, failure.message))
        })
        .collect();
    format!(
        "installer 依赖库没下全（{} 个失败，共 {} 个）：{}",
        report.failed(),
        report.files.len(),
        details.join("；")
    )
}

/// 需要 installer 但元数据不全（缓存半截 / 换线路重解析过）。
fn metadata_missing(input: &StageInput<'_>) -> String {
    format!(
        "{} {} 需要跑 installer processors，但没拿到 install profile / installer 路径；重跑一次即可（元数据缓存可能不完整）",
        input.kind.label(),
        input.version_id
    )
}

/// Java 的一行短标识（`Java 21 · managed`）。
fn java_label(java: &JavaRuntime) -> String {
    match java.major {
        Some(major) => format!("Java {major} · {}", java.origin.as_str()),
        None => format!("Java 主版本未知 · {}", java.origin.as_str()),
    }
}

/// 版本 id → 可做目录名的片段（`+` 与路径分隔符都换掉，与 CLI 同规则）。
fn safe_name(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_end_matches('.')
        .to_string()
}

#[cfg(test)]
#[path = "install_processors_fixture.rs"]
mod fixture;

#[cfg(test)]
#[path = "install_processors_tests.rs"]
mod tests;
