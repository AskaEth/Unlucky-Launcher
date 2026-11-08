//! `geometry` 的单元测试（split 拆出来的：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;
use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;

use crate::theme::Families;

fn ui() -> Ui {
    Ui::with_platform(
        SchemeFlavor::Light,
        Platform::Windows,
        Families {
            text: "Segoe UI",
            display: "Segoe UI",
            mono: "Consolas",
        },
    )
}

/// 4.1 / 4.2：三种行高 44 / 60 / 76（A1 56/72/88 的登记偏离 −12）。
#[test]
fn list_item_heights_are_44_60_76() {
    let geometry = ListItemGeometry::of(&ui());
    assert_eq!(geometry.heights, [44.0, 60.0, 76.0]);
    for lines in 1..=3 {
        assert_eq!(geometry.height(lines), geometry.heights[lines - 1]);
    }
    // 官方 delta 也是每行 +16
    assert_eq!(geometry.heights[1] - geometry.heights[0], 16.0);
    assert_eq!(geometry.heights[2] - geometry.heights[1], 16.0);
}

/// 行高必须**装得下**文本块（否则文字会被裁）：三行文本块 16+24+20 = 60 ≤ 76。
#[test]
fn list_item_rows_fit_their_text_block() {
    let ui = ui();
    let geometry = ListItemGeometry::of(&ui);
    for lines in 1..=3 {
        let block = geometry.text_block(&ui, lines);
        let pad = geometry.pad_y(&ui, lines);
        assert!(pad >= 0.0, "{lines} 行的垂直内边距是负数（行高装不下文本）");
        assert!(
            (block + pad * 2.0 - geometry.height(lines)).abs() < f32::EPSILON,
            "{lines} 行：文本块 {block} + 2×{pad} ≠ 行高 {}",
            geometry.height(lines)
        );
    }
    assert_eq!(geometry.text_block(&ui, 1), 24.0, "BodyLarge 24/24");
    assert_eq!(geometry.text_block(&ui, 2), 44.0, "24 + BodyMedium 20");
    assert_eq!(geometry.text_block(&ui, 3), 60.0, "LabelSmall 16 + 24 + 20");
    assert_eq!(geometry.pad_y(&ui, 1), 10.0, "(44 − 24) / 2");
    assert_eq!(geometry.pad_y(&ui, 3), 8.0, "(76 − 60) / 2");
}

/// 21.1 / 21.5 / 21.6：drawer 240、药丸 216（左右各缩 12）、项高 56、内边距 16/24。
#[test]
fn drawer_pane_is_240_wide_with_a_216_pill() {
    let geometry = NavPaneGeometry::of(&ui(), NavLayout::Drawer);
    assert_eq!(geometry.width, 240.0, "D33 的展开宽度");
    assert_eq!(geometry.pill_inset, 12.0, "ItemPadding");
    assert_eq!(
        geometry.pill_width, 216.0,
        "240 − 2×12（A1: 360 − 2×12 = 336）"
    );
    assert_eq!(geometry.item_height, 56.0, "G24：A1 ActiveIndicatorHeight");
    assert_eq!(geometry.pill_height, 56.0, "指示器高 = 项高");
    assert_eq!((geometry.pad_start, geometry.pad_end), (16.0, 24.0));
    assert_eq!(geometry.gap, 12.0, "图标 ↔ label");
    assert_eq!(geometry.icon, 24.0);
    // 药丸 + 两侧内缩 = 容器宽（这条是「药丸从容器宽派生」的等式，Q8 的回归点）
    assert!((geometry.pill_width + geometry.pill_inset * 2.0 - geometry.width).abs() < 0.01);
}

/// D33：rail 与 drawer 是**同一个容器**的两个宽度（88 ⇄ 240），不是两套组件。
#[test]
fn rail_and_drawer_share_one_container() {
    let ui = ui();
    let rail = NavPaneGeometry::of(&ui, NavLayout::Rail);
    let drawer = NavPaneGeometry::of(&ui, NavLayout::Drawer);

    assert_eq!(rail.width, 88.0);
    assert_eq!(drawer.width, 240.0);
    assert!(rail.width < drawer.width);

    // 同一个「药丸 = 容器宽 − 2×内缩」公式
    for pane in [rail, drawer] {
        assert!(
            (pane.pill_width + pane.pill_inset * 2.0 - pane.width).abs() < 0.01,
            "{:?} 的药丸宽不是从容器宽派生的",
            pane.layout
        );
        // 圆角语言不变：都是 CornerLargeEnd（外侧直角、内侧 16）
        assert_eq!((pane.radius.top_left, pane.radius.bottom_left), (0.0, 0.0));
        assert_eq!(
            (pane.radius.top_right, pane.radius.bottom_right),
            (16.0, 16.0)
        );
        assert_eq!(pane.icon, 24.0);
    }

    // rail 的药丸是 56×32（M68 已修正的结论），不是 drawer 的 216×56
    assert_eq!((rail.pill_width, rail.pill_height), (56.0, 32.0));
    assert_eq!((drawer.pill_width, drawer.pill_height), (216.0, 56.0));
}

/// 6.1 / 6.2 / 6.6：Tabs 48 高、指示条 3、Secondary 线 1。
#[test]
fn tabs_geometry_matches_the_reference() {
    let ui = ui();
    let geometry = TabsGeometry::of(&ui);
    assert_eq!(geometry.height, 48.0, "G22");
    assert_eq!(geometry.icon, 24.0);
    assert_eq!(geometry.pad, 16.0);
    assert_eq!(geometry.bar(true), 3.0, "ActiveIndicatorHeight");
    assert_eq!(geometry.bar(false), 1.0, "DividerHeight");
    // RoundedCornerShape(3dp) 落在 3px 高的条上 = 半径 1.5（与 Full 同渲染）
    assert_eq!(geometry.indicator_radius().top_left, 1.5);
}

/// 16.1 / 16.5：AppBar 56（LM）、图标 24、左右 4、头像 32。
#[test]
fn app_bar_geometry_matches_the_skeleton() {
    let geometry = AppBarGeometry::of(&ui());
    assert_eq!(
        geometry.height, 56.0,
        "LM.page_header_height（A1 64，登记偏离）"
    );
    assert_eq!((geometry.icon, geometry.edge), (24.0, 4.0));
    assert_eq!(geometry.avatar, 32.0);
}
