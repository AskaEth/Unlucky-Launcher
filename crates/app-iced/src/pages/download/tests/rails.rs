//! 两层侧栏的护栏（**变异可证**）+ 三栏宽度账。
//!
//! 从 `download/tests.rs` 拆出来：那边已经贴着 400 行硬线。

use unlucky_i18n::{Catalog, Lang};
use unlucky_presenter::dto::{AppEvent, CoreEvent, VersionKind, VersionSummary};
use unlucky_presenter::state::AppState;

use crate::app::components::navigation::geometry::{typography_headline, ListItemGeometry};
use crate::pages::download::options::InstallDraft;
use crate::pages::download::rails::{major_entries, major_shape, MajorShape};
use crate::pages::download::{Category, Download};
use crate::pages::layout::{
    available_width, frame_metrics, measure_cap_px, PaneKit, VersionRailTier, VersionRails,
    RAIL_GAP, RAIL_ICON_WIDTH,
};

use super::ui;

/// 三栏要吃掉整条可用宽（不是 424 的居中正文列）——三个宽度档各取一个采样点。
///
/// 抓得住：有人把第三栏写成 `Length::Fill`（那会让三列宽度之和**超过**可用宽、右侧被挤出屏）。
/// 抓不住：像素级对齐（出图在报告里）。
#[test]
fn the_three_columns_spend_the_whole_available_width() {
    for (window, tier) in [
        (1180.0, VersionRailTier::Wide),
        (900.0, VersionRailTier::Narrow),
        (700.0, VersionRailTier::Chips),
    ] {
        let rails = VersionRails::of(available_width(window));
        assert_eq!(rails.tier, tier, "{window} 的档位");
        if tier.is_three_column() {
            let spent = rails.type_rail + rails.major_rail + rails.list + 2.0 * RAIL_GAP;
            assert!(
                (spent - available_width(window)).abs() < f32::EPSILON,
                "{window}：三列 + 两处间距应正好等于可用宽（{spent} vs {}）",
                available_width(window)
            );
        }
    }
}

/// **两层侧栏都存在、且选中态真的传给列表**——M107 的护栏，改坏要红。
///
/// 四件事一起钉：
/// 1. 页面装配点仍调 `rails::three_column`（三栏装配是唯一入口）；
/// 2. 装配点里**两个**侧栏函数都被调用（删掉 `major_rail(` → 只剩一级侧栏 ⇒ 红）；
/// 3. 两级各自都有可点的消息（`SetCategory` / `SelectMajor`）；
/// 4. 列表的行确实按第二级选中态过滤（`rows::visible` + `selected_major`）。
///
/// 抓得住：删掉一层侧栏、或者让列表无视 `selected_major`（第二级点了没反应）。
/// 抓不住：两层是否**看起来**是两层（那要出图，见报告）。
#[test]
fn both_rail_levels_exist_and_the_selection_reaches_the_list() {
    let rails_source = include_str!("../rails.rs");
    let download_source = include_str!("../../download.rs");
    let rows_source = include_str!("../rows.rs");

    assert!(
        download_source.contains("rails::three_column"),
        "页面装配点不再调三栏装配"
    );
    for required in ["type_rail(ui", "major_rail(ui"] {
        assert!(
            rails_source.contains(required),
            "两层侧栏缺了一级：找不到 {required}"
        );
    }
    assert!(
        rails_source.contains("Message::SelectMajor"),
        "第二级（主版本）没有可点的消息"
    );
    assert!(
        rails_source.contains("Message::SetCategory"),
        "第一级（版本类型）没有可点的消息"
    );
    // 列表按第二级的**生效值**过滤（搜索期间放宽到「全部」，见 `group::active_bucket`）：
    // 这两样只要有一样不在，第二级点了就等于没反应。
    assert!(
        rows_source.contains("fn visible") && rows_source.contains("group::active_bucket"),
        "列表没有按第二级选中态过滤"
    );
    assert_eq!(Category::ALL.len(), 4, "第一级还是原来那四类，不许新造分类");
}

/// `< 600` 可用宽时两栏都退场、列表吃掉可用宽（chips 档的账）。
#[test]
fn the_chips_tier_gives_the_whole_width_to_the_list() {
    let rails = VersionRails::of(564.0);
    assert_eq!(rails.tier, VersionRailTier::Chips);
    assert_eq!(rails.type_rail, 0.0);
    assert_eq!(rails.major_rail, 0.0);
    assert_eq!(rails.list, 564.0);
}

/// **Chips 档的最后一个桶也要够得着**（复审 P2-2）。
///
/// 夹具按真实清单的量级造：`1.21 … 1.0` 共 **22 个桶**（`Release` 在 MOJANG 清单里就是这个量级）。
/// 三条一起钉：
/// 1. `major_entries` 把**每一个桶**都吐出来（含最后一个 `1.0`）——渲染两条路径都吃这一份数据；
/// 2. chips 行外面套了 `Direction::Horizontal` 的滚动（源码层，删掉那一层就够不着后面的桶）；
/// 3. 选中态按**生效值**算：搜索期间「全部版本」点亮（P2-1 的放宽）。
///
/// 抓得住：把 chips 行的横向滚动拿掉（第 2 条红）；或让 `major_entries` 在某个桶处截断（第 1 条红）。
/// 抓不住：滚动条在真机上好不好拖（那要出图 + 手感，报告里登记为未验）。
#[test]
fn every_bucket_is_reachable_in_the_chips_tier() {
    let versions: Vec<VersionSummary> = (0..=21)
        .rev()
        .map(|minor| VersionSummary {
            id: format!("1.{minor}.4"),
            kind: VersionKind::Release,
            released: "2026-09-20".to_string(),
            installed: false,
        })
        .collect();

    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(versions)));

    let catalog = Lang::En.catalog();
    let mut model =
        Download::from_snapshot(&state, Category::All, &InstallDraft::default(), catalog);
    assert_eq!(model.buckets.len(), 22, "夹具应有 22 个主版本桶");

    let entries = major_entries(catalog, &model);
    assert_eq!(entries.len(), 23, "「全部版本」+ 22 个桶");
    assert_eq!(
        entries.last().and_then(|entry| entry.key),
        Some("1.0"),
        "最后一个桶（版本序最老的那个）必须在列表里，否则它永远点不到"
    );
    assert_eq!(entries[1].label, "1.21", "第一条桶是版本序最新的");

    // 选中态按生效值：选中 1.0 时那一项点亮；搜索期间整体放宽到「全部」
    model.selected_major = Some("1.0".to_string());
    let entries = major_entries(catalog, &model);
    assert!(entries.last().expect("有最后一项").selected);
    state.download_query = "1.0".to_string();
    let mut searching =
        Download::from_snapshot(&state, Category::All, &InstallDraft::default(), catalog);
    searching.selected_major = Some("1.0".to_string());
    let entries = major_entries(catalog, &searching);
    assert!(entries[0].selected, "搜索期间「全部版本」点亮（P2-1）");
    assert!(
        entries.iter().skip(1).all(|entry| !entry.selected),
        "搜索期间不许留下幽灵选中"
    );

    // chips 行的横向滚动是「够得着」的实现手段
    let source = include_str!("../rails.rs");
    assert!(
        source.contains("Direction::Horizontal"),
        "chips 档的主版本行必须可横向滚动（否则 22 个桶里大部分点不到）"
    );
    assert!(
        source.contains("major_entries(catalog, model)"),
        "竖栏与 chips 行必须吃同一份 entries"
    );
}

/// 宽窗下多出来的宽度归**页面块**，不归散文列（M102 订正；旧口径是「留白」）。
///
/// 抓得住：谁把整页块又压回 424（视觉复审 ① 的原样：块宽 = `column`，1180 下左右各留 ~300px）；
/// 抓不住：真实像素（报告里的量测表负责）。
#[test]
fn extra_width_goes_to_the_page_block_not_to_the_prose_column() {
    let metrics = frame_metrics(PaneKit::Single, 1180.0);
    assert_eq!(metrics.column, measure_cap_px(), "散文列仍是行宽上限 424");
    assert_eq!(
        crate::pages::frame::body_width(metrics),
        metrics.available,
        "页面块必须吃满可用宽度（1044），不是被居中压成 424"
    );
    assert!(
        metrics.available - metrics.column >= 400.0,
        "两个量要分得开：块宽 1044 比散文列 424 宽出 620"
    );
}

// ------------------------------------ 图标条档（64 宽）的主版本栏画什么（M125 缺陷 2）

/// 一份只有「两个桶」的模型（`1.21` / `1.20`），给标签相关的两条断言当夹具。
fn bucket_model(catalog: Catalog) -> Download {
    let versions: Vec<VersionSummary> = ["1.21.9", "1.20.4"]
        .iter()
        .map(|id| VersionSummary {
            id: (*id).to_string(),
            kind: VersionKind::Release,
            released: "2026-09-20".to_string(),
            installed: false,
        })
        .collect();
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(versions)));
    Download::from_snapshot(&state, Category::All, &InstallDraft::default(), catalog)
}

/// **图标条档里只有首项换成图标**，桶标签照旧画文字——两种语言走同一条分支。
///
/// 抓得住：谁把首项改回文字（中英文各一条断言，英文会回到「All v」，中文回到只画得出两字）；
/// 或者把桶标签也一起换成图标（最后两条断言）。
/// 抓不住：图标画得对不对称（像素在 `options_shot` 的紧凑档断言与报告里的图）。
#[test]
fn only_the_first_major_entry_switches_to_an_icon_in_the_icon_rail() {
    for (lang, full) in [(Lang::En, "All versions"), (Lang::ZhCn, "全部版本")] {
        let catalog = lang.catalog();
        let model = bucket_model(catalog);
        let entries = major_entries(catalog, &model);

        let first = entries[0];
        assert_eq!(first.key, None, "{lang:?}：第一项是「全部版本」");
        assert_eq!(
            first.label, full,
            "{lang:?}：数据层仍带全称（chips 档要它）"
        );
        assert_eq!(
            major_shape(first, true),
            MajorShape::Icon(full),
            "{lang:?}：图标条档的首项不画文字，全称只进 tooltip"
        );
        assert_eq!(
            major_shape(first, false),
            MajorShape::Text(full),
            "{lang:?}：展开档（140）照旧画全称"
        );

        let bucket = entries[1];
        assert_eq!(bucket.key, Some("1.21"), "{lang:?}：第二条是第一个主版本桶");
        assert_eq!(major_shape(bucket, true), MajorShape::Text("1.21"));
        assert_eq!(major_shape(bucket, false), MajorShape::Text("1.21"));
    }
}

/// **宽度账**（算术，中英文各算一遍）：32px 的文本格里首项的全称放不下、桶标签放得下。
///
/// 这条是「为什么不画文字」的可核证依据，也是**中文结论**的那一半（另一半是上面那条
/// 「中文走图标分支」）：中文全角字形 1em ⇒ 宽度是精确值，不是估计。
#[test]
fn the_icon_rail_text_box_is_32_and_only_bucket_labels_fit() {
    let ui = ui();
    let geometry = ListItemGeometry::of(&ui);
    let text_box = RAIL_ICON_WIDTH - geometry.pad_start - geometry.pad_end;
    assert_eq!(text_box, 32.0, "64 宽的栏 − 2×16 内边距");

    let em = typography_headline().style().size_px;
    assert_eq!(em, 16.0, "ListItem 主文本是 `BodyLarge` 16px");

    // 中文：全角字形 = 1em ⇒ 「全部」= 32（与文本格同宽 ⇒ 零余量，所以短标签这条退路也不走）
    assert_eq!(2.0 * em, text_box, "「全部」正好占满 32：零余量");
    assert!(4.0 * em > text_box, "「全部版本」= 64 > 32：画不出来");

    // 英文：按 0.4em 的**下限**步进（`BodyLarge` 实测 0.43233em，这里取更保守的下限）
    assert!(
        0.4 * em * 12.0 > text_box,
        "「All versions」≥ 76.8 > 32：画不出来（改前图上就是「All v」）"
    );
    // 桶标签 4 字符：下限步进 25.6 ≤ 32（736 那张图实测墨迹 26，余量 6）
    assert!(
        0.4 * em * 4.0 <= text_box,
        "「1.21」/「24w」≤ 32：放得下，照旧画文字"
    );
}
