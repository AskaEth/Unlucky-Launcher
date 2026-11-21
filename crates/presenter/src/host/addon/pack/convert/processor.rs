//! 加载器安装器那一段（M328）：core 的 `ProcessorSummary` → DTO。
//!
//! 判据与翻译口径同父模块：**逐字段照搬，不重算任何结论**。步数不在这里数一遍
//! （`executed` / `skipped` 抄 core 的），步骤顺序也不排（core 给的就是执行顺序）。
//!
//! 两个 `match` 都是**穷尽、无通配臂**：
//! - [`origin_view`]：core 的 `JavaOrigin` 加一档就编译不过（界面那五条词也必须跟着加）；
//! - 步骤结局那一路反解 core 的稳定 id（`executed` / `skipped`），落不到任何一档时退回
//!   `Executed`（= 「真起过 java」的保守说法：宁可把一步说成跑过，也不许把跑过的步说成跳过）。

use unlucky_core::addon::import::report::{ProcessorStep, ProcessorSummary};
use unlucky_core::game::java::JavaOrigin;

use crate::dto::{
    JavaOriginView, ProcessorStepStatusView, ProcessorStepView, ProcessorSummaryView,
};

/// 一次 processor 链的结局 → DTO（`None` 原样传下去：没有链就没有这一段）。
pub(super) fn summary_view(summary: &ProcessorSummary) -> ProcessorSummaryView {
    ProcessorSummaryView {
        java: summary.java.clone(),
        java_origin: origin_view(summary.java_origin),
        java_major: summary.java_major,
        executed: count(summary.executed),
        skipped: count(summary.skipped),
        steps: summary.steps.iter().map(step_view).collect(),
        warnings: summary.warnings.clone(),
    }
}

fn step_view(step: &ProcessorStep) -> ProcessorStepView {
    ProcessorStepView {
        index: count(step.index),
        label: step.label.clone(),
        status: ProcessorStepStatusView::from_slug(step.status)
            .unwrap_or(ProcessorStepStatusView::Executed),
        outputs: count(step.outputs),
        // `elapsed_ms` 本身就是毫秒数字：单位是文案的事（`{elapsed} ms`），这里带单位就会上屏两次。
        elapsed_ms: step.elapsed_ms,
    }
}

/// core 的来源枚举 → DTO 枚举（**穷尽**：core 加一档，这里少一个臂就编译不过）。
const fn origin_view(origin: JavaOrigin) -> JavaOriginView {
    match origin {
        JavaOrigin::Instance => JavaOriginView::Instance,
        JavaOrigin::Managed => JavaOriginView::Managed,
        JavaOrigin::JavaHome => JavaOriginView::JavaHome,
        JavaOrigin::Path => JavaOriginView::Path,
        JavaOrigin::System => JavaOriginView::System,
    }
}

/// `usize` → DTO 的 `u32`（与 DTO 里其余计数同一条收口：越界就饱和，不 panic）。
fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
