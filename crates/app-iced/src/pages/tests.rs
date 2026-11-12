//! 页面层的**数值判据**：M90 接真组件后，「M84 的版面约束一条没破」这件事在这里钉死。
//!
//! 每条断言都注明了「抓得住什么 / 抓不住什么」——只测到纯函数，渲染真像素另见报告里的图与量测。

mod base_wiring;

use super::layout::{self, frame_metrics, measure_cap_px, PaneKit, WidthClass};
use super::nav::NAV;
use super::*;
use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;
use unlucky_i18n::Lang;
use unlucky_presenter::router::{NavSection, Route};

use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

#[test]
fn five_screens_answer_the_mission_list() {
    // M109：内容库是第五页（`download` 与 `content` 共用导航轨的「下载」项）
    assert_eq!(Screen::ALL.len(), 5);
    let slugs: Vec<&str> = Screen::ALL.iter().map(|screen| screen.slug()).collect();
    assert_eq!(
        slugs,
        vec!["instances", "download", "content", "settings", "logs"]
    );
    for screen in Screen::ALL {
        assert_eq!(Screen::from_slug(screen.slug()), Some(screen));
    }
    assert_eq!(Screen::from_slug("gallery"), None);
}

#[test]
fn screens_declare_the_layout_their_page_needs() {
    assert_eq!(Screen::Instances.kit(), PaneKit::ListDetail);
    assert_eq!(Screen::Settings.kit(), PaneKit::Supporting);
    assert_eq!(Screen::Download.kit(), PaneKit::Single);
    // 内容库自带页内三栏，版面基座仍是单列（见 `pages/content/layout.rs`）
    assert_eq!(Screen::Content.kit(), PaneKit::Single);
    assert_eq!(Screen::Logs.kit(), PaneKit::Single);
}

/// 四个页面 × 三个对照宽度都能构图（不 panic、不依赖运行时）。
///
/// **抓得住**：`view` 的每条分支都能产出一棵树（含真空列表、无选中详情）。
/// **抓不住**：像素级观感（那要出图，见报告）。
#[test]
fn every_screen_builds_at_the_three_comparison_widths() {
    let ui = ui();
    let form = demo_launch_form();
    for width in [1180.0, 900.0, 700.0] {
        for screen in Screen::ALL {
            let model = demo_model(screen, width, 760.0);
            let _element: Element<'_, Message> = app(&ui, &model, &form);
        }
    }
}

/// 空数据也要能构图（真实后端的「还没扫到实例」是常态，不能 panic）。
#[test]
fn an_empty_snapshot_still_builds() {
    let ui = ui();
    let form = demo_launch_form();
    for screen in Screen::ALL {
        let model = Model::new(screen, 1180.0, 760.0, Some(screen.nav()));
        let _element: Element<'_, Message> = app(&ui, &model, &form);
    }
}

/// 版面对照：同一页在 1180 / 900 / 700 三个宽度上必须**真的换版面**。
#[test]
fn the_width_comparison_really_changes_the_layout() {
    let wide = frame_metrics(PaneKit::ListDetail, 1180.0);
    let mid = frame_metrics(PaneKit::ListDetail, 900.0);
    let narrow = frame_metrics(PaneKit::ListDetail, 700.0);

    assert_eq!(wide.layout, PageLayout::ListDetail);
    assert_eq!(mid.layout, PageLayout::ListDetail);
    assert_eq!(narrow.layout, PageLayout::SingleColumn);

    // 900 与 1180 同为 list-detail，但详情列必须真的变窄（不是硬编码 1180 的宽度）
    assert!(mid.column < wide.column, "900 的详情列应窄于 1180");
    assert!(narrow.secondary.is_none(), "700 不许再有列表列");
}

/// **M102 的对齐判据**：正文块左缘 = 页头标题左缘（`body_offset == 0`），块宽 = pane 宽（封顶 1440）。
///
/// 页头标题的左缘 = `rail + gutter`（`frame::header_bar` 的 padding 与 `frame::page` 的内容
/// padding 是同一个 `metrics.gutter`）；正文块宽 = `frame::body_width`。
///
/// **抓得住**：块宽被改回 424（视觉复审 ①：左缘 422 对标题 112）、偏移不再是 0（居中/右对齐）、
/// 上限被挪动；
/// **抓不住**：`frame::body` 到底有没有照这两个数建盒子（那由
/// [`frame_body_is_built_from_the_block_quantities`] 扫源码守）。
#[test]
fn body_block_starts_at_the_header_title_and_fills_the_page() {
    for width in [600.0, 840.0, 900.0, 1180.0] {
        let metrics = frame_metrics(PaneKit::Single, width);
        let title_x = layout::rail_width() + metrics.gutter;
        let block = frame::body_width(metrics);

        assert_eq!(
            block, metrics.available,
            "{width}: 单列页块宽必须是可用宽度"
        );
        assert_eq!(
            layout::block::body_offset(metrics),
            0.0,
            "{width}: 块必须贴左"
        );
        assert!(
            (title_x + block + metrics.gutter - width).abs() < f32::EPSILON,
            "{width}: 块从 {title_x} 起、宽 {block}，没铺到右页边距"
        );
        // 块变宽不等于一句话变宽：散文列仍 ≤424
        assert!(metrics.column <= measure_cap_px() + f32::EPSILON);
    }

    // 验收尺寸 1180：标题左缘 112、块宽 1044（旧版居中时正文左缘是 422）
    let wide = frame_metrics(PaneKit::Single, 1180.0);
    assert_eq!(layout::rail_width() + wide.gutter, 112.0, "页头标题左缘");
    assert_eq!(frame::body_width(wide), 1044.0, "正文块宽度");

    // 极宽窗：块宽封顶 1440（tower 拍板），仍然贴左 ⇒ 右侧留白而不是居中
    let huge = frame_metrics(PaneKit::Single, 2560.0);
    assert_eq!(frame::body_width(huge), layout::block::BODY_MAX_WIDTH);
    assert_eq!(frame::body_width(huge), 1440.0);
    assert_eq!(layout::block::body_offset(huge), 0.0);
    assert!(112.0 + frame::body_width(huge) < 2560.0);

    // 两列版面的块宽 = 主列 pane（实例页正文在 panes 的主列里，不铺整页）
    let two = frame_metrics(PaneKit::ListDetail, 1180.0);
    assert_eq!(frame::body_width(two), two.main_pane);
    assert_eq!(two.main_pane, 1180.0 - 88.0 - 48.0 - 336.0 - 24.0);
}

/// **源码结构**：`frame::body` 的盒子必须从「块宽 + 块偏移 + 贴左」这三个数建。
///
/// 为什么读源码：iced 的 `Element` 没有几何内省接口，`frame::body` 换成
/// `centered(metrics.column, content)`（**就是视觉复审 ① 的改前写法**）不会红任何数值断言 ——
/// M102 r1 的变异实测就是这样漏掉的（`pages/tests.rs` 那条只喂 `frame_metrics()` 与
/// `body_width()`）。所以这条盯的是**摆法**：宽度来自 `body_width(metrics)`、左内边距来自
/// `body_offset(metrics)`、`align_x` 只许是 `Start`（不写 = container 默认 Start，等价），
/// 且不出现居中/散文列宽。
///
/// 扫的是 [`layout::source::region_across`]：`frame.rs` **加上** `layout/block.rs`，
/// 于是「拆成 `body` → `body_impl`」（变异 H1）与「把盒子原样搬进 `block::body_box`」（变异 M6）
/// 这两种**等价重构都不假红**，而「换成居中 424 盒子」照样红（变异 A）。
///
/// **抓得住**：盒子的**语义**变了（居中 / 用散文列宽 / 用 `centered`）；
/// **抓不住**：`frame::centered(metrics.column, …)` 这种**与 `frame::body` 逐像素等价**的另写
/// （r4 复审的 M1/M2 —— 按塔的裁定那是等价重构，护栏不该为此红），也不抓「先赋局部变量再建盒」。
#[test]
fn frame_body_is_built_from_the_block_quantities() {
    let body = layout::source::region_across(
        &[include_str!("frame.rs"), include_str!("layout/block.rs")],
        "pub fn body<'a>(",
    );

    for needle in ["body_width(metrics)", "body_offset(metrics)"] {
        assert!(
            body.contains(needle),
            "`frame::body` 必须从 {needle} 建盒（贴左 + 块宽 + 零偏移）：\n{body}"
        );
    }

    // `align_x` 出现就必须是 Start：`.align_y(Alignment::Start)` 单独存在不算数（r2 复审 P2-3），
    // 而「干脆不写 align_x」（container 默认 Start）视为等价重构，放行。
    // 判的是**实参**而不是整段文本，所以 `Alignment::Start` 与 `iced::Alignment::Start` 同等放行
    // （把盒子搬进别的模块后路径要写全，那是等价重构，不该假红 —— r4 复审的变异 M6）。
    for argument in layout::source::call_arguments(&body, ".align_x(") {
        assert!(
            argument.trim().ends_with("Alignment::Start"),
            "`frame::body` 的 align_x 只许是 Start，收到 {argument:?}：\n{body}"
        );
    }

    for forbidden in [
        "centered(",
        "single_column(",
        "metrics.column",
        "Alignment::Center",
    ] {
        assert!(
            !body.contains(forbidden),
            "`frame::body` 里出现了 {forbidden:?} —— 那是居中 / 散文列宽的老写法：\n{body}"
        );
    }
}

/// **源码结构**：`frame::prose` 的宽度必须来自 `prose_width`（= `metrics.column`），不来自块宽。
///
/// M102 把两个量拆开后，「把散文塞进 1044 的块」不会红任何数值断言；这条盯接线。
/// 与 `frame::body` 那条一样用 [`layout::source::region_across`]（`frame.rs` + `layout/block.rs`），
/// 所以把盒子搬进 `layout/block.rs` 的等价重构不假红；抓的是**语义**（用块宽/可用宽度就红）。
#[test]
fn frame_prose_is_built_from_the_prose_width() {
    let prose = layout::source::region_across(
        &[include_str!("frame.rs"), include_str!("layout/block.rs")],
        "pub fn prose<'a>(",
    );

    assert!(
        prose.contains("prose_width(metrics)"),
        "`frame::prose` 必须从 prose_width 取宽：\n{prose}"
    );
    for forbidden in ["body_width(", "metrics.available", "metrics.main_pane"] {
        assert!(
            !prose.contains(forbidden),
            "`frame::prose` 里出现了 {forbidden:?} —— 散文会失去 72 字符行宽承诺：\n{prose}"
        );
    }
}

/// **源码结构（调用点）**：单列页必须调 `frame::body`，不许退回 `frame::single_column`。
///
/// r2 复审的变异 G：把四个页面文件里的 7 个调用点（`download.rs` 2 / `logs.rs` 3 /
/// `instances.rs` 1 / `instances/detail.rs` 1）一键改回 `frame::single_column` ⇒ 420 条全绿，
/// 而视觉复审 ① 原样回来。r2 的护栏只盯 `frame::body` **内部**，`single_column` 又是为设置页
/// 合法保留的，所以「哪个页面用它」必须单独守。
///
/// 逐文件断言（而不是只数一条调用）：**同一个文件里退一个调用点**也会让
/// `frame::single_column(` 出现，立刻红。
///
/// **抓得住**：页面退回居中的老写法（视觉复审 ① 原样）；
/// **抓不住**：新增的页面文件（它不在这个清单里——新页要自己登记）。
#[test]
fn single_column_pages_call_the_body_box_not_the_centred_one() {
    for (name, source) in [
        ("download.rs", include_str!("download.rs")),
        ("logs.rs", include_str!("logs.rs")),
        ("instances.rs", include_str!("instances.rs")),
        ("instances/detail.rs", include_str!("instances/detail.rs")),
    ] {
        assert!(
            source.contains("frame::body("),
            "{name} 没有调用 frame::body —— 正文块不再从块宽/块偏移建盒"
        );
        assert!(
            !source.contains("frame::single_column("),
            "{name} 退回了 frame::single_column（= 424 居中，视觉复审 ① 的原样）"
        );
    }

    // 实例页窄窗那一屏的唯一入口也得走 body
    let narrow =
        layout::source::function_body(include_str!("instances/detail.rs"), "fn narrow_detail");
    assert!(
        narrow.contains("frame::body("),
        "narrow_detail 没走 frame::body：\n{narrow}"
    );
}

/// **M84 的行宽红线**：任何页、任何窗口宽度、任何版面，正文列都不许超过 424px。
///
/// **抓得住**：`frame_metrics` 的 `column`（页面唯一的列宽来源）不会越界；
/// **抓不住**：页面自己绕过 metrics 写死宽度（那条由 `tests/no_hardcoded_tokens.rs` 的
/// 「`.width(` 后面不许跟数字字面量」守门——它扫的是 `src/shell` + `src/pages` + `src/app`
/// （M90 r2/P3-3 把 `app` 加进去了）。
#[test]
fn every_page_any_width_stays_under_the_measure_cap() {
    let cap = measure_cap_px();
    assert!((cap - 424.0).abs() < f32::EPSILON, "行宽上限必须是 424");

    for screen in Screen::ALL {
        for width in [416.0, 600.0, 700.0, 840.0, 900.0, 1180.0, 1600.0, 2560.0] {
            let metrics = frame_metrics(screen.kit(), width);
            assert!(
                metrics.column <= cap + f32::EPSILON,
                "{} @{width} 的正文列 {} 超过 424",
                screen.slug(),
                metrics.column
            );
        }
    }
}

/// 断点 600 / 840 与 rail 88：M84 的三条硬值，接真组件后不许漂。
///
/// **抓得住**：布局系统的常量与 `WidthClass::of` 的分档；
/// **抓不住**：导航轨真画出来是不是 88（那条要量像素；组件自己声明 88 见下一个断言）。
#[test]
fn the_three_frozen_numbers_are_unchanged() {
    assert!((layout::BREAKPOINT_MEDIUM - 600.0).abs() < f32::EPSILON);
    assert!((layout::BREAKPOINT_EXPANDED - 840.0).abs() < f32::EPSILON);
    assert!((layout::rail_width() - 88.0).abs() < f32::EPSILON);

    assert_eq!(WidthClass::of(599.9), WidthClass::Compact);
    assert_eq!(WidthClass::of(600.0), WidthClass::Medium);
    assert_eq!(WidthClass::of(839.9), WidthClass::Medium);
    assert_eq!(WidthClass::of(840.0), WidthClass::Expanded);
}

/// **组件版**导航轨（M78 的 `NavigationRail`）声明的宽度必须与布局系统同值。
///
/// 注意这不是产品路径的实现（M90 r3 订正）：产品页的 rail 由 `shell/nav.rs` 画，组件版只服务
/// `pages::app`（harness 出口图）与单测。这条守的是「组件与布局系统没分叉」，**不是**「产品 rail
/// 没问题」——两套实现的漂移风险见 `pages/mod.rs` 偏离表 #10。
///
/// **抓得住**：`NavPaneGeometry::rail().width` == `layout::rail_width()` == 88；
/// **抓不住**：容器圆角/药丸摆放的像素级正确性（组件自己的 probe 管），也抓不住 `shell/nav.rs`。
#[test]
fn the_real_rail_agrees_with_the_layout_system() {
    let ui = ui();
    let geometry = crate::app::components::navigation::geometry::NavPaneGeometry::of(
        &ui,
        crate::app::components::navigation::NavLayout::Rail,
    );
    assert!(
        (geometry.width - layout::rail_width()).abs() < f32::EPSILON,
        "真组件 rail 宽 {} 与布局系统的 {} 分叉了",
        geometry.width,
        layout::rail_width()
    );
}

/// 路由 → 页面：四个一级页各自落到自己那页，`Detail` 落在实例页，`Agent` 没有页面（app 层回退）。
#[test]
fn routes_map_onto_pages() {
    assert_eq!(Screen::of(&Route::Instances), Some(Screen::Instances));
    assert_eq!(
        Screen::of(&Route::Detail("survival".to_string())),
        Some(Screen::Instances),
        "详情页仍是实例页的那一屏"
    );
    assert_eq!(Screen::of(&Route::Download), Some(Screen::Download));
    assert_eq!(Screen::of(&Route::Logs), Some(Screen::Logs));
    assert_eq!(Screen::of(&Route::Settings), Some(Screen::Settings));
    assert_eq!(Screen::of(&Route::Agent), None, "AI 独立页还没有页面实现");
}

/// 导航轨的每一项都指向一个**它能高亮的**路由（点完不会出现「没有任何项高亮」的悬空态）。
///
/// **抓得住**：`NAV` 表的 5 项与 `Screen::nav` / `Route::nav` 三者没有错位；
/// **抓不住**：点下去真的换页（那是 `app/update.rs` 的 `Navigate` 分支 + 集成图）。
#[test]
fn every_rail_item_points_at_a_route_that_highlights_it() {
    assert_eq!(NAV.len(), 5, "导航轨 5 项（4 个一级页 + AI）");
    let catalog = Lang::En.catalog();

    for (section, _icon, label, route) in NAV {
        assert!(!catalog.text(label).is_empty(), "{} 没有文案", label.name());
        assert_eq!(
            route.nav(),
            section,
            "{} 指向的 {route:?} 高亮的是 {:?}",
            label.name(),
            route.nav()
        );
    }
    assert_eq!(
        NAV[4].0, None,
        "AI 项没有导航高亮（Route::Agent.nav() 是 None）"
    );

    // 四个一级页的 page-nav 与 route-nav 同源
    assert_eq!(Screen::Instances.nav(), NavSection::Instances);
    assert_eq!(Screen::Settings.nav(), NavSection::Settings);
}

/// 演示模型：12 行 + 1 条选中详情（M84 的出图基线就靠这份数据）。
#[test]
fn the_demo_model_carries_the_m84_baseline() {
    let model = demo_model(Screen::Instances, 1180.0, 760.0);
    assert_eq!(model.instances.rows.len(), 12);
    let detail = model.instances.detail.as_ref().expect("演示数据有选中项");
    assert_eq!(detail.meta.len(), 6, "元信息 6 格");
    assert!(detail.note.is_some(), "演示数据带正文段");
    assert_eq!(model.active, Some(NavSection::Instances));

    // 其余三页不带实例数据（占位视觉不读它）
    for screen in [Screen::Download, Screen::Settings, Screen::Logs] {
        let model = demo_model(screen, 1180.0, 760.0);
        assert!(
            model.instances.rows.is_empty(),
            "{} 不该带实例数据",
            screen.slug()
        );
    }
}
