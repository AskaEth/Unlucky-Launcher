//! 安装面板的决策测试（M94 r2）。
//!
//! 从 `install.rs` 外置（那边加上本轮修复会顶到 400 行）。`use super::*` 仍指向
//! `crate::pages::download::install`，`PanelState` / `panel_state` 是那里的 `pub(super)`。
//!
//! **不走 `MockBackend`**：`MockBackend::script_install` 只发 Dialog + Toast、从不发
//! `InstallProgress`（复审 r1 的取证），所以 mock 下 `install` 恒为 `None`，终态分支永远跑不到——
//! 用 mock 写的测试会**假绿**。这里直接构造 `AppState` 并 `apply(CoreEvent::InstallProgress(..))`。

use unlucky_presenter::dto::{
    AppEvent, CoreEvent, InstallProgressView, InstallStage, VersionKind, VersionSummary,
};
use unlucky_presenter::state::AppState;

use unlucky_i18n::Lang;

use super::super::model::{Category, Download};
use super::super::options::InstallDraft;
use super::{panel_state, PanelState};

fn version(id: &str) -> VersionSummary {
    VersionSummary {
        id: id.to_string(),
        kind: VersionKind::Release,
        released: "2026-09-20".to_string(),
        installed: false,
    }
}

fn progress(stage: InstallStage) -> InstallProgressView {
    InstallProgressView {
        version: "1.21.9".to_string(),
        stage,
        files_done: 512,
        files_total: 512,
        ..InstallProgressView::new()
    }
}

fn snapshot(stage: InstallStage) -> AppState {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(vec![
        version("1.21.9"),
        version("1.21.8"),
    ])));
    state.apply(AppEvent::Core(CoreEvent::InstallProgress(Some(progress(
        stage,
    )))));
    state
}

/// **装完之后页面必须回到「能再装下一个」**（复审 p2 的修复验收）。
///
/// 复审现象：`InstallDone` 从不清 `AppState.install`（presenter 只在取消时清），而面板**优先**
/// 用 `install`，终态又没有可执行动作 ⇒ 装完第一个版本后按钮永久消失，只能重启进程。
///
/// 抓得住：`Done` 之后面板还被 `install` 占着（= 装不了第二个版本的根因）、列表没回来、
/// 面板不再给「安装」。测试直接钉 [`panel_state`]——那是 `panel()` 唯一的决策入口，两者不会漂。
/// 抓不住：presenter 侧 `AppState.install` 仍是 `Some(Done)`（proper fix 不在本 mission，已报 finding）；
/// 也抓不住「装完后已装标记不刷新」（同一个 finding）。
#[test]
fn a_finished_install_gives_the_page_back_to_the_list() {
    let running = Download::from_snapshot(
        &snapshot(InstallStage::Downloading),
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert!(running.install.is_some(), "跑着的时候面板要在");
    assert!(matches!(panel_state(&running), PanelState::Install(_)));

    let state = snapshot(InstallStage::Done);
    assert_eq!(
        state.install.as_ref().map(|view| view.stage),
        Some(InstallStage::Done),
        "现实如此：快照里仍留着终态（presenter 不清），页面必须自己让位"
    );

    let model = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert!(model.install.is_none(), "Done 之后不许再占着安装面板");
    assert!(model.selected.is_none(), "装完不替用户选中任何行");
    assert_eq!(model.rows.len(), 2, "列表要回来");
    assert_eq!(
        panel_state(&model),
        PanelState::Hint,
        "回到「选一个版本再安装」"
    );

    // 再装一次：选中下一个版本 → 面板重新给出**安装选项区**（M105 起「安装」在选项区里）
    let next = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft {
            selected: Some("1.21.9".to_string()),
            ..InstallDraft::default()
        },
        Lang::En.catalog(),
    );
    let options = match panel_state(&next) {
        PanelState::Options(options) => options,
        other => panic!("装完之后必须能接着装下一个版本，实际 {other:?}"),
    };
    assert_eq!(options.version, "1.21.9");
    assert!(options.can_confirm, "原版 + 列表里的版本 = 可以装");
}

/// 取消与失败**不许**一起被清：失败态是 P2-4 的错误出口，取消是「已经没事了」。
///
/// 抓得住：把 `finished()` 写成「terminated()」（那会把失败态的日志入口一起清掉，用户失去原因）。
/// 抓不住：presenter 在取消后是否补发 `InstallProgress(None)`（那是 presenter 的行为）。
#[test]
fn only_done_and_cancelled_release_the_panel() {
    let failed = Download::from_snapshot(
        &snapshot(InstallStage::Failed),
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert!(failed.install.is_some(), "失败态要留在屏幕上");
    assert!(matches!(panel_state(&failed), PanelState::Install(_)));

    for stage in [InstallStage::Done, InstallStage::Cancelled] {
        let model = Download::from_snapshot(
            &snapshot(stage),
            Category::All,
            &InstallDraft::default(),
            Lang::En.catalog(),
        );
        assert!(model.install.is_none(), "{stage:?} 之后面板要让位");
    }
}
