//! 控件树那一节：九个状态 × 四档宽度都要能构图（M105/M107 合流后 `tests.rs` 到 433 行，
//! 按 AGENTS.md §3-1 拆出来）。
//!
//! 只测「能构建出树」——**抓不住**画出来长什么样（像素由 `DevDocs/artifacts/m107/` 的图负责）。

use unlucky_i18n::Lang;
use unlucky_presenter::dto::{InstallProgressView, InstallStage, VersionKind};

use crate::pages::download::model::{Download, InstallView, VersionRow};
use crate::pages::download::{view, Category, Message};
use crate::pages::layout::{frame_metrics, PaneKit};

use super::{ui, version};

/// 九个状态都能构建出控件树，且四档宽度都能构建（不 panic、消息类型对得上）。
///
/// 抓得住：某个状态分支漏了返回值、或消息类型接错（编译期就红）；空态/加载态忘了给内容。
/// 抓不住：**画出来长什么样**——像素由 `DevDocs/artifacts/m107/` 的图负责。
#[test]
fn every_state_builds_a_tree_at_every_width() {
    let ui = ui();
    let catalog = Lang::En.catalog();

    let loading = Download::loading();
    let empty = Download {
        loaded: true,
        ..Download::loading()
    };
    let ready = Download {
        options: None,
        rows: vec![VersionRow::of(
            &version("1.21.9", VersionKind::Release, false),
            catalog,
        )],
        category: Category::Release,
        selected: Some("1.21.9".to_string()),
        install: None,
        loaded: true,
        // M107 的五个新字段：桶 / 查询 / 命中数 / 第二级选中 / 折叠态
        ..Download::default()
    };

    let installing = Download {
        install: Some(InstallView::of(
            &InstallProgressView {
                version: "1.21.9".to_string(),
                stage: InstallStage::Downloading,
                files_done: 128,
                files_total: 512,
                ..InstallProgressView::new()
            },
            catalog,
        )),
        ..ready.clone()
    };

    let failed = Download {
        install: Some(InstallView::of(
            &InstallProgressView {
                version: "1.21.9".to_string(),
                stage: InstallStage::Failed,
                ..InstallProgressView::new()
            },
            catalog,
        )),
        ..ready.clone()
    };

    // M107：搜索 / 第二级选中 / 搜不到 / 手动折叠四个新增状态的取样
    let searching = Download {
        query: "1.21".to_string(),
        hits: 1,
        ..ready.clone()
    };
    let majored = Download {
        selected_major: Some("1.21".to_string()),
        ..ready.clone()
    };
    let no_match = Download {
        query: "zzz".to_string(),
        hits: 0,
        ..ready.clone()
    };
    let folded = Download {
        type_rail_folded: true,
        ..ready.clone()
    };

    // 四档宽度：1180 / 1024（Wide 门槛）/ 900（Narrow）/ 700（Chips）
    for width in [1180.0, 1024.0, 900.0, 700.0] {
        let metrics = frame_metrics(PaneKit::Single, width);
        for model in [
            loading.clone(),
            empty.clone(),
            ready.clone(),
            installing.clone(),
            failed.clone(),
            searching.clone(),
            majored.clone(),
            no_match.clone(),
            folded.clone(),
        ] {
            let _: iced::Element<'_, Message> = view(&ui, metrics, &model, Lang::En);
        }
    }
}

/// 安装失败的模型：可读文案 + 出口都有（不静默）。
#[test]
fn a_failed_install_stays_visible() {
    let model = InstallView::of(
        &InstallProgressView {
            version: "1.21.9".to_string(),
            stage: InstallStage::Failed,
            ..InstallProgressView::new()
        },
        Lang::En.catalog(),
    );
    assert!(model.failed());
    assert!(model.terminal());
    assert!(!model.running(), "失败之后不再给「取消安装」");
    assert_eq!(model.stage_label, "Install failed");
}

/// 演示数据自己也得守规矩（它是出图的基线）：八行、四个主版本桶、英文目录下不出现中文类型词。
#[test]
fn the_demo_data_is_sane() {
    let catalog = Lang::En.catalog();
    let demo = crate::pages::download::demo(catalog);
    assert!(demo.loaded);
    assert_eq!(demo.rows.len(), 8);
    assert_eq!(demo.category, Category::All, "混合列表没有单个分类该亮");
    assert!(demo.query.is_empty(), "缺省演示没有搜索词");
    assert_eq!(
        demo.buckets
            .iter()
            .map(|bucket| bucket.key.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("24w"), Some("1.21"), Some("1.20"), None],
        "第二级侧栏的桶顺序与内容"
    );
    assert!(demo
        .rows
        .iter()
        .all(|row| row.supporting.contains("released")));
    assert!(demo.rows.iter().all(|row| !row.supporting.contains('版')));
}

/// 列表与尾部面板怎么分高：**档位 × 面板形态**两个条件同时成立才对半分。
///
/// 修前（M123）：面板一律按内容高钉在底部，700×760 的 chips 档里列表只剩 18px（0.4 行）。
/// M125 r1 修法只判档位，又踩出另一条回归：chips 档「刚进页面还没选版本」（`Hint`，面板只有
/// 一行提示）也被五五分，700×760 下只显示 **4 整行**、下半块留 ≈160px 空画布
/// （复审 P2-1 实测；main 那一态是 **7 整行**——不是 8：第 8 条墨迹其实是
/// `Pick a version, then install` 那句 hint，横向范围对不上版本号。r2 修回 7 整行）。
///
/// 现在的判据：`Chips` **且**面板真占高度（`Options` / `Install`）才 `Shared`；
/// `Hint` 在任何档位都 `Pinned`。
///
/// 抓得住：①chips 档改回「只判档位」（`Hint` 跟着对半分 ⇒ 列表掉到 4 行，像素守门
/// `ready::a_ready_shot_keeps_the_whole_visible_list` 立刻红）；②三栏档也改成 `Shared`
/// （1180 下选项面板被切成一半，`an_options_shot_…` 会红）。
/// 抓不住：**实际分到多少像素**（`iced` 的 `Element` 没有几何内省，那只能出图量）。
#[test]
fn the_chips_tier_shares_the_height_only_when_the_panel_takes_room() {
    use crate::pages::download::install::panel_state;
    use crate::pages::download::options::{InstallDraft, Loader, OptionsView};
    use crate::pages::download::rows::TailShare;
    use crate::pages::layout::VersionRailTier;
    use unlucky_presenter::state::AppState;

    let catalog = Lang::En.catalog();
    let mut app = AppState::new();
    app.apply(unlucky_presenter::dto::AppEvent::Core(
        unlucky_presenter::dto::CoreEvent::VersionListChanged(vec![version(
            "1.21.9",
            VersionKind::Release,
            false,
        )]),
    ));

    let row = VersionRow::of(&version("1.21.9", VersionKind::Release, false), catalog);
    // 选中版本 ⇒ 选项区（面板真的占高度）
    let options = Download {
        options: Some(OptionsView::of(
            &row,
            &app,
            &InstallDraft {
                selected: Some("1.21.9".to_string()),
                loader: Loader::Vanilla,
                loader_version: String::new(),
                instance_name: String::new(),
            },
            catalog,
        )),
        install: None,
        loaded: true,
        ..Download::default()
    };
    // 刚进页面 ⇒ 只有一行提示
    let hint = Download {
        options: None,
        install: None,
        loaded: true,
        ..Download::default()
    };

    assert_eq!(
        TailShare::of(VersionRailTier::Chips, &panel_state(&options)),
        TailShare::Shared,
        "chips 档 + 选项区展开：对半分（列表 4 整行、面板自己可滚）"
    );
    assert_eq!(
        TailShare::of(VersionRailTier::Chips, &panel_state(&hint)),
        TailShare::Pinned,
        "chips 档 + 只有一行提示：钉死（8 个版本要在屏）"
    );
    for tier in [
        VersionRailTier::Wide,
        VersionRailTier::Narrow,
        VersionRailTier::Compact,
    ] {
        assert_eq!(
            TailShare::of(tier, &panel_state(&options)),
            TailShare::Pinned,
            "{tier:?}：三栏档一律钉死（面板整块在屏）"
        );
        assert_eq!(TailShare::of(tier, &panel_state(&hint)), TailShare::Pinned);
    }

    // 两个装配点（三栏装配 / chips 分支）都**按档位 + 面板形态**传
    let chips_branch = include_str!("../../download.rs");
    let three_column = include_str!("../rails.rs");
    assert_eq!(
        chips_branch.matches("TailShare::of(rails.tier,").count()
            + three_column.matches("TailShare::of(rails.tier,").count(),
        2,
        "三栏装配与 chips 分支都要把面板形态传进 TailShare::of"
    );
}
