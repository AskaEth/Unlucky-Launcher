//! `dto::adopt` 的 IPC 契约用例（M223 从 `dto/adopt.rs` **纯搬移**，断言一条未改）。
//!
//! 为什么搬：`dto/adopt.rs` 落笔 512 行，越过 400 行硬线（AGENTS §3.1）；口径与
//! [`crate::dto::events::tests`] 完全一致——**纯搬移、一行不改**。`adopt.rs` 是非 `mod.rs`
//! 文件，`mod tests;` 的落点就是本文件。

use super::*;

fn candidate(version_id: &str, problems: Vec<AdoptProblemView>) -> AdoptCandidateView {
    AdoptCandidateView {
        version_id: version_id.to_string(),
        jar_present: true,
        inherits: Vec::new(),
        registered: Vec::new(),
        problems,
    }
}

fn scan_view() -> AdoptScanView {
    AdoptScanView {
        versions_dir: PathBuf::from("/game/versions"),
        candidates: vec![
            candidate("1.20.1", Vec::new()),
            candidate(
                "1.19.4",
                vec![AdoptProblemView {
                    code: "manifest_missing".to_string(),
                    message: "版本 1.19.4 没有清单文件".to_string(),
                }],
            ),
        ],
    }
}

#[test]
fn events_round_trip_through_json() {
    let event = AdoptEvent::Failed(Box::new(AdoptFailureView {
        code: "adopt_not_adoptable".to_string(),
        message: "版本 1.19.4 不能登记".to_string(),
        problems: vec![AdoptProblemView {
            code: "manifest_missing".to_string(),
            message: "清单缺了".to_string(),
        }],
    }));
    let text = serde_json::to_string(&event).expect("序列化");
    assert_eq!(
        serde_json::from_str::<AdoptEvent>(&text).expect("反序列化"),
        event
    );

    let command = AdoptCommand::Preview(Box::new(AdoptRequestView {
        version_id: "1.20.1".to_string(),
        name: Some("生存".to_string()),
        target: AdoptTargetView::Shared,
    }));
    let text = serde_json::to_string(&command).expect("序列化");
    assert_eq!(
        serde_json::from_str::<AdoptCommand>(&text).expect("反序列化"),
        command
    );
}

#[test]
fn scan_summarises_adoptable_and_empty() {
    let view = scan_view();
    assert_eq!(view.total(), 2);
    assert_eq!(view.adoptable(), 1);
    assert!(!view.is_empty());
    assert!(AdoptScanView {
        versions_dir: PathBuf::from("/game/versions"),
        candidates: Vec::new(),
    }
    .is_empty());
}

#[test]
fn target_slugs_round_trip_and_mark_foreign_dirs() {
    for target in AdoptTargetView::ALL {
        assert_eq!(AdoptTargetView::from_slug(target.slug()), Some(target));
    }
    assert_eq!(AdoptTargetView::default(), AdoptTargetView::InstanceDir);
    assert!(!AdoptTargetView::InstanceDir.touches_foreign_dir());
    assert!(AdoptTargetView::Shared.touches_foreign_dir());
    assert!(AdoptTargetView::VersionDir.touches_foreign_dir());
}

#[test]
fn state_follows_the_four_stages() {
    let mut state = AdoptState::default();
    assert_eq!(state.stage(), AdoptStage::Pick);

    state.apply(AdoptEvent::Scan(Box::new(scan_view())));
    assert_eq!(state.stage(), AdoptStage::Pick);
    assert_eq!(state.scan.as_ref().map(AdoptScanView::adoptable), Some(1));

    let plan = AdoptPlanView {
        version_id: "1.20.1".to_string(),
        name: "1.20.1".to_string(),
        target: AdoptTargetView::InstanceDir,
        isolation: AdoptIsolationView::Custom,
        run_dir: PathBuf::from("/ull/data/instances/1.20.1"),
        notes: vec!["运行目录 = /ull/data/instances/1.20.1".to_string()],
    };
    state.apply(AdoptEvent::Planned(Box::new(plan.clone())));
    assert_eq!(state.stage(), AdoptStage::Preview);
    assert_eq!(
        state.plan.as_ref().map(|plan| &plan.run_dir),
        Some(&plan.run_dir)
    );

    // 失败会把预检顶掉：留着预检，确认按钮就会在「不能登记」的结论上继续亮着
    state.apply(AdoptEvent::Failed(Box::new(AdoptFailureView {
        code: "adopt_duplicate_name".to_string(),
        message: "已有同名实例".to_string(),
        problems: Vec::new(),
    })));
    assert_eq!(state.stage(), AdoptStage::Failed);
    assert!(state.plan.is_none());

    state.apply(AdoptEvent::Done(Box::new(AdoptDoneView {
        instance_id: "id-1".to_string(),
        name: "1.20.1".to_string(),
        version_id: "1.20.1".to_string(),
        isolation: AdoptIsolationView::Custom,
        run_dir: PathBuf::from("/ull/data/instances/1.20.1"),
        dir_created: true,
        notes: Vec::new(),
    })));
    assert_eq!(state.stage(), AdoptStage::Done);
    assert!(state.failure.is_none());

    // 关面板 = 全清（再开要重新扫，否则画的是上一轮的候选）
    state.apply(AdoptEvent::Cleared);
    assert_eq!(state.stage(), AdoptStage::Pick);
    assert!(state.scan.is_none() && state.done.is_none());
}
