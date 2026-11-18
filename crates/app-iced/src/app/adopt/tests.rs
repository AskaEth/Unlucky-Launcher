//! `app::adopt` 的用态用例（M223）：表单态 + 四段投影。
//!
//! 抓得住：
//! - `Form` 的生命周期（open 清上一轮 / close 全清 / settled 解除等待）；
//! - `candidate()` 在选中项不在列表里时返回 `None`（界面自动退回「请选择」）；
//! - 四段的文案真源（标题/后果原文/问题逐条）能从 `AdoptState` 投影出来。
//!
//! 抓不住：**像素**（那是 D30 的截图通道）；也不抓 core 的判据（那是 `host/adopt/tests.rs`）。

use unlucky_i18n::{Key, Lang};
use unlucky_presenter::dto::{
    AdoptCandidateView, AdoptDoneView, AdoptEvent, AdoptFailureView, AdoptIsolationView,
    AdoptPlanView, AdoptProblemView, AdoptScanView, AdoptStage, AdoptState, AdoptTargetView,
};

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

fn state_with_scan() -> AdoptState {
    let mut state = AdoptState::default();
    state.apply(AdoptEvent::Scan(Box::new(AdoptScanView {
        versions_dir: std::path::PathBuf::from("/game/versions"),
        candidates: vec![candidate("1.20.1", Vec::new())],
    })));
    state
}

/// 打开 ⇒ 关闭：上一轮的选中与名字都要清掉（否则下次打开画的是上一次的草稿）。
#[test]
fn open_and_close_reset_the_form() {
    let mut form = Form::default();
    form.open();
    assert!(form.open);
    assert_eq!(form.busy, Busy::Scanning);
    assert_eq!(form.target, AdoptTargetView::InstanceDir);

    form.selected = Some("1.20.1".to_string());
    form.name = "生存".to_string();
    form.close();
    assert!(!form.open);
    assert!(form.selected.is_none());
    assert!(form.name.is_empty());

    // 再开一次也不该带着上一轮的选中项
    form.open();
    assert!(form.selected.is_none());
}

/// `settled` 只解除等待，不动用户填的东西（回程不该清表单）。
#[test]
fn settling_keeps_what_the_user_typed() {
    let mut form = Form::default();
    form.open();
    form.selected = Some("1.20.1".to_string());
    form.name = "生存".to_string();
    form.settled();
    assert_eq!(form.busy, Busy::Idle);
    assert_eq!(form.selected.as_deref(), Some("1.20.1"));
    assert_eq!(form.name, "生存");
}

/// 选中项不在候选列表里 ⇒ `candidate()` 是 `None`（界面不能对着一个不存在的版本画「预检」）。
#[test]
fn candidate_is_none_when_the_selection_vanished() {
    let state = state_with_scan();
    let mut form = Form {
        selected: Some("1.20.1".to_string()),
        ..Form::default()
    };
    assert!(form.candidate(&state).is_some());
    form.selected = Some("9.9.9".to_string());
    assert!(form.candidate(&state).is_none());
}

/// 四段的标题与后果原文都从目录来（不写死英文字面量、不重写 core 的后果句）。
#[test]
fn the_four_stages_project_from_the_event_state() {
    let catalog = Lang::ZhCn.catalog();
    let mut state = state_with_scan();

    // Pick
    assert_eq!(state.stage(), AdoptStage::Pick);
    assert_eq!(
        shell::title_text(state.stage(), catalog),
        catalog.text(Key::AdoptTitle),
        "标题走目录"
    );
    assert!(shell::subtitle(state.stage(), &state, catalog).contains("不复制"));

    // Preview：`notes` 是 core `target_notes()` 的原文，界面一个字都不许改
    let note =
        "运行目录 = /game（共享档）：mods/saves/config 会落在别人的 .minecraft 里".to_string();
    state.apply(AdoptEvent::Planned(Box::new(AdoptPlanView {
        version_id: "1.20.1".to_string(),
        name: "1.20.1".to_string(),
        target: AdoptTargetView::Shared,
        isolation: AdoptIsolationView::Shared,
        run_dir: std::path::PathBuf::from("/game"),
        notes: vec![note.clone()],
    })));
    assert_eq!(state.stage(), AdoptStage::Preview);
    assert_eq!(
        state.plan.as_ref().map(|plan| plan.notes.first().cloned()),
        Some(Some(note.clone())),
        "后果原文必须原样留着"
    );

    // Failed：problems 逐条（不折叠）
    state.apply(AdoptEvent::Failed(Box::new(AdoptFailureView {
        code: "adopt_not_adoptable".to_string(),
        message: "版本 1.19.4 不能登记".to_string(),
        problems: vec![
            AdoptProblemView {
                code: "manifest_missing".to_string(),
                message: "没有清单".to_string(),
            },
            AdoptProblemView {
                code: "missing_parent".to_string(),
                message: "缺父版本 1.20.1".to_string(),
            },
        ],
    })));
    assert_eq!(state.stage(), AdoptStage::Failed);
    assert_eq!(
        state.failure.as_ref().map(|f| f.problems.len()),
        Some(2),
        "两条问题一条都不许吞"
    );
    assert_eq!(
        shell::title_text(state.stage(), catalog),
        catalog.text(Key::AdoptFailureTitle)
    );

    // Done
    state.apply(AdoptEvent::Done(Box::new(AdoptDoneView {
        instance_id: "id-1".to_string(),
        name: "1.20.1".to_string(),
        version_id: "1.20.1".to_string(),
        isolation: AdoptIsolationView::Shared,
        run_dir: std::path::PathBuf::from("/game"),
        dir_created: false,
        notes: vec![note],
    })));
    assert_eq!(state.stage(), AdoptStage::Done);
    assert_eq!(
        shell::title_text(state.stage(), catalog),
        catalog.text(Key::AdoptDoneTitle)
    );
}

/// 中英两套目录的向导文案都不是占位（`text()` 回落到 key 名就是这个用例要抓的假绿）。
#[test]
fn both_catalogs_have_the_wizard_wording() {
    for lang in [Lang::En, Lang::ZhCn] {
        let catalog = lang.catalog();
        for key in [
            Key::AdoptTitle,
            Key::AdoptSubtitle,
            Key::AdoptScanning,
            Key::AdoptEmpty,
            Key::AdoptSelectLabel,
            Key::AdoptReady,
            Key::AdoptTargetLabel,
            Key::AdoptTargetInstanceDir,
            Key::AdoptTargetShared,
            Key::AdoptTargetVersionDir,
            Key::AdoptNameLabel,
            Key::AdoptPreview,
            Key::AdoptConfirm,
            Key::AdoptBack,
            Key::AdoptClose,
            Key::AdoptPlanTitle,
            Key::AdoptPlanRunDir,
            Key::AdoptPlanIsolation,
            Key::AdoptFailureTitle,
            Key::AdoptProblemCount,
            Key::AdoptDoneTitle,
            Key::AdoptDoneHint,
        ] {
            let value = catalog.text(key);
            assert!(!value.is_empty(), "{key:?} 在 {lang:?} 是空的");
            assert_ne!(
                value,
                format!("{key:?}"),
                "{key:?} 在 {lang:?} 没进目录（回落成了 key 名）"
            );
        }
    }
    // 共享档的短提示必须把「会写进别人的 .minecraft」说清楚（选之前就得知道）
    let hint = Lang::En.catalog().text(Key::AdoptTargetSharedHint);
    assert!(hint.contains(".minecraft"), "{hint}");
}

/// 「点选共享档 ⇒ 立刻看到后果原文」但**不**把用户甩到确认页：
/// 预检结果到了也只是 Pick 段里的提示，直到用户点过「预检」(`checked`) 才进 Preview。
///
/// 抓得住：把 `Form::stage` 写回直接取 `AdoptState::stage()`（点一次共享档就跳到确认页，
/// 实例名还没来得及改）；或忘了 `fresh_plan` 的目标一致性判据（改档之后画上一档的后果原文）。
#[test]
fn a_foreign_target_shows_its_notes_without_jumping_to_confirm() {
    let mut state = state_with_scan();
    let note =
        "运行目录 = /game（共享档）：mods/saves/config 会落在别人的 .minecraft 里".to_string();
    state.apply(AdoptEvent::Planned(Box::new(AdoptPlanView {
        version_id: "1.20.1".to_string(),
        name: "1.20.1".to_string(),
        target: AdoptTargetView::Shared,
        isolation: AdoptIsolationView::Shared,
        run_dir: std::path::PathBuf::from("/game"),
        notes: vec![note.clone()],
    })));

    let mut form = Form {
        open: true,
        selected: Some("1.20.1".to_string()),
        target: AdoptTargetView::Shared,
        ..Form::default()
    };

    // 自动预检回来但用户没点「预检」：仍是 Pick 段，notes 当作提示可读
    assert_eq!(form.stage(&state), AdoptStage::Pick);
    assert_eq!(
        form.fresh_plan(&state)
            .map(|plan| plan.notes.first().cloned()),
        Some(Some(note.clone())),
        "后果原文必须能取到"
    );

    // 点过「预检」之后才进确认段
    form.checked = true;
    assert_eq!(form.stage(&state), AdoptStage::Preview);

    // 换了目标档：上一档的预检作废（不再当后果原文画）
    form.target = AdoptTargetView::VersionDir;
    assert!(form.fresh_plan(&state).is_none(), "过期的预检不许画");
}

/// **P2-2**：`fresh_plan` 必须按**版本 id** 过期，不只是目标档。
///
/// 场景：先选 1.20.1 + 版本目录档（notes 说 `versions/1.20.1`），再改选 1.19.4——目标档没变，
/// 旧的预检结果若不按版本过滤就会继续当「这个版本的后果原文」画出来，那是一句假话。
///
/// 抓得住：`fresh_plan` 只看 `plan.target == self.target`（第一次修法就是这样漏的）。
/// 抓不住：core 真的会不会为 1.19.4 算出同一段 notes（那是 `target_notes()` 的活）。
#[test]
fn fresh_plan_expires_when_the_selected_version_changes() {
    let mut state = state_with_scan();
    state.apply(AdoptEvent::Planned(Box::new(AdoptPlanView {
        version_id: "1.20.1".to_string(),
        name: "1.20.1".to_string(),
        target: AdoptTargetView::VersionDir,
        isolation: AdoptIsolationView::Isolated,
        run_dir: std::path::PathBuf::from("/game/versions/1.20.1"),
        notes: vec!["运行目录 = 版本目录 /game/versions/1.20.1".to_string()],
    })));

    let mut form = Form {
        open: true,
        selected: Some("1.20.1".to_string()),
        target: AdoptTargetView::VersionDir,
        checked: true,
        ..Form::default()
    };
    assert!(
        form.fresh_plan(&state).is_some(),
        "对得上版本与目标档时应当可读"
    );

    // 改选另一个版本：目标档没变，但旧预检说的是别的版本的目录
    form.selected = Some("1.19.4".to_string());
    assert!(
        form.fresh_plan(&state).is_none(),
        "换了版本之后旧预检必须过期（否则画的是 1.20.1 的后果原文）"
    );
    // 没选版本时同样不该画（`None` = 没有任何版本的预检可言）
    form.selected = None;
    assert!(form.fresh_plan(&state).is_none());
}
