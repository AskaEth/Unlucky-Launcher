//! 视图模型的判据（从 `model.rs` 外置：那边加上本轮的分类过滤会顶到 400 行）。

use unlucky_presenter::dto::{AncientKind, AppEvent, CoreEvent, VersionKind, VersionSummary};
use unlucky_presenter::state::AppState;

use unlucky_i18n::Lang;

use super::super::options::InstallDraft;
use super::{Category, Download};

fn version(id: &str, kind: VersionKind) -> VersionSummary {
    VersionSummary {
        id: id.to_string(),
        kind,
        released: "2026-09-20".to_string(),
        installed: false,
    }
}

fn state_with(versions: Vec<VersionSummary>) -> AppState {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(versions)));
    state
}

/// 分类筛选是**本页的过滤**（复审 P2-1：真 host 的 `ListVersions(_)` 丢掉分类参数、回全量清单）。
///
/// 抓得住：`from_snapshot` 忘了按分类过滤——那正是「chip 点亮一下、列表照旧全量、事件回来 chip
/// 跳回『全部』」的根因；也抓得住「分类被列表反推覆盖」（`Category::of` 回到 `All`）。
/// 抓不住：真 host 是否按分类拉清单（`host/dispatch.rs:30` 丢参数，已报 finding）。
#[test]
fn the_category_filters_the_list_locally() {
    let state = state_with(vec![
        version("1.21.9", VersionKind::Release),
        version("1.21.9-pre3", VersionKind::Snapshot),
        version("rd-132211", VersionKind::Ancient(AncientKind::Classic)),
    ]);

    let release = Download::from_snapshot(
        &state,
        Category::Release,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert_eq!(
        release.category,
        Category::Release,
        "谁选的就是谁：不从列表反推（反推会把混合列表打回 All）"
    );
    assert_eq!(release.rows.len(), 1, "只剩正式版");
    assert_eq!(release.rows[0].id, "1.21.9");

    let all = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert_eq!(all.rows.len(), 3, "「全部」不过滤");
}

/// 分类把行全滤掉时是**空态**，不是加载态（两态文案与位置不同，别混）。
///
/// 抓得住：把「筛出来是空」当成「还没拉到」。
/// 抓不住：用户看到空态之后会怎么操作（那是 UI 层）。
#[test]
fn a_category_that_hides_everything_is_the_empty_state() {
    let state = state_with(vec![version("1.21.9", VersionKind::Release)]);
    let ancient = Download::from_snapshot(
        &state,
        Category::Ancient,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert!(ancient.rows.is_empty());
    assert!(!ancient.is_loading(), "快照里有版本 ⇒ 不是加载中");
    assert!(ancient.is_empty(), "筛掉所有行 ⇒ 空态");
}
