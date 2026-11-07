//! 版本管理页的判据：**快照 → 模型 → 控件树**三段各自的断言。
//!
//! 每条都写清「抓得住什么 / 抓不住什么」——这里只测纯转换与树能构建，**像素**由
//! `DevDocs/artifacts/m107/` 的十张图与报告的量测表负责（同 `pages/tests.rs` 的口径）。

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;
use unlucky_i18n::Lang;
use unlucky_presenter::dto::{
    AncientKind, AppEvent, CoreEvent, InstallProgressView, InstallStage, VersionCategory,
    VersionKind, VersionSummary,
};
use unlucky_presenter::state::AppState;

use crate::theme::{Families, Ui};

use super::super::layout::{frame_metrics, PaneKit};
use super::model::{Category, Download};
use super::options::InstallDraft;

mod i18n;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

fn version(id: &str, kind: VersionKind, installed: bool) -> VersionSummary {
    VersionSummary {
        id: id.to_string(),
        kind,
        released: "2026-09-20".to_string(),
        installed,
    }
}

fn state_with(versions: Vec<VersionSummary>) -> AppState {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(versions)));
    state
}

/// 一份覆盖四种版本形态的清单（`1.21.x` / `24w14a` / `rd-132211` / `a1.2.6`）。
fn mixed() -> Vec<VersionSummary> {
    vec![
        version("1.21.9", VersionKind::Release, false),
        version("1.21.8", VersionKind::Release, true),
        version("1.20.4", VersionKind::Release, true),
        version("24w14a", VersionKind::Snapshot, false),
        version(
            "rd-132211",
            VersionKind::Ancient(AncientKind::Classic),
            false,
        ),
    ]
}

// --------------------------------------------------------------- 版面

/// 页面仍是单列页（页内三栏是**页内**的事，不改 `PaneKit`）：任何宽度都不出现「页面级第二列」。
///
/// 抓得住：有人把 `Screen::Download` 的 `kit()` 改成 `ListDetail`（那会让页头与页边距跟着变）。
/// 抓不住：页内三栏的宽度（那由 `layout/rails/tests.rs` 的数值表钉住）。
#[test]
fn the_page_kit_is_still_single() {
    for width in [700.0, 900.0, 1024.0, 1180.0, 2400.0] {
        let metrics = frame_metrics(PaneKit::Single, width);
        assert!(metrics.secondary.is_none(), "{width} 出现了页面级第二列");
    }
}

// --------------------------------------------------------------- 两层侧栏（见 `tests/rails.rs`）

mod rails;

// --------------------------------------------------------------- 搜索一致性 / 复数（见 `tests/search.rs`）

mod search;

// --------------------------------------------------------------- 文案（i18n）

// --------------------------------------------------------------- 快照 → 模型

/// 真快照进得来：行数、分类、选中项、已装标记、主版本桶全部来自 `AppState`。
#[test]
fn the_snapshot_becomes_the_page_model() {
    let mut state = state_with(vec![
        version("1.21.9", VersionKind::Release, false),
        version("1.21.8", VersionKind::Release, true),
    ]);
    state.apply(AppEvent::Core(CoreEvent::InstallProgress(Some(
        InstallProgressView {
            version: "1.21.9".to_string(),
            loader: "Vanilla".to_string(),
            stage: InstallStage::Downloading,
            files_done: 128,
            files_total: 512,
            speed_bps: 1_258_291,
            eta_secs: 75,
            ..InstallProgressView::new()
        },
    ))));

    let model = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );

    assert_eq!(model.rows.len(), 2);
    assert_eq!(model.rows[0].id, "1.21.9");
    assert!(model.rows[1].installed, "快照说装了就是装了");
    assert_eq!(model.category, Category::All);
    assert_eq!(
        model.selected.as_deref(),
        Some("1.21.9"),
        "正在安装的版本 = 选中行"
    );
    assert!(model.loaded, "有版本 = 已加载");
    assert_eq!(model.buckets.len(), 1, "两个版本同一个主版本桶");
    assert_eq!(model.buckets[0].key.as_deref(), Some("1.21"));
    assert_eq!(model.hits, 2, "没有搜索词时命中数 = 行数");

    let install = model.install.expect("安装进度要转进来");
    assert!(install.running());
    assert_eq!(install.detail, "128/512 files");
}

/// 快照里的两个过滤条件（`installed_only` / `download_query`）原样照用。
#[test]
fn presenter_filters_still_apply() {
    let mut state = state_with(vec![
        version("1.21.9", VersionKind::Release, false),
        version("1.21.8", VersionKind::Release, true),
        version("1.20.4", VersionKind::Release, true),
    ]);

    let installed_only = {
        state.installed_only = true;
        Download::from_snapshot(
            &state,
            Category::All,
            &InstallDraft::default(),
            Lang::En.catalog(),
        )
    };
    assert_eq!(installed_only.rows.len(), 2, "仅已装");

    let queried = {
        state.installed_only = false;
        state.download_query = " 1.21 ".to_string();
        Download::from_snapshot(
            &state,
            Category::All,
            &InstallDraft::default(),
            Lang::En.catalog(),
        )
    };
    assert_eq!(queried.rows.len(), 2, "版本号片段匹配两个");
    assert_eq!(queried.query, "1.21", "查询词去掉首尾空格后进模型");
    assert_eq!(queried.hits, 2);
}

/// 空快照 = 加载态；过滤掉所有行 = 空态。
#[test]
fn loading_and_empty_are_different_states() {
    let loading = Download::from_snapshot(
        &AppState::new(),
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert!(loading.is_loading() && !loading.is_empty());

    let mut state = state_with(vec![version("1.21.9", VersionKind::Release, false)]);
    state.download_query = "no-such-version".to_string();
    let empty = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert!(!empty.is_loading(), "列表拉到了，不是加载中");
    assert!(empty.is_empty(), "过滤后没有行 = 空态");
    assert!(empty.no_match(), "跨类型也没有 ⇒ 没有匹配");
    assert!(!empty.only_elsewhere());
}

/// **搜索跨类型**：命中数与当前分类无关，行数相关。
///
/// 抓得住：有人把 `hits` 也算成「当前分类的命中」（那会让「正式版里搜 24w」显示 0 个匹配，
/// 用户以为没有这个版本）。
/// 抓不住：大小写之外的搜索口径（模糊匹配 / 分词属 presenter）。
#[test]
fn search_hits_are_cross_category() {
    let mut state = state_with(mixed());
    state.download_query = "1.20".to_string();

    let all = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert_eq!(all.rows.len(), 1);
    assert_eq!(all.hits, 1);

    // 同一个词，站在「预览版」分类里：行数 0，命中数仍是 1
    let snapshot = Download::from_snapshot(
        &state,
        Category::Snapshot,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert_eq!(snapshot.rows.len(), 0);
    assert_eq!(snapshot.hits, 1, "命中数是跨类型读数");
    assert!(snapshot.only_elsewhere(), "有命中但不在当前分类");
    assert!(!snapshot.no_match(), "不是「没有匹配」");
}

/// 「搜不到」与「在当前分类搜不到」是两种画面（文案与出口都不同）。
#[test]
fn the_two_search_misses_are_distinguishable() {
    let mut state = state_with(mixed());

    state.download_query = "zzz".to_string();
    let nothing = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert!(nothing.no_match() && !nothing.only_elsewhere());

    state.download_query = "rd-".to_string();
    let elsewhere = Download::from_snapshot(
        &state,
        Category::Release,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert!(elsewhere.only_elsewhere() && !elsewhere.no_match());

    let home = Download::from_snapshot(
        &state,
        Category::Ancient,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    assert_eq!(home.rows.len(), 1, "切到远古版就看见了");
    assert!(!home.no_match() && !home.only_elsewhere());
}

/// 分类反推：混合 = 全部，远古 = 远古（不看子类）。
#[test]
fn the_category_is_derived_from_the_list() {
    let mixed = vec![
        version("1.21.9", VersionKind::Release, false),
        version("1.21.9-pre3", VersionKind::Snapshot, false),
    ];
    assert_eq!(Category::of(&mixed), Category::All);
    assert_eq!(
        Category::of(&[version("x", VersionKind::Snapshot, false)]),
        Category::Snapshot
    );
    assert_eq!(
        Category::of(&[version(
            "rd-132211",
            VersionKind::Ancient(AncientKind::OldBeta),
            false
        )]),
        Category::Ancient
    );
    assert_eq!(Category::of(&[]), Category::All);
}

/// 分类 chip 派的是**真指令**。
#[test]
fn category_commands_are_real() {
    assert_eq!(Category::All.command(), None, "全部 = 画现有快照，不派指令");
    assert_eq!(Category::Release.command(), Some(VersionCategory::Release));
    assert_eq!(
        Category::Snapshot.command(),
        Some(VersionCategory::Snapshot)
    );
    assert_eq!(
        Category::Ancient.command(),
        Some(VersionCategory::Ancient(AncientKind::Classic))
    );
    assert!(Category::Ancient.matches(VersionKind::Ancient(AncientKind::Unlisted)));
    assert!(!Category::Release.matches(VersionKind::Snapshot));
}

// --------------------------------------------------------------- 控件树（见 `tests/view.rs`）

mod view;

// ------------------------------------------------- 选项区 × 三栏同屏（见 `tests/options_screen.rs`）

mod options_screen;

// ------------------------------------------------- 选项区 × 三栏的像素断言（见 `tests/options_shot.rs`）

mod options_shot;
