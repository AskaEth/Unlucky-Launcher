//! `drawer` 的单元测试（单文件 ≤400 行，AGENTS.md §3-1 拆出来）。

use super::super::super::interaction::{self, Interaction};
use super::item::palette;
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
#[test]
fn drawer_selected_uses_secondary_container() {
    let ui = ui();
    let palette = palette(&ui, NavLayout::Drawer, Interaction::Enabled, true);
    assert_eq!(palette.pill, Some(ui.color(Role::SecondaryContainer)));
    assert_eq!(palette.icon, ui.color(Role::OnSecondaryContainer));
    assert_eq!(palette.label, ui.color(Role::OnSecondaryContainer));
}

/// 12b §5.6：**rail** 的选中标签是 `Secondary`（与 drawer 不同，别串线）。
#[test]
fn rail_selected_label_is_secondary() {
    let ui = ui();
    let rail = palette(&ui, NavLayout::Rail, Interaction::Enabled, true);
    let drawer = palette(&ui, NavLayout::Drawer, Interaction::Enabled, true);
    assert_eq!(rail.label, ui.color(Role::Secondary));
    assert_ne!(rail.label, drawer.label, "两条线的选中标签角色不同");
    // 但图标与药丸相同（同一套语言）
    assert_eq!(rail.icon, drawer.icon);
    assert_eq!(rail.pill, drawer.pill);
}

/// 未选中：静息 `OnSurfaceVariant`；hover/focus/pressed 提到 `OnSurface` 且叠状态层。
#[test]
fn inactive_lifts_to_on_surface() {
    let ui = ui();
    let resting = palette(&ui, NavLayout::Drawer, Interaction::Enabled, false);
    assert_eq!(resting.label, ui.color(Role::OnSurfaceVariant));
    assert_eq!(resting.pill, None);
    for state in [
        Interaction::Hovered,
        Interaction::Focused,
        Interaction::Pressed,
    ] {
        let active = palette(&ui, NavLayout::Drawer, state, false);
        assert_eq!(active.label, ui.color(Role::OnSurface), "{state:?}");
        assert_eq!(active.icon, ui.color(Role::OnSurface), "{state:?}");
        assert!(active.pill.is_some(), "{state:?} 必须叠状态层");
    }
}

/// 禁用：`OnSurface`@38%，不画药丸，不叠状态层。
#[test]
fn disabled_draws_no_pill() {
    let ui = ui();
    let palette = palette(&ui, NavLayout::Drawer, Interaction::Disabled, true);
    assert_eq!(
        palette.icon,
        interaction::disabled_content_over_role(&ui, Role::OnSurface, false)
    );
    assert_eq!(palette.label, palette.icon);
    assert_eq!(palette.pill, None);
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
