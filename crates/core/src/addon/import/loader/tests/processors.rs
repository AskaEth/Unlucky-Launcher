//! M321 的用例：**processor 链必须说话**（从 `tests.rs` 分出来，那边贴着 400 行硬线）。
//!
//! 两处判据互相独立：
//!
//! | 用例 | 抓得住什么（变异） |
//! |------|--------------------|
//! | [`the_processor_steps_really_reach_the_sink_the_caller_gave`] | `install_declared_loader` 把 `job.steps` 传回 `None` ⇒ 回调一条都收不到 |
//! | [`the_processor_run_and_its_warnings_land_in_the_report_itself`] | `done.processors` / `done.warnings` 又被丢弃 ⇒ 报告里没有货；`note_processors` 与 `note_loader_warnings` 重复铺同一条 ⇒ 「只许一次」的断言红 |
//!
//! 全部离线：假件不发一个字节、不起 java（夹具见 [`super::support`]）。

use std::sync::{Arc, Mutex};

use crate::addon::import::keys;
use crate::addon::import::{import_pack, ImportRequest, ImportStatus};
use crate::addon::loader::processors::{StepOutcome, StepSink};
use crate::addon::temp::TempRoot;
use crate::download::blocking::block_on;
use crate::game::java::JavaOrigin;
use crate::game::layout::GameLayout;

use super::support::{mrpack, FakeInstaller};

/// 逐步骤回调真的交到了安装器手上（不再是 `None`），每一步都回调一次。
#[test]
fn the_processor_steps_really_reach_the_sink_the_caller_gave() {
    let root = TempRoot::new("m321-steps");
    let pack = mrpack("m321-steps-pack");
    let target = root.join("instance");
    let game = GameLayout::new(root.join("game"));
    let installer = FakeInstaller {
        steps: 3,
        ..FakeInstaller::default()
    };

    let seen: Arc<Mutex<Vec<(usize, String, u128)>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&seen);
    let sink: StepSink = Arc::new(move |outcome: &StepOutcome| {
        recorder.lock().expect("锁").push((
            outcome.index,
            outcome.label.clone(),
            outcome.elapsed_ms,
        ));
    });

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target)
            .with_loader_install(&installer, &game)
            .with_steps(sink),
    ));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    let seen = seen.lock().expect("锁").clone();
    assert_eq!(
        seen.iter().map(|(index, _, _)| *index).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "三步必须逐条回调（顺序即执行顺序）：{seen:?}"
    );
    assert!(
        seen.iter()
            .all(|(_, label, _)| label.contains("net.example")),
        "回调里要带得出「是哪一步」：{seen:?}"
    );
}

/// 告警与逐步骤结果必须**在数据层**就进 `ImportReport`（不是 CLI 自己拼的）。
///
/// 这条用例一条 CLI 代码都不碰，只直调 `import_pack`：GUI 走的是同一条 core 路径，
/// 所以「只在 CLI 成立」这种东西在这条断言下不可能蒙过去。
///
/// 三份告警各有归宿（**同一来源不许进两次**，否则人读输出里同一条会印两遍）：
/// 计划期 → `NOTE_LOADER_WARNING`；processor 期 → `NOTE_PROCESSOR_WARNING`（来自
/// `ProcessorSummary`）。每条只计一次是硬要求，见下面 `count == 1`。
#[test]
fn the_processor_run_and_its_warnings_land_in_the_report_itself() {
    let root = TempRoot::new("m321-report");
    let pack = mrpack("m321-report-pack");
    let target = root.join("instance");
    let game = GameLayout::new(root.join("game"));
    let installer = FakeInstaller {
        steps: 2,
        warnings: vec!["loader 库缺下载地址，已降级".to_string()],
        processor_warnings: vec![
            "产物摘要不符：forge-client.jar（本机 JDK 的 zlib 变体）".to_string()
        ],
        ..FakeInstaller::default()
    };

    let report = block_on(import_pack(
        &ImportRequest::new(pack.path(), &target).with_loader_install(&installer, &game),
    ));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    // ①逐步骤结果
    let processors = report
        .processors
        .as_ref()
        .expect("processor 链跑过就必须把结果带进报告");
    assert_eq!((processors.executed, processors.skipped), (2, 0));
    assert_eq!(
        processors
            .steps
            .iter()
            .map(|step| (step.index, step.status))
            .collect::<Vec<_>>(),
        vec![(1, "executed"), (2, "executed")],
        "{processors:?}"
    );
    assert_eq!(processors.java_origin, JavaOrigin::Managed);
    // ②两份告警各自带着自己的 key 进 `report.warnings`（界面/脚本按 key 查文案）
    let with = |key: &str| {
        report
            .warnings
            .iter()
            .filter(|note| note.message_key == key)
            .count()
    };
    assert_eq!(with(keys::NOTE_LOADER_WARNING), 1, "{:?}", report.warnings);
    assert_eq!(
        with(keys::NOTE_PROCESSOR_WARNING),
        1,
        "同一条 processor 告警不许进两次：{:?}",
        report.warnings
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|note| note.message.contains("zlib 变体")),
        "{:?}",
        report.warnings
    );
    // ③`--json` 的载荷里真有这一段（报告是序列化的，不是内部字段）
    let json = serde_json::to_value(&report).expect("报告要能序列化");
    assert_eq!(json["processors"]["executed"], 2, "{json}");
    assert_eq!(
        json["processors"]["steps"].as_array().map(Vec::len),
        Some(2)
    );
    assert!(
        json["warnings"]
            .as_array()
            .is_some_and(|list| !list.is_empty()),
        "warnings 在 JSON 里也不许是空的：{json}"
    );
}
