//! 加载器的 installer processors 段（M35）：依赖库下载 → 选 Java → 串行执行。
//!
//! 只在 `LoaderSupport::RequiresInstaller` 的版本上出现（Forge 1.13~1.20.x 主流段、
//! NeoForge 1.20.5+）。其余加载器（Fabric/Quilt、Forge spec 0）走纯 json 路线，这一段整块跳过。
//!
//! ```text
//! ProcessorPlan::build（core，纯计算：变量表 + 每步命令行 + 依赖库清单）
//!   → download_libraries（M27 引擎：镜像/健康度/退避/校验）
//!   → select_java（M33 解析链：托管 > JAVA_HOME > PATH > 扫盘）
//!   → run（串行 java -cp，产物存在性硬校验、摘要软校验）
//! ```
//!
//! 顺序上与版本 JSON 的关系是**硬约束**：processors 全部成功之后才让 M27 的 `execute`
//! 写 `versions/<id>/<id>.json`——那份清单是"这版装完了"的判据，抢在 processors 前写会
//! 留下一个启动必崩却被报成"已安装"的实例。

use std::sync::Arc;

use unlucky_core::addon::loader::processors::{
    self, ProcessorError, ProcessorPlan, ProcessorPlanInput, ProcessorRun, StepOutcome, StepSink,
};
use unlucky_core::addon::loader::LoaderRelease;
use unlucky_core::download::blocking::block_on;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::arguments::Environment;
use unlucky_core::game::install::InstallReport;

use super::{failure_error, report, CONCURRENCY};
use crate::cli::InstallArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::progress_line;

/// 组执行计划；不需要跑 installer 的加载器返回 `None`。
///
/// # Errors
///
/// install profile 的 `data` 表/`processors` 形状不对时返回 `processor_plan` 错误。
pub(super) fn plan(ctx: &Ctx, release: &LoaderRelease) -> Result<Option<ProcessorPlan>, CliError> {
    if !release.requires_installer() {
        return Ok(None);
    }
    let (Some(profile), Some(installer)) = (
        release.install_profile.as_ref(),
        release.installer.as_deref(),
    ) else {
        return Err(CliError::user(
            "loader_metadata",
            format!(
                "{} {} 需要跑 installer processors，但没拿到 install profile / installer 路径",
                release.kind.label(),
                release.loader_version
            ),
        )
        .with_hint("重跑一次（元数据缓存可能不完整）"));
    };
    let game = ctx.game();
    let scratch = ctx
        .layout()
        .cache()
        .join("processors")
        .join(safe_name(&release.version_id));
    let plan = ProcessorPlan::build(ProcessorPlanInput {
        kind: release.kind,
        mc_version: &release.mc_version,
        version_id: &release.version_id,
        profile,
        layout: &game,
        installer,
        scratch: &scratch,
        environment: &Environment::host(),
    })
    .map_err(map_processor_error)?;
    Ok(Some(plan))
}

/// 下载依赖库 + 执行 processors。
///
/// # Errors
///
/// 依赖库下载失败（稳定码沿用 `install_*`）、找不到可用 Java、processor 非零退出、
/// 产物没落盘。
pub(super) fn execute(
    ctx: &Ctx,
    engine: &DownloadEngine,
    plan: &ProcessorPlan,
    args: &InstallArgs,
) -> Result<(InstallReport, ProcessorRun), CliError> {
    let sink = report::progress_sink(args.progress, args.json_progress, plan.libraries.len());
    let libraries = block_on(processors::download_libraries(
        engine,
        &plan.libraries,
        CONCURRENCY,
        sink,
    ));
    if !libraries.is_complete() {
        return Err(failure_error(&libraries, ctx));
    }

    let managed = ctx.layout().java_managed();
    let Some(java) = processors::select_java(&managed, &plan.java_requirement) else {
        return Err(CliError::user(
            "processor_java_missing",
            format!(
                "跑 installer processors 需要 {}，但一个可用的 Java 都没找到",
                plan.java_requirement.describe()
            ),
        )
        .with_hint(format!(
            "`%prog% java install {}` 装一个托管 JDK，或把 JAVA_HOME 指向可用的 JDK（托管目录 {}）",
            plan.java_requirement.minimum,
            managed.display()
        )));
    };
    let sink = step_sink(args.progress, args.json_progress, plan.step_count());
    let run = processors::run(plan, &java, sink).map_err(map_processor_error)?;
    Ok((libraries, run))
}

/// 每步一行的实时进度（只写 stderr）。
fn step_sink(enabled: bool, json_mode: bool, total: usize) -> Option<StepSink> {
    if !enabled {
        return None;
    }
    Some(Arc::new(move |outcome: &StepOutcome| {
        let record = serde_json::json!({
            "event": "processor",
            "index": outcome.index,
            "total": total,
            "jar": outcome.label,
            "status": outcome.status.as_str(),
            "elapsed_ms": outcome.elapsed_ms,
        });
        let text = format!(
            "processor {}/{} {} → {}（输出 {} 个{}）",
            outcome.index,
            total,
            outcome.label,
            if outcome.status.as_str() == "skipped" {
                "跳过（产物已就绪）"
            } else {
                "执行完成"
            },
            outcome.outputs,
            if outcome.elapsed_ms > 0 {
                format!("，{} ms", outcome.elapsed_ms)
            } else {
                String::new()
            }
        );
        if json_mode {
            progress_line(&record.to_string());
        } else {
            progress_line(&text);
        }
    }))
}

/// processor 执行错误 → CLI 错误（稳定码 `processor_*`）。
fn map_processor_error(error: ProcessorError) -> CliError {
    let cli = CliError::user(error.kind().as_str(), error.message().to_string());
    match error.hint() {
        Some(hint) => cli.with_hint(hint.to_string()),
        None => cli,
    }
}

/// 版本 id → 可做目录名的片段（`+` 与路径分隔符都换掉）。
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
