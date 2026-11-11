//! 逐个外调 java 执行 processor：`java -cp <依赖…>:<jar> <主类> <参数…>`。
//!
//! 依据 `DevDocs/analyse/05-modloader-modpack.md` §1.2（Forge 新式 installer 的 processors
//! 流水线）与 HMCL `ForgeNewInstallTask.ProcessorTask` 的实现，四条硬规矩：
//!
//! 1. **串行**：后面的 processor 吃前面的产物（拆包 → 映射 → 打补丁），并行必炸；
//! 2. **幂等**：每步先按 `outputs` 的 SHA-1 判定，全部命中就整步跳过（重跑 install 零成本）；
//! 3. **主类来自 jar 的 MANIFEST**：官方 install profile 不写主类（少数 fork 给 `mainClass`）；
//! 4. **失败要能定位**：非零退出带脱敏后的 stderr 尾部（路径/用户名不能泄漏到日志外）。
//!
//! # 关于产物 SHA-1 与 zlib 变体（HMCL 踩过的坑）
//!
//! processor 动态生成的 jar 是用**本机 JDK 的 zlib** 压缩的。zlib-ng 之类非标准实现会把
//! 同样的内容压出不同字节 → SHA-1 与 profile 里的期望值不符。HMCL 的处置是「哈希不符时
//! 退化成 ZIP 结构校验」。我们的处置更保守：**存在性强制、摘要不符只告警**，
//! 既不误杀真实装，也不假装校验通过。

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};

use super::error::ProcessorError;
use super::jar::read_main_class;
use super::plan::{ProcessorPlan, ProcessorStep};
use super::prepare::prepare;
use super::text::{host_redactor, non_empty, tail};
use crate::download::verify;
use crate::download::HashAlgo;
use crate::game::java::{self, JavaRequirement, JavaRuntime};
use crate::launch::redact::Redactor;
use crate::platform::child::quiet;

/// 一步的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    /// 产物已就绪（SHA-1 命中），整步跳过。
    Skipped,
    /// 真的起了一次 java。
    Executed,
}

impl StepStatus {
    /// 稳定字符串 id（`--json` 用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Executed => "executed",
        }
    }
}

/// 一步的执行结果。
#[derive(Debug, Clone)]
pub struct StepOutcome {
    /// 序号（从 1 起）。
    pub index: usize,
    /// jar 的 maven 坐标。
    pub label: String,
    /// 结局。
    pub status: StepStatus,
    /// 产物个数。
    pub outputs: usize,
    /// 耗时（毫秒；跳过时为 0）。
    pub elapsed_ms: u128,
    /// stdout 尾巴（已脱敏，可能为空）。
    pub stdout_tail: Option<String>,
    /// stderr 尾巴（已脱敏，可能为空）。
    pub stderr_tail: Option<String>,
}

impl StepOutcome {
    /// JSON 视图。
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "index": self.index,
            "jar": self.label,
            "status": self.status.as_str(),
            "outputs": self.outputs,
            "elapsed_ms": self.elapsed_ms,
            "stderr_tail": self.stderr_tail,
        })
    }
}

/// 一次 processors 执行的汇总。
#[derive(Debug, Clone)]
pub struct ProcessorRun {
    /// 实际用的 Java（含来源与主版本）。
    pub java: JavaRuntime,
    /// 逐步骤结果（顺序即执行顺序）。
    pub steps: Vec<StepOutcome>,
    /// 不致命的告警（摘要不符、没有 outputs 的步骤等）。
    pub warnings: Vec<String>,
}

impl ProcessorRun {
    /// 真正跑过的步数。
    #[must_use]
    pub fn executed(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| step.status == StepStatus::Executed)
            .count()
    }

    /// 跳过的步数（幂等命中）。
    #[must_use]
    pub fn skipped(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| step.status == StepStatus::Skipped)
            .count()
    }

    /// JSON 视图。
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "java": {
                "binary": self.java.binary.display().to_string(),
                "source": self.java.origin.as_str(),
                "major": self.java.major,
                "version": self.java.version,
            },
            "executed": self.executed(),
            "skipped": self.skipped(),
            "steps": self.steps.iter().map(StepOutcome::to_json).collect::<Vec<_>>(),
            "warnings": self.warnings,
        })
    }
}

/// 每步完成的回调（CLI 往 stderr 打实时进度用）。
pub type StepSink = Arc<dyn Fn(&StepOutcome) + Send + Sync>;

/// 按 M33 的解析链挑一个能跑 installer 的 Java：托管 > `JAVA_HOME` > `PATH` > 扫盘。
///
/// 与启动路径的差异只有一点：**扫盘只在前面都不满足时才做**（别为了一次安装白跑
/// N 次 `java -version`），这里复用 [`crate::game::java::pick`] 的同一套优先序。
#[must_use]
pub fn select_java(managed_root: &Path, requirement: &JavaRequirement) -> Option<JavaRuntime> {
    let mut candidates = java::discover_managed(managed_root);
    candidates.extend(java::system_candidates());
    if let Some(runtime) = java::pick(&candidates, requirement) {
        if usable(runtime) {
            return Some(runtime.clone());
        }
    }
    candidates.extend(java::standard_candidates());
    let deduped = java::discover::dedupe(candidates);
    java::pick(&deduped, requirement).cloned().filter(usable)
}

/// 候选是否真的能用：探测到主版本，或者那个文件确实在盘上。
fn usable(runtime: &JavaRuntime) -> bool {
    runtime.major.is_some() || runtime.binary.is_file()
}

/// 顺序执行所有 processor。
///
/// 全部产物已就绪时走快路径：不复制 client jar、不解包 installer、不起任何进程。
///
/// # Errors
///
/// 临时目录/解包/复制失败（IO）、依赖缺失、`java -cp` 起不来或非零退出、
/// 产物没落盘时返回 [`ProcessorError`]。
pub fn run(
    plan: &ProcessorPlan,
    runtime: &JavaRuntime,
    sink: Option<StepSink>,
) -> Result<ProcessorRun, ProcessorError> {
    let redactor = host_redactor();
    let mut warnings = Vec::new();
    let mut outcomes = Vec::with_capacity(plan.steps.len());
    let satisfied: Vec<bool> = plan
        .steps
        .iter()
        .map(|step| outputs_satisfied(step, &mut warnings))
        .collect();
    if satisfied.iter().any(|done| !done) {
        prepare(plan)?;
    }
    for (step, done) in plan.steps.iter().zip(satisfied) {
        let outcome = if done {
            StepOutcome {
                index: step.index,
                label: step.label.clone(),
                status: StepStatus::Skipped,
                outputs: step.outputs.len(),
                elapsed_ms: 0,
                stdout_tail: None,
                stderr_tail: None,
            }
        } else {
            run_step(plan, runtime, step, &redactor, &mut warnings)?
        };
        if let Some(sink) = &sink {
            sink(&outcome);
        }
        outcomes.push(outcome);
    }

    Ok(ProcessorRun {
        java: runtime.clone(),
        steps: outcomes,
        warnings,
    })
}

/// 起一次 java，跑完校验产物。
fn run_step(
    plan: &ProcessorPlan,
    runtime: &JavaRuntime,
    step: &ProcessorStep,
    redactor: &Redactor,
    warnings: &mut Vec<String>,
) -> Result<StepOutcome, ProcessorError> {
    ensure_dependencies(step)?;
    let main_class = match &step.main_class {
        Some(main_class) => main_class.clone(),
        None => read_main_class(&step.jar)?,
    };
    let classpath = step.classpath_join()?;

    let started = Instant::now();
    let output = quiet(&runtime.binary)
        .arg("-cp")
        .arg(&classpath)
        .arg(&main_class)
        .args(&step.args)
        .current_dir(&plan.scratch)
        .output()
        .map_err(|error| {
            ProcessorError::io(format!(
                "起 java 失败（{}）：{error}",
                runtime.binary.display()
            ))
            .with_hint(
                "确认 `ull java list` 里的路径可用，或用 `ull java install <major>` 装一个托管 JDK",
            )
        })?;
    let elapsed_ms = started.elapsed().as_millis();
    let stdout = redactor
        .scrub(&String::from_utf8_lossy(&output.stdout))
        .into_owned();
    let stderr = redactor
        .scrub(&String::from_utf8_lossy(&output.stderr))
        .into_owned();

    if !output.status.success() {
        let exit = output
            .status
            .code()
            .map_or_else(|| "signal".to_string(), |code| code.to_string());
        return Err(
            ProcessorError::failed(&step.label, &exit, &tail(&stderr)).with_hint(
                "installer 的 processor 是官方产物，失败多半是 Java 版本/磁盘权限/网络（部分步骤要联网取映射）",
            ),
        );
    }

    verify_outputs(step, warnings)?;
    Ok(StepOutcome {
        index: step.index,
        label: step.label.clone(),
        status: StepStatus::Executed,
        outputs: step.outputs.len(),
        elapsed_ms,
        stdout_tail: non_empty(tail(&stdout)),
        stderr_tail: non_empty(tail(&stderr)),
    })
}

/// 依赖 jar 是否都在盘上（缺了直接报错：起了 java 也只会得到 `NoClassDefFoundError`）。
fn ensure_dependencies(step: &ProcessorStep) -> Result<(), ProcessorError> {
    if !step.jar.is_file() {
        return Err(ProcessorError::library_missing(format!(
            "processor {} 的可执行 jar 不在盘上：{}",
            step.label,
            step.jar.display()
        )));
    }
    let missing: Vec<String> = step
        .classpath
        .iter()
        .filter(|path| !path.is_file())
        .map(|path| path.display().to_string())
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(ProcessorError::library_missing(format!(
        "processor {} 有 {} 个依赖 jar 不在盘上：{}",
        step.label,
        missing.len(),
        missing.join("；")
    ))
    .with_hint("重跑一次 `ull install` 会补齐依赖（下载走镜像/健康度引擎）"))
}

/// 产物存在性（硬）与摘要（软）校验。
fn verify_outputs(step: &ProcessorStep, warnings: &mut Vec<String>) -> Result<(), ProcessorError> {
    let missing: Vec<String> = step
        .outputs
        .iter()
        .filter(|output| !output.path.is_file())
        .map(|output| output.path.display().to_string())
        .collect();
    if !missing.is_empty() {
        return Err(ProcessorError::output_missing(format!(
            "processor {} 跑完了但产物没落盘：{}",
            step.label,
            missing.join("；")
        ))
        .with_hint("磁盘权限/杀软拦截是常见原因；也可以删掉 versions/ 下这版后重装"));
    }
    for output in &step.outputs {
        let Some(expected) = &output.sha1 else {
            continue;
        };
        let Ok(actual) = verify::digest_file(&output.path, HashAlgo::Sha1) else {
            continue;
        };
        if !verify::matches(expected, &actual) {
            warnings.push(format!(
                "processor {} 的产物摘要与 profile 不符（期望 {expected} 实际 {actual}）：非标准 zlib 实现会压出不同字节，按可用处理",
                step.label
            ));
        }
    }
    Ok(())
}

/// 产物是否都已就绪（幂等判据：存在 + 摘要命中；profile 没给摘要就只认存在）。
fn outputs_satisfied(step: &ProcessorStep, warnings: &mut Vec<String>) -> bool {
    if step.outputs.is_empty() {
        // 没有产物声明 → 无法判断是否跑过，只能每次都跑（HMCL 同样处理）。
        return false;
    }
    for output in &step.outputs {
        if !output.path.is_file() {
            return false;
        }
        let Some(expected) = &output.sha1 else {
            continue;
        };
        let Ok(actual) = verify::digest_file(&output.path, HashAlgo::Sha1) else {
            return false;
        };
        if !verify::matches(expected, &actual) {
            warnings.push(format!(
                "{} 的摘要与 profile 不符，重新执行该 processor",
                output.path.display()
            ));
            return false;
        }
    }
    true
}
