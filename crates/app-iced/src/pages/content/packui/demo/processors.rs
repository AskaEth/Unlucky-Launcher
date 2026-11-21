//! 加载器安装步骤那一段的出图夹具（M328）。
//!
//! 拆出来只为行数（`demo.rs` 已经贴着 400 行硬线）：夹具形状本身与产品路径的 DTO 一一对应，
//! 造完仍走 [`super::super::model::Pack::of`] 那条产品路径（见 `demo.rs` 的模块文档）。
//!
//! 三步而不是二十步：3 步以内 `parts::bounded` 直接铺开（不套空框），图上能看到完整一段；
//! 「步数多就滚」由 `bounded` 的既有口径保证，不需要靠夹具演示。

use unlucky_presenter::dto::{
    JavaOriginView, ProcessorStepStatusView, ProcessorStepView, ProcessorSummaryView,
};

/// 两跑一跳的处理器链（Forge 常见形状：第 2 步产物已就绪，整步跳过）。
pub(super) fn summary() -> ProcessorSummaryView {
    ProcessorSummaryView {
        java: "C:/ULL/data/java-managed/jdk-17-temurin/bin/java.exe".to_string(),
        java_origin: JavaOriginView::Managed,
        java_major: Some(17),
        executed: 2,
        skipped: 1,
        steps: vec![
            step(
                1,
                "net.minecraftforge:installertools:1.3.0",
                ProcessorStepStatusView::Executed,
                3,
                1_240,
            ),
            step(
                2,
                "net.minecraftforge:jarsplitter:1.2.0",
                ProcessorStepStatusView::Skipped,
                0,
                0,
            ),
            step(
                3,
                "net.minecraftforge:ForgeAutoRenamingTool:1.0.5",
                ProcessorStepStatusView::Executed,
                1,
                880,
            ),
        ],
        // 这一步链自己的告警与报告级 `warnings` 是同一份（core 的 `note_processors` 铺两次），
        // 界面上由报告级那几行说，所以夹具这里留空、不造两遍。
        warnings: Vec::new(),
    }
}

fn step(
    index: u32,
    label: &str,
    status: ProcessorStepStatusView,
    outputs: u32,
    elapsed_ms: u64,
) -> ProcessorStepView {
    ProcessorStepView {
        index,
        label: label.to_string(),
        status,
        outputs,
        elapsed_ms,
    }
}

/// 链跑了却**一步都没列**（`steps` 空）的夹具（M337）。
///
/// 真机形状：加载器安装器报「跑过」，但计划里 0 步（每一步都被免掉），报告照样带 `processors`
/// 段、`steps` 却是空表。此时界面不许把这一段画成空白——它要走「没有步骤」那一句
/// （`model::report_processor` 的 `empty`），所以夹具必须能造出这个形状。
pub(super) fn empty() -> ProcessorSummaryView {
    ProcessorSummaryView {
        steps: Vec::new(),
        executed: 0,
        skipped: 0,
        ..summary()
    }
}
