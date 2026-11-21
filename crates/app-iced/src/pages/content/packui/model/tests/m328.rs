//! M328：加载器安装步骤段**真的被构造出来**了吗。
//!
//! 判据落在 [`report_of`]（`&ImportReportView → Report` 的纯函数）上——它不碰 iced、不开窗：
//! 给一份三步的报告，段必须存在、段头/汇总/Java 行都是人话（不是 slug）、步数与名字对得上。
//!
//! 抓得住四件事：
//! 1. 段没接进 `Report`（`processor` 是 `None`）⇒ 第一条红；
//! 2. 来源又退回 slug 上屏（`managed` 而不是「启动器自己装的」）⇒ 第一条红；
//! 3. 耗时把单位带进数字（上屏 `1240 ms ms`）⇒ 第一条红；
//! 4. 汇总行「为 0 就省略半句」的条件分支被加回来 ⇒ 第二条红（`0 skipped` 必须照印）。
//!
//! 画法（`bounded` 收口后的真实高度）**不在**这里——那只能靠出图看。

use std::path::PathBuf;

use unlucky_presenter::dto::{
    ImportCancelView, ImportReportView, ImportStatusView, JavaOriginView, PackFormatView,
    ProcessorStepStatusView, ProcessorStepView, ProcessorSummaryView,
};

use super::super::report::report_of;
use super::{catalog, PackPageState};

/// 一份带处理器链的报告（其余字段留空：这一段与它们无关）。
fn report_with(processors: Option<ProcessorSummaryView>) -> ImportReportView {
    ImportReportView {
        status: ImportStatusView::Success,
        format: Some(PackFormatView::Curseforge),
        pack_name: Some("Better MC".to_string()),
        pack_version: None,
        game_version: Some("1.20.1".to_string()),
        loader: Some("forge 47.2.0".to_string()),
        loader_installed: true,
        processors,
        version_id: Some("1.20.1-forge-47.2.0".to_string()),
        target: PathBuf::from("C:/ULL/instances/survival/.minecraft"),
        files: Vec::new(),
        conflicts: Vec::new(),
        missing: Vec::new(),
        rejected: Vec::new(),
        warnings: Vec::new(),
        error: None,
        registered: Some("Survival".to_string()),
        bytes_written: 0,
        rolled_back: true,
        failure_stage: None,
        cancel: ImportCancelView::NotRequested,
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

/// 两跑一跳（照 core 的 `ProcessorSummary::from_run` 形状）。
fn summary(skipped: u32) -> ProcessorSummaryView {
    ProcessorSummaryView {
        java: "C:/ULL/data/java-managed/jdk-17-temurin/bin/java.exe".to_string(),
        java_origin: JavaOriginView::Managed,
        java_major: Some(17),
        executed: 2,
        skipped,
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
        warnings: Vec::new(),
    }
}

#[test]
fn a_report_with_steps_builds_the_loader_step_section() {
    let report = report_with(Some(summary(1)));
    let block = report_of(&report, &PackPageState::default(), catalog());

    let section = block
        .processor
        .expect("报告里有处理器链 ⇒ 段必须被构造出来");

    // ① 段头 / 汇总 / Java 行：三句人话，不是 slug。
    assert_eq!(section.title, "Loader install steps");
    assert_eq!(section.summary, "2 ran · 1 skipped");
    assert_eq!(
        section.java_line, "ran on Java 17 (installed by the launcher)",
        "来源要走 M325 那五条人话，不许印 managed"
    );
    assert!(
        !section.java_line.contains("managed"),
        "内部标识不上屏：{}",
        section.java_line
    );

    // ② 逐步骤：步数、序号、名字、结局都对得上。
    assert_eq!(section.steps.len(), 3, "三步全在（含跳过的那步）");
    assert_eq!(section.steps[0].index, 1);
    assert_eq!(
        section.steps[0].label,
        "net.minecraftforge:installertools:1.3.0"
    );
    assert_eq!(section.steps[0].note, "ran · 3 outputs · 1240 ms");
    assert_eq!(
        section.steps[1].note, "skipped · output already there",
        "跳过那步不带耗时（跳过的耗时恒为 0，印 0 只会误导）"
    );
    assert_eq!(section.steps[2].note, "ran · 1 outputs · 880 ms");

    // ③ 耗时是纯数字：单位在文案里，带进数字就会上屏 `1240 ms ms`。
    assert!(
        !section.steps[0].note.contains("ms ms"),
        "耗时不许带单位：{}",
        section.steps[0].note
    );

    // ④ 空态不该出现（有步骤就没有空态那一句）。
    assert!(section.empty.is_none());
}

#[test]
fn the_summary_always_prints_both_halves() {
    // 塔台口径：汇总行**不做**「为 0 就省略半句」的条件分支——`跳过 0 步` 是诚实信息，
    // 且零分支才能纯断言（多一个 `if` 就多一条没人测的分支）。
    let report = report_with(Some(summary(0)));
    let block = report_of(&report, &PackPageState::default(), catalog());
    let section = block.processor.expect("段必须在");
    assert_eq!(section.summary, "2 ran · 0 skipped", "跳过 0 步也要印出来");
}

#[test]
fn a_chain_with_no_steps_says_so_instead_of_drawing_nothing() {
    let mut empty = summary(0);
    empty.steps.clear();
    empty.executed = 0;
    empty.skipped = 0;
    let report = report_with(Some(empty));
    let block = report_of(&report, &PackPageState::default(), catalog());
    let section = block.processor.expect("链跑了就有段");
    assert!(section.steps.is_empty());
    assert_eq!(
        section.empty.as_deref(),
        Some("The loader installer listed no steps."),
        "一步都没列时要有一句话，别画成空白"
    );
}

#[test]
fn a_report_without_a_chain_has_no_section() {
    // 纯原版 / 没接安装器：不许编一段「装了 0 步」出来。
    let block = report_of(&report_with(None), &PackPageState::default(), catalog());
    assert!(block.processor.is_none(), "{:?}", block.processor);
}
