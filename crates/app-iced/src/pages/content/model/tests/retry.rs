//! M191：安装失败的两档分组 + 「只重试这些」（`super` 按主题分卷，父文件已贴 400 行硬线）。
//!
//! 三条变异守门：两档不许合并 / 全成功不许渲染 / 重试只针对失败那次的实例。

use super::super::{Content, PageState, RetryPanel};
use super::{hit, results, state_with};

use unlucky_i18n::{Key, Lang};
use unlucky_presenter::dto::{
    InstallFailureKind, InstallFailureView, InstallProgressView, InstallStage,
};

/// 一份「两档都有」的失败进度快照。
fn failed_progress() -> unlucky_presenter::dto::InstallProgressView {
    let mut progress = InstallProgressView {
        version: "Sodium".to_string(),
        instance: "survival".to_string(),
        stage: InstallStage::Failed,
        files_done: 1,
        files_total: 4,
        ..InstallProgressView::new()
    };
    // 三种可显示性各一条（与 `plan::label_of` 的三种情况同形）：
    // 根项目用请求里的标题、下载失败用文件名、拿不到名的只画 id。
    progress.retry = vec![
        InstallFailureView::new(
            "AANobbMI",
            "Sodium",
            InstallFailureKind::Unreachable,
            "连不上 CDN",
        ),
        InstallFailureView::new(
            "P7dR8mSH",
            "Entity Culling",
            InstallFailureKind::Incompatible,
            "没有匹配的版本",
        ),
        InstallFailureView::new("1eAoo2KR", "", InstallFailureKind::Unreachable, "HTTP 404"),
    ];
    progress
}

/// 两档**分组**画，各有自己的标题；条目按档分开，不混成一段。
///
/// 变异守门②：把两档合并（`kind_of` 都映成 `Unreachable`，或 `title_of` 都返回同一个 key）
/// ⇒ `groups.len()` 变 1 / 两个 title 相同 ⇒ 这条红。
#[test]
fn the_two_tiers_are_two_groups_with_two_labels() {
    let panel = RetryPanel::of(&failed_progress(), Lang::En.catalog()).expect("有失败项就有面板");
    assert_eq!(panel.total, 3);
    assert!(panel.has_both_tiers(), "{:?}", panel.groups);
    // **P2-1 的守门**：有名字的画「名字 (id)」，没有名字的才只画 id
    assert_eq!(
        panel.items_of(InstallFailureKind::Incompatible),
        Some(["Entity Culling (P7dR8mSH)".to_string()].as_slice())
    );
    assert_eq!(
        panel.items_of(InstallFailureKind::Unreachable),
        Some(["Sodium (AANobbMI)".to_string(), "1eAoo2KR".to_string()].as_slice())
    );
    // 变异守门：把标签置空 ⇒ 这一行退回裸 id，上面两条立刻红
    assert!(
        panel
            .items_of(InstallFailureKind::Unreachable)
            .is_some_and(|items| items[0].starts_with("Sodium")),
        "根项目必须显示请求里的标题，不是 id"
    );
    let titles: Vec<Key> = panel.groups.iter().map(|group| group.title).collect();
    assert_ne!(titles[0], titles[1], "两档的标题 key 不许是同一个");
    // 文案来自词表（不是硬编码中文）：英文目录下这两句必须不同且不含中文
    let incompatible = Lang::En.catalog().text(titles[0]);
    let unreachable = Lang::En.catalog().text(titles[1]);
    assert_ne!(incompatible, unreachable);
    assert!(incompatible.is_ascii(), "{incompatible}");
    assert!(unreachable.is_ascii(), "{unreachable}");
    // 按钮文字带条数，也来自词表
    assert!(panel.action.contains('3'), "{}", panel.action);
    assert!(panel.action.is_ascii(), "{}", panel.action);
}

/// 只有一档失败时只画一段（空组不出现——空的「取不到」标题会让用户以为那里也坏了）。
#[test]
fn an_empty_tier_does_not_get_a_group() {
    let mut progress = InstallProgressView {
        stage: InstallStage::Failed,
        ..InstallProgressView::new()
    };
    progress.retry = vec![InstallFailureView::new(
        "P7dR8mSH",
        "Entity Culling",
        InstallFailureKind::Incompatible,
        "没有匹配的版本",
    )];
    let panel = RetryPanel::of(&progress, Lang::En.catalog()).expect("有失败项就有面板");
    assert!(!panel.has_both_tiers());
    assert!(panel.items_of(InstallFailureKind::Unreachable).is_none());
}

/// **全成功时不渲染**：`Done` 且没有失败项 ⇒ 面板不存在（不是置灰，是连构造都没有）。
///
/// 变异守门③：把 `RetryPanel::of` 的判据去掉（恒返回 `Some`）⇒ 这条红。
#[test]
fn a_fully_successful_install_has_no_retry_surface_at_all() {
    let done = InstallProgressView {
        version: "Sodium".to_string(),
        instance: "survival".to_string(),
        stage: InstallStage::Done,
        files_done: 4,
        files_total: 4,
        ..InstallProgressView::new()
    };
    assert!(RetryPanel::of(&done, Lang::En.catalog()).is_none());
    // 页面上也一样：状态是 Done ⇒ 页底那块既没有重试面板，也没有失败面板
    let mut state = state_with(Some(results(vec![hit(&["1.20.1"], &["fabric"])], 0)), false);
    state.install = Some(done);
    let model = Content::from_snapshot(&state, &PageState::default(), Lang::En.catalog());
    assert!(model.retry.is_none(), "全成功还在页面上画重试面板");
    assert!(model.panel.is_none());
    assert!(model.retry_install_request().is_none());

    // 判据是**两半**：`Failed` **且** 名单非空。第一半单独钉住——就算快照里残留了名单
    // （不该发生，host 成功时会清），`Done` 也不画（r1 复审指出：只去掉这一半曾全绿）。
    let mut stale = InstallProgressView {
        stage: InstallStage::Done,
        ..InstallProgressView::new()
    };
    stale.retry = failed_progress().retry;
    assert!(
        RetryPanel::of(&stale, Lang::En.catalog()).is_none(),
        "Done 态不许画重试面板（判据是「失败且有名单」，不是「有名单」）"
    );

    // 第二半：失败但没有名单（例如整合包那条本地失败）同样不画：没有可重试的东西
    let mut failed = InstallProgressView {
        stage: InstallStage::Failed,
        ..InstallProgressView::new()
    };
    failed.retry.clear();
    assert!(RetryPanel::of(&failed, Lang::En.catalog()).is_none());
}

/// 「只重试这些」发出去的是**失败那次的实例** + `retry: true`（不是页面此刻选中的实例，
/// 也不是一次新的根项目安装）。
#[test]
fn the_retry_action_targets_the_failed_attempt() {
    let mut state = state_with(Some(results(vec![hit(&["1.20.1"], &["fabric"])], 0)), false);
    // 名单要有，否则这块入口根本不存在
    state.install = Some(failed_progress());

    let model = Content::from_snapshot(&state, &PageState::default(), Lang::En.catalog());
    assert!(model.retry.is_some());
    let request = model.retry_install_request().expect("有面板就该有请求");
    assert!(
        request.retry,
        "重试必须是 retry: true（否则 host 会从根解起）"
    );
    assert_eq!(request.instance, "survival");
}
