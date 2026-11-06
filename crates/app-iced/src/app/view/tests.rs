//! `app/view.rs` 的守门测试（rebase 到 5adac23 之后 view.rs 贴 400 行上限，按「测试独立成文件」拆出）。

use super::*;
use unlucky_i18n::Key;
use unlucky_presenter::dto::{
    InstanceRuntime, InstanceSummary, LaunchStage, LoaderInfo, LoaderKind, Progress,
};
use unlucky_presenter::mock_data::mock_instances;

fn instance(runtime: InstanceRuntime) -> InstanceSummary {
    InstanceSummary {
        id: "survival".to_string(),
        name: "Survival".to_string(),
        mc_version: "1.20.4".to_string(),
        loader: LoaderInfo {
            kind: LoaderKind::Fabric,
            version: "0.16.5".to_string(),
        },
        last_played: Some("2026-10-29T19:22:00Z".to_string()),
        runtime,
        icon: None,
        isolation: "isolated".to_string(),
        version_id: "1.20.4".to_string(),
        version_present: true,
    }
}

/// **取证不变量（M100）**：`ForcedState` 的三档在产品路径上必须映射成「页面控件怎么画」。
///
/// 抓得住：① 谁把 `Rest` 映射成 Hovered/Pressed（那会让 `--screenshot` 的默认值反而画出状态层，
/// 与「取证密闭」正好相反，而且图看起来完全正常）；② 加了变体却映射到别的态这种复制粘贴错。
/// 抓不住：`shell::overridden`（标题栏 / 导航轨按钮走的**另一处** match，断言在 `shell/tests.rs`），
/// 也抓不住不消费 `model.forced` 的裸 `iced::button`（设置页「外观」区就是）。
/// 测试用的薄包装：默认宽度 / 默认分类 / 空草稿 / **给定语言**。
fn at_lang(state: &AppState, forced: Option<crate::cli::ForcedState>, lang: Lang) -> pages::Model {
    let draft = pages::InstallDraft::default();
    let page = pages::content::PageState::default();
    model(
        state,
        1180.0,
        forced,
        pages::Category::All,
        &draft,
        lang,
        &page,
    )
}

/// 同 [`at_lang`]，语言取英文（绝大多数断言只关心「快照 → 模型」的映射，不关心译文）。
fn at(state: &AppState, forced: Option<crate::cli::ForcedState>) -> pages::Model {
    at_lang(state, forced, Lang::En)
}

#[test]
fn the_forced_state_maps_to_the_expected_interaction() {
    let state = AppState::new();
    let forced = |forced| at(&state, Some(forced)).forced;

    assert_eq!(
        forced(crate::cli::ForcedState::Rest),
        Some(Interaction::Enabled),
        "Rest = 静息态（压掉系统光标 hover）"
    );
    assert_eq!(
        forced(crate::cli::ForcedState::Hover),
        Some(Interaction::Hovered)
    );
    assert_eq!(
        forced(crate::cli::ForcedState::Pressed),
        Some(Interaction::Pressed)
    );
    assert_eq!(at(&state, None).forced, None);
}

#[test]
fn the_snapshot_becomes_the_page_model() {
    let mut state = AppState::new();
    state.apply(unlucky_presenter::dto::AppEvent::Core(
        unlucky_presenter::dto::CoreEvent::InstancesChanged(mock_instances()),
    ));
    state.navigate(Route::Detail("survival".to_string()));

    let model = at(&state, None);
    assert_eq!(model.screen, pages::Screen::Instances);
    assert_eq!(model.active, Some(pages::Screen::Instances.nav()));
    assert_eq!(model.window_width, 1180.0);
    assert_eq!(model.instances.rows.len(), 4, "mock 的四个实例");
    assert_eq!(model.instances.rows[0].id, "survival");

    let detail = model.instances.detail.expect("路由是 Detail，要有详情");
    assert_eq!(detail.id, "survival");
    assert_eq!(detail.meta.len(), 6);
    assert!(detail.can_launch, "空闲实例可启动");
    assert!(!detail.can_stop);
    assert!(detail.note.is_none(), "真快照没有正文段");
}

#[test]
fn a_busy_instance_offers_stop_instead_of_play() {
    let mut state = AppState::new();
    state.apply(unlucky_presenter::dto::AppEvent::Core(
        unlucky_presenter::dto::CoreEvent::InstancesChanged(vec![instance(
            InstanceRuntime::Busy {
                stage: LaunchStage::Download,
                progress: Progress {
                    done: 750,
                    total: 1_500,
                },
            },
        )]),
    ));
    state.navigate(Route::Detail("survival".to_string()));

    let model = at(&state, None);
    assert_eq!(
        model.instances.rows[0].status.as_deref(),
        Some("Downloading · 50%")
    );
    let detail = model.instances.detail.expect("有详情");
    assert!(!detail.can_launch, "启动中不许再点 Play（不做乐观回显）");
    assert!(detail.can_stop, "启动中要能 Stop");
}

#[test]
fn failed_instances_can_be_retried() {
    let failed = instance(InstanceRuntime::Failed {
        reason: "exit 1".to_string(),
    });
    assert_eq!(
        runtime_status(&failed.runtime, Lang::En.catalog()).as_deref(),
        Some("Launch failed · exit 1")
    );
    assert!(
        detail_of(&failed, Lang::En.catalog()).can_launch,
        "失败后要能重试"
    );
}

#[test]
fn timestamps_are_sliced_not_guessed() {
    let en = Lang::En.catalog();
    assert_eq!(
        last_played(Some("2026-10-29T19:22:00Z"), en),
        "2026-10-29 19:22 UTC"
    );
    assert_eq!(last_played(None, en), "Never");
    // 短串 / 非 ASCII 原样返回（不 panic）
    assert_eq!(last_played(Some("2026-10"), en), "2026-10");
    assert_eq!(last_played(Some("昨天"), en), "昨天");
    // 「从未游玩」是唯一走词表的那一支
    assert_eq!(last_played(None, Lang::ZhCn.catalog()), "从未游玩");
}

/// **快照 → 页面文案跟着 `App::lang` 走**（M120 的核心接线断言，模型这一层）。
///
/// 抓得住：`view::model` 又把语言写死成 `Lang::En`（M106 之前的原样）、阶段/元信息标签还留着
/// 英文基线字面量、原版徽标直接画 presenter 的中文「原版」。这三种都不会让任何别的测试红。
/// 抓不住：页面把 key 画没画上屏（那是 `pages/instances/tests/i18n.rs` 的取样口 + 出图）。
#[test]
fn the_runtime_lines_and_meta_labels_follow_the_language() {
    let mut state = AppState::new();
    state.apply(unlucky_presenter::dto::AppEvent::Core(
        unlucky_presenter::dto::CoreEvent::InstancesChanged(vec![
            instance(InstanceRuntime::Running { pid: 1234 }),
            instance(InstanceRuntime::Busy {
                stage: LaunchStage::Verify,
                progress: Progress {
                    done: 750,
                    total: 1_500,
                },
            }),
        ]),
    ));
    state.navigate(Route::Detail("survival".to_string()));

    let en = at_lang(&state, None, Lang::En);
    let zh = at_lang(&state, None, Lang::ZhCn);

    // 列表 overline：中英各一份，数字与分隔符不动
    assert_eq!(
        en.instances.rows[0].status.as_deref(),
        Some("Running · pid 1234")
    );
    assert_eq!(
        zh.instances.rows[0].status.as_deref(),
        Some("运行中 · pid 1234")
    );
    assert_eq!(
        en.instances.rows[1].status.as_deref(),
        Some("Verifying · 50%")
    );
    assert_eq!(zh.instances.rows[1].status.as_deref(), Some("校验中 · 50%"));

    // 元信息卡的标签与值
    let (en_detail, zh_detail) = (
        en.instances.detail.expect("有详情"),
        zh.instances.detail.expect("有详情"),
    );
    let labels = |detail: &pages::Detail| -> Vec<String> {
        detail.meta.iter().map(|cell| cell.label.clone()).collect()
    };
    assert_eq!(
        labels(&en_detail),
        vec![
            "VERSION",
            "LOADER",
            "ISOLATION",
            "VERSION ID",
            "FILES",
            "LAST PLAYED"
        ]
    );
    assert_eq!(
        labels(&zh_detail),
        vec!["版本", "加载器", "隔离方式", "版本标识", "文件", "上次游玩"]
    );

    // 值里只有「是否在磁盘上」与「上次游玩」是可翻译的；版本号 / 隔离方式 / 版本标识是数据
    // 隔离档位是 core 的稳定 id（`isolated` / `shared`），也要换成人话；认不出来的原样上屏
    assert_eq!(en_detail.meta[2].value, "Isolated");
    assert_eq!(zh_detail.meta[2].value, "独立目录");
    assert_eq!(en_detail.meta[4].value, "Present on disk");
    assert_eq!(zh_detail.meta[4].value, "已在磁盘上");
    assert_eq!(
        en_detail.meta[5].value, "2026-10-29 19:22 UTC",
        "时间戳是数据：只切片，不随语言变形"
    );
    assert_eq!(zh_detail.meta[5].value, "2026-10-29 19:22 UTC");

    // 详情状态行与列表 overline 同源
    assert_eq!(zh_detail.status.as_deref(), Some("运行中 · pid 1234"));
}

/// 原版徽标：`LoaderInfo::badge()` 对原版返回**中文**「原版」，英文界面里必须换回 `Vanilla`。
///
/// 抓得住：`loader_badge` 退回直接调 `badge()`（M90 ~ M119 的形状）——那样英文界面会漏中文，
/// 而中文界面又是对的，所以只有这条双语断言抓得住它。
#[test]
fn the_vanilla_badge_follows_the_language() {
    let mut bare = instance(InstanceRuntime::Idle);
    bare.loader = LoaderInfo {
        kind: LoaderKind::Vanilla,
        version: String::new(),
    };
    let mut state = AppState::new();
    state.apply(unlucky_presenter::dto::AppEvent::Core(
        unlucky_presenter::dto::CoreEvent::InstancesChanged(vec![bare]),
    ));
    state.navigate(Route::Detail("survival".to_string()));

    let en = at_lang(&state, None, Lang::En);
    let zh = at_lang(&state, None, Lang::ZhCn);
    assert_eq!(en.instances.rows[0].supporting, "1.20.4 · Vanilla");
    assert_eq!(zh.instances.rows[0].supporting, "1.20.4 · 原版");
}

/// **品牌名不许被「版本号为空」吞掉**（M120 r1 复审的 p2-1 回归）。
///
/// 背景：`host::game::loader_from_id("1.20.1-forge")` 这类 id 里取不到版本段（它按 `forge-`
/// 找，而 id 的写法是 `-forge` 结尾），于是 `LoaderInfo { kind: Forge, version: "" }`。
/// `LoaderInfo::badge()` 对这种输入给的正是品牌名 `Forge`——所以判据只能是「类型是原版」，
/// 不能是「版本号为空」。
///
/// 三个 id 各一条断言（复审点名的三档）；`Vanilla` 那一支由
/// `the_vanilla_badge_follows_the_language` 守，两条合起来把 `loader_badge` 的两侧都钉住。
///
/// 抓得住：`loader_badge` 把「非原版 + 空版本号」也送进 `Key::LoaderVanilla`
/// （r1 的真实形状：`left: "1.20.4 · Vanilla" / right: "1.20.4 · Forge"`）；
/// 抓不住：`loader_from_id` 认不出自家 `-fabric-` / `-quilt-` 口径这个**更老**的 bug
/// （在 presenter 里，另一条 mission）。
#[test]
fn the_loader_badge_keeps_the_brand_when_the_version_is_missing() {
    // (版本 id, 期望徽标) —— id 形状取自 `host::game::loader_from_id` 的三条真实输入
    let cases = [
        ("1.20.1-forge", LoaderKind::Forge, "Forge"),
        ("1.20.1-neoforge", LoaderKind::NeoForge, "NeoForge"),
        ("1.20.1-OptiFine", LoaderKind::OptiFine, "OptiFine"),
    ];

    for (id, kind, brand) in cases {
        let mut one = instance(InstanceRuntime::Idle);
        one.loader = LoaderInfo {
            kind,
            // `loader_from_id` 对这三条 id 取不到版本段 ⇒ 空串
            version: String::new(),
        };
        let mut state = AppState::new();
        state.apply(unlucky_presenter::dto::AppEvent::Core(
            unlucky_presenter::dto::CoreEvent::InstancesChanged(vec![one]),
        ));
        state.navigate(Route::Detail("survival".to_string()));

        // 品牌名两套语言都不翻译 ⇒ 中英逐字相同
        for lang in [Lang::En, Lang::ZhCn] {
            let model = at_lang(&state, None, lang);
            assert_eq!(
                model.instances.rows[0].supporting,
                format!("1.20.4 · {brand}"),
                "{id} 的徽标被显示成了别的东西（新增的判据「版本号为空 ⇒ 原版」就是这条要拦的）"
            );
            let detail = model.instances.detail.expect("有详情");
            assert_eq!(
                detail.meta[1].label,
                lang.catalog().text(Key::InstancesMetaLoader)
            );
            assert_eq!(detail.meta[1].value, brand, "{id} 的 LOADER 卡值");
        }
    }
}

#[test]
fn the_agent_route_has_no_page() {
    let mut state = AppState::new();
    state.navigate(Route::Agent);
    let model = at(&state, None);
    // Agent 没有页面：模型退回实例页的空壳，view 会走外壳回退（`None` 分支）
    assert_eq!(pages::Screen::of(state.route()), None);
    assert!(model.instances.detail.is_none());
    assert_eq!(model.active, None, "AI 页不高亮任何导航项");
}
