//! search 的单测（从 `search.rs` 拆出：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;
use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;

use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

fn leaked(flavor: SchemeFlavor) -> &'static Ui {
    Box::leak(Box::new(ui(flavor)))
}

/// **M69 不变量**：图标可视边 = pad + offset（12 + 4 = 16），且本实现只做加法。
#[test]
fn icon_edge_is_pad_plus_offset_not_a_literal() {
    let bar = Component::Search.metrics().expect("Search 有组件级度量");
    assert_eq!(bar.pad_start, 12.0, "M69 钉住的 pad_start");
    assert_eq!(bar.icon_offset, 4.0, "M69 钉住的 icon_offset");
    assert_eq!(bar.pad_start + bar.icon_offset, 16.0);
    assert_eq!(bar.icon, 24.0, "SearchBarTokens 图标 24");
    // 文本从 pad（12）起：图标（16..40）+ gap 8 → 48 是文字的实际起点
    assert_eq!(
        bar.pad_start + bar.icon_offset + bar.icon + bar.gap,
        48.0,
        "图标槽 + 间距后文字起点 48"
    );
}

/// 20.1：高 56、圆角 full = 28（药丸）；条高来自 LM，不重复定义。
#[test]
fn bar_height_and_radius_come_from_tokens() {
    let ui = ui(SchemeFlavor::Light);
    assert_eq!(ui.metrics().search_bar_height, 56.0);
    assert_eq!(
        ShapeScale::Full.radius_px(ui.metrics().search_bar_height),
        28.0,
        "SearchBarCornerRadius = InputFieldHeight / 2"
    );
    assert_eq!(Component::Search.spec().shape, ShapeScale::Full);
    // 高程按观感取 L0（无可见阴影）
    assert_eq!(
        Component::Search.spec().elevation,
        unlucky_design::elevation::Elevation::Level0
    );
}

/// 20.6 / 20.9：面板头部 56、形状 ExtraLarge(28)、最小高 240、docked 宽 360。
#[test]
fn docked_panel_geometry() {
    let ui = ui(SchemeFlavor::Light);
    assert_eq!(ShapeScale::ExtraLarge.dp(), 28.0, "DockedContainerShape");
    assert_eq!(
        SEARCH_VIEW_MIN_HEIGHT, 240.0,
        "DockedExpandedTableMinHeight"
    );
    assert_eq!(SEARCH_MIN_WIDTH, 360.0, "SearchBarMinWidth = docked 面板宽");
    assert_eq!(ui.metrics().search_bar_height, 56.0, "头部与 bar 同高");
}

/// bar 与 view 都能构建（含禁用态：输入被摘掉）。
#[test]
fn both_forms_build() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        for state in Interaction::ALL {
            let ui = leaked(flavor);
            let bar = SearchBar::new("").placeholder("搜索").state(state);
            let _: Element<'_, u8> = search_bar(ui, bar, |_| 0_u8);

            let results: &[&str] = &["实例一", "1.21.4"];
            let view = SearchView::new("实例", results, "没有结果").placeholder("搜索");
            let _: Element<'_, u8> = search_view(ui, view, |_| 0_u8);
        }
    }
}

/// 空结果也必须给面板内容（20C：不许收回到 bar）。
#[test]
fn empty_results_still_render_a_panel() {
    let ui = leaked(SchemeFlavor::Light);
    let empty: &[&str] = &[];
    let view = SearchView::new("xyz", empty, "没有结果");
    let _: Element<'_, u8> = search_view(ui, view, |_| 0_u8);
}

/// **M460 判据 ①**：尾随动作画在**药丸里**（`bar_content` 的 `contents` 行——与输入框同一个
/// 容器），而且是一个**挂了 `on_press`** 的按钮；不给动作时药丸与之前逐像素相同。
///
/// 为什么这条不是几何测算：`Element` 在 iced 0.14 没有内省可言，能证明「叉在药丸里」的只有
/// **拼树的那一段自己说**——[`super::trailing::action_in_field`] 由 `trailing()` 写，而
/// `trailing()` 的产物正是 `bar_content` push 进 `contents` 的那一项。把叉挪回页面（M457 的
/// 写法）之后，这一支收到的 `action` 是 `None`，取样变 `Some(false)`。
///
/// ## 变异与期望红（每条只改一处）
///
/// | 变异 | 改哪里 | 期望红在哪句 |
/// |---|---|---|
/// | 叉退回页面外（组件不再接动作） | `pages/instances/search.rs` 删掉 `.trailing_action(…)`，自己 `buttons::icon` | `Some(false)` ≠ `Some(true)` |
/// | 动作画在药丸外（不进 `contents` 行） | `search.rs::bar_content` 不再 push `tail.row` | 同上（`trailing()` 收到 `None`） |
/// | 空药丸上也画一个叉 | `pages/instances/search.rs` 的 `is_active(query)` 一并去掉 | 第二条断言变 `Some(true)` |
/// | 叉不能点（只画图标） | `trailing` 里 `buttons::icon(…, Some(message), …)` 的 `Some` → `None` | 按钮的落点没了（页面级判据 + 代码走查；本取样口只证明「动作被拼进了输入行」） |
#[test]
fn the_trailing_action_is_mounted_inside_the_bar() {
    let ui = leaked(SchemeFlavor::Light);

    let with_action = SearchBar::new("原版").trailing_action(7_u8);
    let _: Element<'_, u8> = search_bar(ui, with_action, |_| 0_u8);
    assert_eq!(
        super::trailing::action_in_field(),
        Some(true),
        "有动作的药丸里没画出尾随动作（M460：叉必须在输入行内）"
    );

    // 不给动作：这正是「空药丸」与非实例页调用方的形态，取样必须说「没有动作」。
    let plain: SearchBar<'_, u8> = SearchBar::new("");
    let _: Element<'_, u8> = search_bar(ui, plain, |_| 0_u8);
    assert_eq!(
        super::trailing::action_in_field(),
        Some(false),
        "没有给动作的药丸里画出了叉（空药丸不该有清除动作）"
    );
}
