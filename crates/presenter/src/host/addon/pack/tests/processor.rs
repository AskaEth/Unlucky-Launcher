//! M328：装载器那一段**真的过了边界**吗（纯 presenter 判据，不靠界面）。
//!
//! # 判据
//!
//! 给 core 一份三步的报告（两跑一跳）⇒ DTO 里必须是三步，且名字、结局、产物数、耗时、
//! 来源逐项都在。这条用例不碰 iced、不开窗：它钉的是「转换那一格搬没搬」，不是「画得好不好」。
//!
//! # 变异证明
//!
//! 把 `convert::report_view` 里的 `processors: report.processors.as_ref().map(...)` 换成
//! `processors: None` ⇒ 本用例红（`core 有处理器段，DTO 就必须有`）。M328 之前在真实现上
//! 就是这个样子：报告里每一步都在，DTO 里一格都没有。
//!
//! 反过来也钉了一条：`failed_report_with` 那一份构造**必须显式表态**（`processors: None`）——
//! 预检/装配阶段就没进管线，报一句「装了 0 步」是假话。

use std::path::Path;

use unlucky_core::addon::import::report::{ProcessorStep, ProcessorSummary};
use unlucky_core::addon::import::ImportReport;
use unlucky_core::game::java::JavaOrigin;

use crate::dto::{JavaOriginView, ProcessorStepStatusView};

use super::harness::temp_dir;

fn step(index: usize, label: &str, status: &'static str, outputs: usize, ms: u64) -> ProcessorStep {
    ProcessorStep {
        index,
        label: label.to_string(),
        status,
        outputs,
        elapsed_ms: ms,
    }
}

/// 一份「两跑一跳」的 processor 汇总（形状照 core 的 `ProcessorSummary::from_run`）。
fn summary() -> ProcessorSummary {
    ProcessorSummary {
        java: "C:/ULL/data/java-managed/jdk-17-temurin/bin/java.exe".to_string(),
        java_origin: JavaOrigin::Managed,
        java_major: Some(17),
        executed: 2,
        skipped: 1,
        steps: vec![
            step(
                1,
                "net.minecraftforge:installertools:1.3.0",
                "executed",
                3,
                1_240,
            ),
            step(2, "net.minecraftforge:jarsplitter:1.2.0", "skipped", 0, 0),
            step(
                3,
                "net.minecraftforge:ForgeAutoRenamingTool:1.0.5",
                "executed",
                1,
                880,
            ),
        ],
        warnings: vec!["产物数 3 与计划里的 2 不一致".to_string()],
    }
}

#[test]
fn every_processor_step_reaches_the_dto() {
    let target = temp_dir("m328-processors");
    let mut report = ImportReport::empty(target.clone());
    report.note_processors(summary());
    let core_steps = report
        .processors
        .as_ref()
        .expect("夹具自己设的 processor 段")
        .steps
        .len();

    let view = super::super::convert::report_view(&report, target.as_path(), false);

    let processors = view
        .processors
        .as_ref()
        .expect("core 有处理器段，DTO 就必须有");

    // ① 步数一致：不搬那一格这一条就红（`processors` 是 `None`）。
    assert_eq!(processors.steps.len(), core_steps, "步数必须逐条搬过来");
    assert_eq!(processors.steps.len(), 3, "两跑一跳");
    assert_eq!(
        (processors.executed, processors.skipped),
        (2, 1),
        "两跑一跳"
    );

    // ② 每步的名字 / 结局 / 产物数 / 耗时都在。
    let expected = [
        (
            "net.minecraftforge:installertools:1.3.0",
            ProcessorStepStatusView::Executed,
            3,
            1_240,
        ),
        (
            "net.minecraftforge:jarsplitter:1.2.0",
            ProcessorStepStatusView::Skipped,
            0,
            0,
        ),
        (
            "net.minecraftforge:ForgeAutoRenamingTool:1.0.5",
            ProcessorStepStatusView::Executed,
            1,
            880,
        ),
    ];
    for (index, (label, status, outputs, elapsed)) in expected.iter().enumerate() {
        let step = &processors.steps[index];
        assert_eq!(
            step.index as usize,
            index + 1,
            "序号从 1 起，顺序即执行顺序"
        );
        assert_eq!(step.label.as_str(), *label, "第 {} 步的名字", index + 1);
        assert_eq!(step.status, *status, "第 {} 步的结局", index + 1);
        assert_eq!(step.outputs, *outputs, "第 {} 步的产物数", index + 1);
        // 耗时是**纯数字毫秒**：单位归文案（`{elapsed} ms`），这里带单位就上屏两次。
        assert_eq!(step.elapsed_ms, *elapsed, "第 {} 步的耗时", index + 1);
    }

    // ③ Java 与来源：来源是枚举（可穷尽 match），slug 只作稳定 id 用，不上屏。
    assert_eq!(
        processors.java_origin,
        JavaOriginView::Managed,
        "来源要按 core 的枚举逐档搬"
    );
    assert_eq!(processors.java_origin.slug(), "managed");
    assert_eq!(
        JavaOriginView::from_slug("managed"),
        Some(JavaOriginView::Managed)
    );
    assert_eq!(processors.java_major, Some(17));
    assert_eq!(
        processors.java,
        "C:/ULL/data/java-managed/jdk-17-temurin/bin/java.exe"
    );

    // ④ 这一步链自己的告警也搬（它与报告级 warnings 是同一份，但界面那一段要能单独说）。
    assert_eq!(processors.warnings.len(), 1);
    assert!(processors.warnings[0].contains("产物数 3"));

    let _ = std::fs::remove_dir_all(&target);
}

#[test]
fn a_failed_report_says_it_had_no_processor_chain() {
    // 预检/装配阶段就失败：没进管线 ⇒ 不许编一个「装了 0 步」的段出来。
    let view = super::super::convert::host_failed_report(
        std::env::temp_dir(),
        Path::new("C:/packs/Broken.mrpack"),
        &super::super::import::TargetProblem::Occupied {
            path: std::env::temp_dir(),
        },
    );
    assert!(
        view.processors.is_none(),
        "没进管线就没有处理器段：{:?}",
        view.processors
    );
}

/// 状态字符串对齐（M333 P33-2 的**便宜的一半**）：core 的 `StepStatus::as_str` 与 DTO 的
/// [`ProcessorStepStatusView::slug`] 必须逐字相同。
///
/// 为什么这条用例值钱：`ProcessorStep.status` 是 `&'static str`（core 的枚举在上边界之前就被
/// 摊成字符串了），presenter 只能 `from_slug(..).unwrap_or(Executed)`——落不到任何一档**不报错**，
/// 只是把那一步说成「跑过」。core 哪天加第三档，界面就会**静默**印成
/// 「执行完成 · 产物 N 个 · T ms」。
///
/// 抓得住：既有两档两边一字不差、每一档都能从 slug 反解回来、DTO 不多出 core 不发的档。
/// 抓不住：core 新增的第三档本身（那需要 core 给出 `ALL`，本轮**不动 core 枚举**）——但下面那个
/// `match` **没有通配臂**，core 加一档时本文件**编译不过**，比静默印错强。
#[test]
fn every_core_step_status_slug_matches_the_view_slug() {
    use unlucky_core::addon::loader::processors::StepStatus;

    // core 的 `StepStatus` 没有 `ALL`（本轮不动 core）⇒ 两档在这里逐一列出，靠下面这个
    // **穷尽、无通配臂**的 `match` 当警报：core 加一档就编译不过。
    let pairs: Vec<(StepStatus, ProcessorStepStatusView)> =
        [StepStatus::Executed, StepStatus::Skipped]
            .into_iter()
            .map(|status| {
                let view = match status {
                    StepStatus::Executed => ProcessorStepStatusView::Executed,
                    StepStatus::Skipped => ProcessorStepStatusView::Skipped,
                };
                (status, view)
            })
            .collect();

    for (status, view) in &pairs {
        assert_eq!(
            view.slug(),
            status.as_str(),
            "core 与 DTO 的稳定 id 必须逐字相同（差一个字 = 界面静默改口径）"
        );
        assert_eq!(
            ProcessorStepStatusView::from_slug(status.as_str()),
            Some(*view),
            "presenter 必须能反解 core 发出的每一个 id"
        );
    }

    // DTO 侧不许多出 core 不发的档：多一档就是界面里有个永远进不去的分支。
    assert_eq!(
        pairs.len(),
        ProcessorStepStatusView::ALL.len(),
        "两档对两档"
    );
    for view in ProcessorStepStatusView::ALL {
        assert!(
            pairs.iter().any(|(_, mapped)| *mapped == view),
            "DTO 的这一档在 core 里没有对应：{view:?}"
        );
    }
}
