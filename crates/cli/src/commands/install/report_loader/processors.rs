//! 加载器安装报告里的 **processors 段**（M35）：人读几行 + `--json` 一块。
//!
//! M346 r3 从 `report_loader.rs` 搬出来（那边贴着 400 行硬线，其中一条用例的
//! `human_lines(...)` 调用还被 `#[rustfmt::skip]` 压在一行里才塞得下）。**纯搬移**：
//! 类型、函数体、文档逐字未变，只有可见性从「同文件私有」改成 `pub(super)`（结构体仍是
//! `pub`，由父模块 `pub use` 重导出，构造点 `install/loader.rs` 的路径一字未改）。

use serde_json::json;
use unlucky_core::addon::loader::processors::{ProcessorPlan, ProcessorRun};
use unlucky_core::game::install::InstallReport;

use crate::commands::pack::wording::step_body;

/// processors 段（M35）的报告输入：计划 + 依赖库下载结果 + 执行结果。
///
/// `--dry-run` 只有 `plan`；幂等跳过时 `libraries`/`run` 都是 `None` 且 `skipped = true`。
pub struct ProcessorSection<'a> {
    /// 执行计划（dry-run 也带上：将执行的 processor 清单 + 所需 Java）。
    pub plan: &'a ProcessorPlan,
    /// processor 依赖库的下载结果。
    pub libraries: Option<&'a InstallReport>,
    /// 执行结果。
    pub run: Option<&'a ProcessorRun>,
    /// 整段跳过（版本 JSON 已在盘上 = processors 早已跑完）。
    pub skipped: bool,
}

/// processors 段的 JSON 载荷（计划 + 依赖库结果 + 逐步骤结果）。
pub(super) fn processor_json(section: &ProcessorSection<'_>) -> serde_json::Value {
    let mut node = section.plan.to_json();
    if let Some(object) = node.as_object_mut() {
        object.insert("skipped".to_string(), json!(section.skipped));
        object.insert(
            "libraries_result".to_string(),
            json!(section.libraries.map(|report| json!({
                "downloaded": report.downloaded(),
                "skipped": report.skipped(),
                "failed": report.failed(),
                "bytes": report.bytes(),
            }))),
        );
        object.insert(
            "run".to_string(),
            json!(section.run.map(ProcessorRun::to_json)),
        );
    }
    node
}

/// processors 段的人读行（计划 / 依赖库 / 逐步骤 / 告警）。
pub(super) fn processor_lines(section: &ProcessorSection<'_>) -> Vec<String> {
    let plan = section.plan;
    let mut lines = Vec::new();
    lines.push(format!(
        "  installer processors {} 步，需要 {}（installer {}）",
        plan.step_count(),
        plan.java_requirement.describe(),
        plan.installer.display()
    ));
    if section.skipped {
        lines.push("  版本清单已存在：processors 整段跳过（幂等）".to_string());
    }
    if let Some(libraries) = section.libraries {
        lines.push(format!(
            "  processor 依赖库 {} 个：下载 {} / 跳过 {} / 失败 {}",
            plan.libraries.len(),
            libraries.downloaded(),
            libraries.skipped(),
            libraries.failed()
        ));
    }
    if let Some(run) = section.run {
        lines.push(format!(
            "  Java {}（{}）{}",
            run.java
                .major
                .map_or_else(|| "未知".to_string(), |major| major.to_string()),
            run.java.origin.as_str(),
            run.java.binary.display()
        ));
        let total = run.steps.len();
        for step in &run.steps {
            // 结局那半句只有一处实现（M346：`pack/wording.rs` 的 `step_body`，读目录 `PackProcessorStep*`）。
            lines.push(format!(
                "    [{}/{}] {} → {}",
                step.index,
                total,
                step.label,
                step_body(step.status.as_str(), step.outputs, step.elapsed_ms)
            ));
        }
        for warning in &run.warnings {
            lines.push(format!("  告警: {warning}"));
        }
    }
    for warning in &plan.warnings {
        lines.push(format!("  告警: {warning}"));
    }
    lines
}
