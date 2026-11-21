//! 安装选项区 × 三栏**同屏**（M123 ①）：宽度怎么分、窄窗怎么退化、选项区在不在同一棵树里。
//!
//! ## 为什么这条测试必须存在
//!
//! 「安装选项区」（`download/options`）与「三栏」（`download/rails`）各自都出过图、也各自都有
//! 断言，但它们**同屏**这件事从来没被验过。选项区挤在列表列的最底下：它一高，列表就矮；
//! 窗口一窄，两条栏一收，列表就窄。这两种挤压都不会让任何既有断言变红——那些断言分别只看
//! 「宽度裁决的数字」「选项区的纯决策」「树能构建出来」，没有一条把它们放在同一屏里看。
//!
//! ## 抓得住什么 / 抓不住什么
//!
//! 抓得住：①七档宽度下 `VersionRails` 的三栏数字与 `pages/layout/rails.rs` 模块文档的表逐格一致；
//! ②选项区的三种状态（原版 / Fabric / 不兼容）在**每一档**上都能与两份侧栏的数据一起构建出树，
//! 而且列表列**从不低于** [`version_list_min_width`]（= 424：行内「版本号 + 类型 + 日期 + 安装」
//! 四样仍在一行，不会因为宽度而折行/裁字）；③`Chips` 档两条栏归零、列表吃满可用宽（退化是
//! 逐档下沉，不是「挤一挤」）。
//!
//! 抓不住：**画出来长什么样**——两栏是不是真同屏、选项区有没有被窗口底边切掉、某一格文本会不会
//! 仍被裁。前两条由同目录的 `tests/options_shot.rs`（像素断言，需 `ULL_DL_OPTIONS_SHOT`）在真 PNG
//! 上验证，文本本身由 `DevDocs/artifacts/m123/` 的图人工看。
//! 更根本的一条：**没有 iced 的渲染器就拿不到布局**（`Element::layout` 要 `&Renderer`，而本 crate
//! 的 `Renderer` 在 `gpu` feature 下是 wgpu/tiny-skia 的 `fallback::Renderer`，测试里构造不出来），
//! 所以这里断的是**喂给树的模型 + 裁决出来的数字**，不是 iced 亲手算的矩形。

use iced::Element;
use unlucky_i18n::Lang;
use unlucky_presenter::dto::{AncientKind, AppEvent, CoreEvent, VersionKind, VersionSummary};
use unlucky_presenter::state::AppState;

use crate::pages::download::model::{Category, Download};
use crate::pages::download::{view, InstallDraft, Loader, Message};
use crate::pages::layout::rails::version_list_min_width;
use crate::pages::layout::{frame_metrics, PaneKit, VersionRailTier, VersionRails};

use super::ui;

/// 一档宽度的期望（数字来源：`pages/layout/rails.rs` 模块文档 §4 的数值表）。
///
/// 表里每一格都写死，**不**从被策源反算——否则「裁决算错了」与「表抄错了」会互相掩护。
const WIDTHS: [(f32, VersionRailTier, f32, f32, f32); 7] = [
    (1180.0, VersionRailTier::Wide, 140.0, 140.0, 716.0),
    (1024.0, VersionRailTier::Wide, 140.0, 140.0, 560.0),
    (900.0, VersionRailTier::Narrow, 64.0, 140.0, 512.0),
    (880.0, VersionRailTier::Narrow, 64.0, 140.0, 492.0),
    // 812 = `Narrow` 档的门槛：类型栏已收成 64，列表仍停在 424
    (812.0, VersionRailTier::Narrow, 64.0, 140.0, 424.0),
    // 736 = `Compact` 档：两条栏都收成 64，列表仍停在 424
    (736.0, VersionRailTier::Compact, 64.0, 64.0, 424.0),
    // 700 = `Chips` 档：三栏不成立，列表吃满可用宽
    (700.0, VersionRailTier::Chips, 0.0, 0.0, 564.0),
];

/// 出图那一屏的演示版本表（三种形态就够：正式 / 预览 / 远古，两条侧栏都有内容）。
fn versions() -> Vec<VersionSummary> {
    fn row(id: &str, kind: VersionKind) -> VersionSummary {
        VersionSummary {
            id: id.to_string(),
            kind,
            released: "2026-09-20".to_string(),
            installed: false,
        }
    }

    vec![
        row("1.21.9", VersionKind::Release),
        row("1.20.4", VersionKind::Release),
        row("24w14a", VersionKind::Snapshot),
        row("rd-132211", VersionKind::Ancient(AncientKind::Classic)),
    ]
}

/// 选项态模型：`AppState` + 草稿 → **产品那条真转换**（与 `demo::options` 同一条路径）。
///
/// `cached` = 加载器版本列表（Fabric 那一屏才有；它是选项区里最高的一段，也正是「面板一高
/// 列表就矮」的那一态）。
fn options_model(version: &str, loader: Loader, cached: Option<&[&str]>) -> Download {
    let catalog = Lang::En.catalog();
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(versions())));
    if let Some(items) = cached {
        state.apply(AppEvent::Core(CoreEvent::LoaderVersions {
            kind: loader.kind(),
            mc_version: version.to_string(),
            result: Ok(items.iter().map(|item| (*item).to_string()).collect()),
        }));
    }
    let draft = InstallDraft {
        selected: Some(version.to_string()),
        loader,
        ..InstallDraft::default()
    };
    Download::from_snapshot(&state, Category::All, &draft, catalog)
}

/// 选项区的三种取样（原版 / Fabric 带版本列表 / NeoForge 对远古版不兼容）。
fn samples() -> [(&'static str, Download); 3] {
    [
        ("options", options_model("1.21.9", Loader::Vanilla, None)),
        (
            "options-fabric",
            options_model(
                "1.21.9",
                Loader::Fabric,
                Some(&["0.16.5", "0.16.4", "0.15.11"]),
            ),
        ),
        (
            "options-incompat",
            options_model("rd-132211", Loader::NeoForge, None),
        ),
    ]
}

/// 七档宽度 × 三种选项态：三栏数字一致、树能构建、列表列不被挤到 424 以下。
///
/// 424 这条是「不裁字」的宽度侧前提：行内四样的最小宽 `layout::rails::version_row_min_width()`
/// = 412 < 424，所以只要列表列 ≥ 424，版本号那一格就还有 12px 余量，四样不会折行。
#[test]
fn the_options_state_keeps_all_three_columns_at_every_width() {
    let ui = ui();
    let floor = version_list_min_width();
    let samples = samples();

    for (width, tier, type_rail, major_rail, list) in WIDTHS {
        let metrics = frame_metrics(PaneKit::Single, width);
        let rails = VersionRails::of_metrics(metrics);

        assert_eq!(rails.tier, tier, "{width} 的档位");
        assert!((rails.type_rail - type_rail).abs() < 0.01, "{width} 类型栏");
        assert!(
            (rails.major_rail - major_rail).abs() < 0.01,
            "{width} 主版本栏"
        );
        assert!((rails.list - list).abs() < 0.01, "{width} 列表列");
        assert!(
            rails.list >= floor - 0.01,
            "{width}：列表列 {} 低于行宽下限 {floor} ⇒ 行内四样会折行/裁字",
            rails.list
        );
        // 三栏档两条栏都 >0（同屏的前提）；Chips 档两条都归零、列表吃满（逐档下沉）
        if rails.tier.is_three_column() {
            assert!(rails.type_rail > 0.0 && rails.major_rail > 0.0, "{width}");
        } else {
            assert_eq!((rails.type_rail, rails.major_rail), (0.0, 0.0), "{width}");
        }

        for (name, model) in &samples {
            // 同一份模型里：两条栏的内容（4 个类型 / 主版本桶）+ 行 + 选项面板都在
            assert_eq!(Category::ALL.len(), 4, "第一栏是四个固定项");
            assert!(!model.buckets.is_empty(), "{name}@{width} 第二栏没有桶");
            assert!(!model.rows.is_empty(), "{name}@{width} 列表没有行");
            assert!(
                model.options.is_some(),
                "{name}@{width} 面板不是选项态——这张图就不再是「选项区 × 三栏」"
            );
            // 树真的建得出来（两栏 + 列表 + 面板同一次装配）
            let _: Element<'_, Message> = view(&ui, metrics, model, Lang::En);
        }
    }
}

/// 窄窗退化是**逐档下沉**：类型栏先成图标条、再两条都成图标条，列表列在两种情形下都不低于 424。
///
/// 抓得住：某天有人为了「窄窗也放得下两条栏」把 64 改成 48 或把列表下限往下调 —— 那时这里的
/// 逐档数字会先红。抓不住：图标条里的 tooltip 在真机上是否弹得出来（静态图拍不到 hover 层）。
#[test]
fn the_narrow_tiers_step_down_before_they_squeeze_the_list() {
    let mut previous = VersionRails::of_metrics(frame_metrics(PaneKit::Single, 2400.0));
    assert_eq!(previous.tier, VersionRailTier::Wide);

    for width in [1600.0, 1180.0, 1024.0, 900.0, 812.0, 736.0, 700.0, 600.0] {
        let rails = VersionRails::of_metrics(frame_metrics(PaneKit::Single, width));
        // 档位只降不升，且降档时**收的是侧栏**，列表不会在降档的那一格突然变窄再变宽
        assert!(
            rails.list >= version_list_min_width() - 0.01,
            "{width}：列表列 {} 跌破 424",
            rails.list
        );
        if rails.tier.is_three_column() && previous.tier.is_three_column() {
            assert!(
                rails.type_rail <= previous.type_rail + 0.01
                    && rails.major_rail <= previous.major_rail + 0.01,
                "{width}：侧栏在宽窗上反而变宽了"
            );
        }
        previous = rails;
    }
}
