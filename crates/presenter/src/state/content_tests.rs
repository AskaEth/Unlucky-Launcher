//! 内容库状态机的守门测试（M109；复审 P1-2 的补课）。
//!
//! 从 `state.rs` 拆出（那个文件贴 400 行硬上限）。钉的是**翻页判据**：
//! `has_more` 必须读「最近一页是否装满」，不能读累计条数——否则第 2 页之后恒真。

use crate::dto::{
    AppEvent, ContentDetail, ContentFailure, ContentFailureKind, ContentHit, ContentKind,
    ContentQuery, ContentResults, ContentSource, ContentVersionView, CoreEvent,
};

use super::AppState;

fn hit(index: usize) -> ContentHit {
    ContentHit {
        id: format!("p{index}"),
        slug: format!("p{index}"),
        title: format!("Project {index}"),
        summary: "s".to_string(),
        author: "a".to_string(),
        downloads: 0,
        updated: "2026-01-01".to_string(),
        source: ContentSource::Modrinth,
        game_versions: vec!["1.20.1".to_string()],
        loaders: vec!["fabric".to_string()],
        icon: None,
    }
}

/// 造一页结果：`offset` 是游标，`count` 是这一页实际回来几条。
fn page(offset: u32, count: u32) -> ContentResults {
    let mut query = ContentQuery::first(ContentKind::Mod);
    query.offset = offset;
    ContentResults {
        query,
        hits: (0..count)
            .map(|index| hit(offset as usize + index as usize))
            .collect(),
        total: 999,
        curseforge: false,
    }
}

fn apply(state: &mut AppState, results: ContentResults) {
    state.apply(AppEvent::Core(CoreEvent::ContentResults(Box::new(results))));
}

/// 复审 P1-2 的第一条：装满一页 ⇒ 还有更多；下一（末）页不满 ⇒ 没有了。
#[test]
fn a_short_last_page_clears_has_more() {
    let mut state = AppState::new();
    let limit = ContentQuery::first(ContentKind::Mod).limit;

    apply(&mut state, page(0, limit));
    assert!(state.content.last_page_filled, "首页装满 ⇒ 还有下一页");
    assert_eq!(
        state.content.results.as_ref().map_or(0, |r| r.hits.len()),
        limit as usize
    );

    // 末页只回来 3 条：累计 23 条「够一页了」，但**这一页**没装满 ⇒ 没有更多
    apply(&mut state, page(limit, 3));
    assert!(
        !state.content.last_page_filled,
        "末页不满 ⇒ 不许说还有更多（判据不能读累计条数）"
    );
    assert_eq!(
        state.content.results.as_ref().map_or(0, |r| r.hits.len()),
        limit as usize + 3,
        "命中项仍然要累加"
    );
}

/// 复审 P1-2 的第二条：再点一次，回来的空页仍然是「没有更多」。
#[test]
fn an_empty_last_page_keeps_has_more_false() {
    let mut state = AppState::new();
    let limit = ContentQuery::first(ContentKind::Mod).limit;

    apply(&mut state, page(0, limit));
    apply(&mut state, page(limit, 0));
    assert!(!state.content.last_page_filled, "空页 ⇒ 没有更多");

    // 用户又点了一次「加载更多」：空页不会把状态翻回真
    apply(&mut state, page(limit + limit, 0));
    assert!(
        !state.content.last_page_filled,
        "再点一次空页 ⇒ 仍然没有更多"
    );
    assert_eq!(
        state.content.results.as_ref().map_or(0, |r| r.hits.len()),
        limit as usize,
        "空页不改变列表内容"
    );
}

/// 换条件（offset 回到 0）时把「最近一页装满」清掉：不然新条件的第一帧就会画「没有更多了」。
#[test]
fn changing_the_query_resets_the_page_flag() {
    let mut state = AppState::new();
    let limit = ContentQuery::first(ContentKind::Mod).limit;

    apply(&mut state, page(0, limit));
    assert!(state.content.last_page_filled);

    let mut query = ContentQuery::first(ContentKind::Mod);
    query.text = "sodium".to_string();
    state.set_content_query(query);
    assert!(!state.content.last_page_filled, "换条件 ⇒ 清掉上一批的判据");
    assert!(state.content.pending);
    assert!(
        state.content.results.is_none(),
        "换条件要清列表（不然加载态画旧命中项）"
    );
}

/// 迟到的一批不许改写判据（用户已经换了条件）。
#[test]
fn a_stale_batch_does_not_touch_the_page_flag() {
    let mut state = AppState::new();
    let limit = ContentQuery::first(ContentKind::Mod).limit;

    apply(&mut state, page(0, limit));
    let mut query = ContentQuery::first(ContentKind::Mod);
    query.text = "sodium".to_string();
    state.set_content_query(query);

    // 旧条件的一页这时才回来：既不改结果，也不改「还有更多」
    apply(&mut state, page(limit, 5));
    assert!(!state.content.last_page_filled);
    assert!(state.content.results.is_none(), "迟到的一批被丢掉");
}

// ------------------------------------------------ M162：失败大类与详情通道

/// 事件 → 状态：`ContentFailed` 带大类，没联网时 `is_offline()` 为真（页面据此画离线态）。
#[test]
fn a_content_failure_keeps_its_kind() {
    let mut state = AppState::new();
    state.set_content_query(ContentQuery::first(ContentKind::Mod));
    state.apply_core(CoreEvent::ContentFailed(Box::new(ContentFailure::offline(
        "connect timed out",
    ))));
    let failure = state.content.failed.as_ref().expect("要记住失败");
    assert!(failure.is_offline(), "没联网必须能被页面认出来");
    assert!(!state.content.pending);

    state.apply_core(CoreEvent::ContentFailed(Box::new(ContentFailure::new(
        ContentFailureKind::Server,
        "HTTP 503",
    ))));
    let failure = state.content.failed.as_ref().expect("要记住失败");
    assert!(!failure.is_offline(), "站点 5xx 不是「没联网」");
}

/// 详情通道：请求态 → 结果态；换项目时旧详情先清掉。
#[test]
fn the_detail_channel_replaces_the_previous_project() {
    let mut state = AppState::new();
    state.content.request_detail("p1");
    assert!(state.content.detail_pending);
    state.apply_core(CoreEvent::ContentDetail {
        project: "p1".to_string(),
        result: Box::new(Ok(detail("p1"))),
    });
    assert!(!state.content.detail_pending);
    assert_eq!(
        state.content.detail.as_ref().map(|d| d.project.as_str()),
        Some("p1")
    );

    state.content.request_detail("p2");
    assert!(
        state.content.detail.is_none(),
        "换了项目就不能留着上一个的版本列表"
    );
    state.apply_core(CoreEvent::ContentDetail {
        project: "p2".to_string(),
        result: Box::new(Err(ContentFailure::offline("connect timed out"))),
    });
    assert!(state
        .content
        .detail_failed
        .as_ref()
        .is_some_and(ContentFailure::is_offline));
}

fn detail(project: &str) -> ContentDetail {
    ContentDetail {
        project: project.to_string(),
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
    }
}

/// r1 P2-3：迟到的详情结果（A）不许写进 B 的请求/面板。
#[test]
fn a_late_detail_result_for_the_previous_project_is_dropped() {
    let mut state = AppState::new();
    state.content.request_detail("p1");
    // 用户立刻切到 p2：新的请求在飞
    state.content.request_detail("p2");
    // p1 的失败这时才回来
    state.apply_core(CoreEvent::ContentDetail {
        project: "p1".to_string(),
        result: Box::new(Err(ContentFailure::offline("connect timed out"))),
    });
    assert!(
        state.content.detail_failed.is_none(),
        "A 的失败不许画在 B 的面板上"
    );
    assert!(state.content.detail_pending, "B 仍应在飞");
    assert_eq!(state.content.detail_requested.as_deref(), Some("p2"));

    // p1 的成功结果同样被丢掉（哪怕载荷里写着 p1）
    state.apply_core(CoreEvent::ContentDetail {
        project: "p1".to_string(),
        result: Box::new(Ok(detail("p1"))),
    });
    assert!(state.content.detail.is_none());
    assert!(state.content.detail_pending);

    // 真正属于 B 的结果才落地
    state.apply_core(CoreEvent::ContentDetail {
        project: "p2".to_string(),
        result: Box::new(Ok(detail("p2"))),
    });
    assert!(!state.content.detail_pending);
    assert_eq!(
        state.content.detail.as_ref().map(|d| d.project.as_str()),
        Some("p2")
    );
}
