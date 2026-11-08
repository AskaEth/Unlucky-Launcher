//! 内容库视图模型的守门测试（M109）：四态判定、兼容性判定、装入闸门。
//!
//! 全部是纯函数级断言（不开图形环境）：`status_of` / `blockers_of` / `Content::can_install`
//! 是页面「该画什么、按钮能不能按」的唯一判据，钉住它们比截图更早期。

use super::*;
use unlucky_i18n::Lang;
use unlucky_presenter::dto::{
    ContentDetail, ContentFailure, ContentFailureKind, ContentHit, ContentKind, ContentQuery,
    ContentResults, ContentSource, ContentVersionView, InstanceRuntime, LoaderKind,
};

fn hit(versions: &[&str], loaders: &[&str]) -> ContentHit {
    ContentHit {
        id: "p1".to_string(),
        slug: "p".to_string(),
        title: "Sodium".to_string(),
        summary: "fast".to_string(),
        author: "jelly".to_string(),
        downloads: 10,
        updated: "2026-01-01".to_string(),
        source: ContentSource::Modrinth,
        game_versions: versions.iter().map(|item| (*item).to_string()).collect(),
        loaders: loaders.iter().map(|item| (*item).to_string()).collect(),
        icon: None,
    }
}

fn targets() -> Vec<Target> {
    vec![
        Target {
            id: "fabric-1.20.1".to_string(),
            name: "Survival".to_string(),
            mc_version: "1.20.1".to_string(),
            loader: LoaderKind::Fabric,
        },
        Target {
            id: "forge-1.21.1".to_string(),
            name: "Modded".to_string(),
            mc_version: "1.21.1".to_string(),
            loader: LoaderKind::Forge,
        },
    ]
}

fn blockers_of_hit(hit: &ContentHit, kind: ContentKind, target: Option<&str>) -> Vec<Blocker> {
    blockers_of(
        Some(HitFacts {
            versions: hit.game_versions.as_slice(),
            loaders: hit.loaders.as_slice(),
            source: hit.source,
        }),
        kind,
        &targets(),
        target,
    )
}

#[test]
fn a_matching_project_has_no_blockers() {
    let hit = hit(&["1.20.1"], &["fabric"]);
    assert!(blockers_of_hit(&hit, ContentKind::Mod, Some("fabric-1.20.1")).is_empty());
}

#[test]
fn a_version_mismatch_is_reported_before_the_user_clicks() {
    // 抓得住：把「版本对不上」留到点了才报错（任务书第 3 条禁止的正是这个）。
    let hit = hit(&["1.20.1"], &["fabric"]);
    let blockers = blockers_of_hit(&hit, ContentKind::Mod, Some("forge-1.21.1"));
    assert!(blockers.contains(&Blocker::Version {
        needs: "1.20.1".to_string(),
        actual: "1.21.1".to_string()
    }));
    assert!(blockers.contains(&Blocker::Loader {
        loader: LoaderKind::Forge
    }));
}

#[test]
fn both_mismatches_are_reported_separately() {
    // 合成一条会在「都不兼容」时只说一半原因
    let hit = hit(&["1.20.1"], &["fabric"]);
    assert_eq!(
        blockers_of_hit(&hit, ContentKind::Mod, Some("forge-1.21.1")).len(),
        2
    );
}

#[test]
fn site_data_that_is_missing_never_blocks() {
    // 站点没给版本/加载器 = 未知，不是「不支持」
    let hit = hit(&[], &[]);
    assert!(blockers_of_hit(&hit, ContentKind::Mod, Some("forge-1.21.1")).is_empty());
}

#[test]
fn a_curseforge_project_is_blocked_because_its_file_endpoint_is_not_wired() {
    // 复审 P1-1：检索通了、文件端点没通 ⇒ 必须在**点之前**禁用并说明，
    // 否则「切到 CF 源后每条都必然失败，UI 一点提示都没有」。
    let mut cf = hit(&["1.20.1"], &["fabric"]);
    cf.source = ContentSource::Curseforge;
    assert_eq!(
        blockers_of_hit(&cf, ContentKind::Mod, Some("fabric-1.20.1")),
        vec![Blocker::SourceNotWired]
    );
    // 同样的版本/加载器，Modrinth 那条就没有这条原因（否则等于把整页禁死）
    let modrinth = hit(&["1.20.1"], &["fabric"]);
    assert!(blockers_of_hit(&modrinth, ContentKind::Mod, Some("fabric-1.20.1")).is_empty());
}

#[test]
fn modpacks_are_blocked_by_the_missing_pipeline_not_by_compatibility() {
    let hit = hit(&["1.20.1"], &["fabric"]);
    assert_eq!(
        blockers_of_hit(&hit, ContentKind::Modpack, Some("fabric-1.20.1")),
        vec![Blocker::NeedsPipeline]
    );
    // 资源包 / 光影 / Mod 都没有这一条
    for kind in [
        ContentKind::Mod,
        ContentKind::Resourcepack,
        ContentKind::Shader,
    ] {
        assert!(blockers_of_hit(&hit, kind, Some("fabric-1.20.1")).is_empty());
    }
}

#[test]
fn no_target_instance_disables_the_button_with_a_reason() {
    assert_eq!(
        blockers_of(None, ContentKind::Mod, &[], None),
        vec![Blocker::NoTarget]
    );
    // 有实例但选中项不在名单里（被删了）：同样不许静默通过
    assert_eq!(
        blockers_of(None, ContentKind::Mod, &targets(), Some("gone")),
        vec![Blocker::NoTarget]
    );
    // 没选中项目：只剩「目标」与「整包」这两类先天原因
    assert!(blockers_of(None, ContentKind::Mod, &targets(), Some("fabric-1.20.1")).is_empty());
}

fn state_with(results: Option<ContentResults>, pending: bool) -> AppState {
    let mut state = AppState::new();
    state.instances = Vec::new();
    state.content.pending = pending;
    state.content.results = results;
    state
}

fn results(hits: Vec<ContentHit>, offset: u32) -> ContentResults {
    let mut query = ContentQuery::first(ContentKind::Mod);
    query.offset = offset;
    ContentResults {
        query,
        hits,
        total: 100,
        curseforge: false,
    }
}

#[test]
fn the_four_states_are_derived_from_three_fields() {
    let page = PageState::default();
    let catalog = Lang::En.catalog();

    // 还没有结果 ⇒ 加载
    let loading = Content::from_snapshot(&state_with(None, false), &page, catalog);
    assert!(loading.is_loading());
    assert_eq!(loading.status, Status::Loading);

    // 请求在飞 ⇒ 加载（即使手里还有上一批）
    let pending = Content::from_snapshot(
        &state_with(Some(results(vec![hit(&["1.20.1"], &["fabric"])], 0)), true),
        &page,
        catalog,
    );
    assert_eq!(pending.status, Status::Loading);

    // 拉到了但零命中 ⇒ 空
    let empty = Content::from_snapshot(
        &state_with(Some(results(Vec::new(), 0)), false),
        &page,
        catalog,
    );
    assert_eq!(empty.status, Status::Empty);

    // 有命中 ⇒ 有数据
    let ready = Content::from_snapshot(
        &state_with(Some(results(vec![hit(&["1.20.1"], &["fabric"])], 0)), false),
        &page,
        catalog,
    );
    assert_eq!(ready.status, Status::Ready);
    assert_eq!(ready.hits.len(), 1);
    assert_eq!(ready.hits[0].title, "Sodium");
    assert!(!ready.has_more, "只有 1 条、不够一页 ⇒ 没有下一页");
}

#[test]
fn a_failure_outranks_the_loading_state() {
    let mut state = state_with(None, true);
    state.content.failed = Some(ContentFailure::new(ContentFailureKind::Server, "站点 503"));
    let model = Content::from_snapshot(&state, &PageState::default(), Lang::En.catalog());
    assert_eq!(model.status, Status::Failed("站点 503".to_string()));
    assert!(model.status.failed());
    assert!(!model.status.offline());
}

#[test]
fn no_network_is_its_own_state_not_an_empty_list() {
    // M162：无网必须画「未联网」，不能与「没有匹配的内容」共用一个空态。
    let mut state = state_with(None, false);
    state.content.results = Some(results(Vec::new(), 0));
    state.content.failed = Some(ContentFailure::offline("connect timed out"));
    let model = Content::from_snapshot(&state, &PageState::default(), Lang::En.catalog());
    assert_eq!(
        model.status,
        Status::Offline("connect timed out".to_string())
    );
    assert!(model.status.offline());
    assert!(model.status.failed());
    assert_ne!(model.status, Status::Empty);
}

#[test]
fn versions_come_from_the_detail_channel_of_the_selected_project() {
    let mut state = state_with(Some(results(vec![hit(&["1.20.1"], &["fabric"])], 0)), false);
    let mut page = PageState {
        selected: Some("p1".to_string()),
        ..PageState::default()
    };
    // 详情还没到：说明「正在取」，不是「一个版本都没有」
    state.content.request_detail("p1");
    let model = Content::from_snapshot(&state, &page, Lang::En.catalog());
    assert!(model.available_versions.is_empty());
    assert!(model.versions_note.is_some());

    state.content.apply_detail(
        "p1",
        ContentDetail {
            project: "p1".to_string(),
            title: "Sodium".to_string(),
            description: "fast".to_string(),
            body: String::new(),
            versions: vec![ContentVersionView {
                id: "v1".to_string(),
                name: "Sodium 0.6.0".to_string(),
                number: "mc1.20.1-0.6.0".to_string(),
                kind: "release".to_string(),
                game_versions: vec!["1.20.1".to_string()],
                loaders: vec!["fabric".to_string()],
                published: "2026-02-01T00:00:00Z".to_string(),
                file_name: Some("sodium.jar".to_string()),
                file_size: Some(2048),
            }],
        },
    );
    let model = Content::from_snapshot(&state, &page, Lang::En.catalog());
    assert_eq!(model.available_versions.len(), 1);
    assert_eq!(model.available_versions[0].number, "mc1.20.1-0.6.0");
    assert!(model.available_versions[0].line.contains("mc1.20.1-0.6.0"));
    assert!(model.versions_note.is_none());

    // 换了选中项：上一个项目的版本列表不许画在这个项目下面
    page.selected = Some("p2".to_string());
    let model = Content::from_snapshot(&state, &page, Lang::En.catalog());
    assert!(model.available_versions.is_empty());
}

#[test]
fn can_install_needs_a_selection_and_no_blockers() {
    let mut state = state_with(Some(results(vec![hit(&["1.20.1"], &["fabric"])], 0)), false);
    state.instances = vec![unlucky_presenter::dto::InstanceSummary {
        id: "fabric-1.20.1".to_string(),
        name: "Survival".to_string(),
        mc_version: "1.20.1".to_string(),
        loader: unlucky_presenter::dto::LoaderInfo {
            kind: LoaderKind::Fabric,
            version: "0.16.5".to_string(),
        },
        last_played: None,
        runtime: InstanceRuntime::Idle,
        icon: None,
        isolation: "isolated".to_string(),
        version_id: "1.20.1".to_string(),
        version_present: true,
        launch: unlucky_presenter::dto::LaunchArgsView::default(),
    }];
    let catalog = Lang::En.catalog();
    let page = PageState {
        kind: ContentKind::Mod,
        selected: None,
        target: None,
        draft: String::new(),
        pack: super::super::packui::PackPageState::default(),
    };

    // 没选中项目 ⇒ 不能装（也不该说「成功」）
    let no_selection = Content::from_snapshot(&state, &page, catalog);
    assert!(!no_selection.can_install());
    assert_eq!(
        no_selection.target.as_deref(),
        Some("fabric-1.20.1"),
        "只有一个实例就默认选它"
    );

    // 选中兼容项 ⇒ 能装
    let page = PageState {
        selected: Some("p1".to_string()),
        ..page
    };
    let compatible = Content::from_snapshot(&state, &page, catalog);
    assert!(compatible.can_install());
    assert!(compatible.blocker_reasons(catalog).is_empty());
    assert!(compatible.target_label.is_some());
}

#[test]
fn the_panel_only_shows_while_an_install_is_unfinished() {
    use unlucky_presenter::dto::{InstallProgressView, InstallStage};

    let mut state = state_with(None, false);
    state.install = Some(InstallProgressView {
        version: "Sodium".to_string(),
        stage: InstallStage::Downloading,
        files_total: 1,
        ..InstallProgressView::new()
    });
    let running = Content::from_snapshot(&state, &PageState::default(), Lang::En.catalog());
    assert!(running.panel.is_some(), "下载中要有面板");

    state.install = Some(InstallProgressView {
        stage: InstallStage::Done,
        ..InstallProgressView::new()
    });
    let done = Content::from_snapshot(&state, &PageState::default(), Lang::En.catalog());
    assert!(done.panel.is_none(), "装完的面板不该钉在页面上");
}

#[test]
fn loader_names_are_ascii_so_the_english_baseline_stays_english() {
    for loader in [
        LoaderKind::Vanilla,
        LoaderKind::Fabric,
        LoaderKind::Forge,
        LoaderKind::NeoForge,
        LoaderKind::Quilt,
    ] {
        assert!(loader_name(loader).is_ascii(), "{loader:?}");
    }
    assert_eq!(
        loader_name(LoaderKind::Vanilla),
        "Vanilla",
        "不能画「原版」"
    );
}
