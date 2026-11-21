//! `drawer` 的单元测试（单文件 ≤400 行，AGENTS.md §3-1 拆出来）。

use super::super::super::interaction::{self, Interaction};
use super::item::{palette, pill_color};
use super::*;

use unlucky_design::color::SchemeFlavor;
use unlucky_design::icons::IconKind;
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

/// §21.7：选中项图标/标签都是 `OnSecondaryContainer`，药丸 `SecondaryContainer`。
///
/// M296：药丸在选中时**也**吃 hover / pressed（实色容器上叠状态层），三态两两不同。
/// 抓得住：选中分支写回「不叠层」（`hover == pressed == rest` 一起红）；
/// 抓不住：图上差几个色阶（`DevDocs/artifacts/selstate` 的裁图判据）。
#[test]
fn drawer_selected_uses_secondary_container() {
    let ui = ui();
    let palette = palette(&ui, BASE, NavLayout::Drawer, Interaction::Enabled, true);
    assert_eq!(
        pill_color(&ui, BASE, Interaction::Enabled, true),
        Some(ui.color(Role::SecondaryContainer))
    );
    assert_eq!(palette.icon, ui.color(Role::OnSecondaryContainer));
    assert_eq!(palette.label, ui.color(Role::OnSecondaryContainer));

    let rest = pill_color(&ui, BASE, Interaction::Enabled, true);
    let hover = pill_color(&ui, BASE, Interaction::Hovered, true);
    let pressed = pill_color(&ui, BASE, Interaction::Pressed, true);
    assert_ne!(hover, rest, "选中 + hover 必须与静息不同");
    assert_ne!(pressed, rest, "选中 + pressed 必须与静息不同");
    assert_ne!(hover, pressed, "8% 与 10% 必须能区分");
    assert_eq!(
        hover,
        Some(interaction::selected_fill(
            &ui,
            Interaction::Hovered,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer
        )),
        "叠层必须经唯一入口求值"
    );
}

/// 12b §5.6：**rail** 的选中标签是 `Secondary`（与 drawer 不同，别串线）。
#[test]
fn rail_selected_label_is_secondary() {
    let ui = ui();
    let rail = palette(&ui, BASE, NavLayout::Rail, Interaction::Enabled, true);
    let drawer = palette(&ui, BASE, NavLayout::Drawer, Interaction::Enabled, true);
    assert_eq!(rail.label, ui.color(Role::Secondary));
    assert_ne!(rail.label, drawer.label, "两条线的选中标签角色不同");
    // 图标与药丸相同（同一套语言）。
    //
    // 复审 P2-1：这里原先写成 `pill_color(..) == pill_color(..)`——比的是同一个表达式，恒真，
    // 抓不到任何东西（注释却还在说「两条线的药丸一致」）。药丸从 M257 起**不吃 layout**
    // （`pill_color` 的签名里根本没有 `NavLayout`），所以「两条线一致」由**签名**保证；
    // 本用例改判真正要守的那件事：静息选中药丸就是 `SecondaryContainer` 这一档
    // （换成 `Primary` 立刻红，变异原文见 M257 r2 报告）。
    assert_eq!(rail.icon, drawer.icon, "图标与布局无关");
    assert_eq!(
        pill_color(&ui, BASE, Interaction::Enabled, true),
        Some(ui.color(Role::SecondaryContainer)),
        "两条线的静息选中药丸都必须来自 SecondaryContainer"
    );
}

/// 未选中：静息 `OnSurfaceVariant`；hover/focus/pressed 提到 `OnSurface` 且叠状态层。
#[test]
fn inactive_lifts_to_on_surface() {
    let ui = ui();
    let resting = palette(&ui, BASE, NavLayout::Drawer, Interaction::Enabled, false);
    assert_eq!(resting.label, ui.color(Role::OnSurfaceVariant));
    assert_eq!(pill_color(&ui, BASE, Interaction::Enabled, false), None);
    for state in [
        Interaction::Hovered,
        Interaction::Focused,
        Interaction::Pressed,
    ] {
        let active = palette(&ui, BASE, NavLayout::Drawer, state, false);
        assert_eq!(active.label, ui.color(Role::OnSurface), "{state:?}");
        assert_eq!(active.icon, ui.color(Role::OnSurface), "{state:?}");
        assert!(
            pill_color(&ui, BASE, state, false).is_some(),
            "{state:?} 必须叠状态层"
        );
    }
}

/// 禁用：`OnSurface`@38%，不画药丸，不叠状态层。
#[test]
fn disabled_draws_no_pill() {
    let ui = ui();
    let palette = palette(&ui, BASE, NavLayout::Drawer, Interaction::Disabled, true);
    assert_eq!(
        palette.icon,
        interaction::disabled_content_over_role(&ui, BASE, Role::OnSurface, false)
    );
    assert_eq!(palette.label, palette.icon);
    assert_eq!(pill_color(&ui, BASE, Interaction::Disabled, true), None);
}

/// 禁用是**修饰器**：只清消息，图标 / 选中态全部保留。
#[test]
fn disabled_only_clears_the_message() {
    let item = NavItem::new("Settings", IconKind::Settings, ())
        .selected()
        .disabled();
    assert!(item.on_press.is_none());
    assert!(item.selected);
    assert_eq!(item.icon, IconKind::Settings);
    assert_eq!(item.label, "Settings");
}

/// D33：两种宽度都能建出树（同一个 `navigation` 入口）。
#[test]
fn both_layouts_build_a_tree() {
    let ui = ui();
    for layout in NavLayout::ALL {
        let base = match layout {
            NavLayout::Rail => Navigation::rail(),
            NavLayout::Drawer => Navigation::drawer(),
        };
        let _: Element<'_, ()> = navigation(
            &ui,
            base.group(
                NavGroup::titled("LIBRARY")
                    .item(NavItem::new("Instances", IconKind::Instances, ()).selected())
                    .item(NavItem::new("Download", IconKind::Download, ())),
            )
            .group(NavGroup::titled("SYSTEM").item(NavItem::new(
                "Settings",
                IconKind::Settings,
                (),
            ))),
        );
    }
}
