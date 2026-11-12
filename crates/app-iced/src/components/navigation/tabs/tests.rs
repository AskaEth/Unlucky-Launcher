//! `tabs` 的单元测试（split 拆出来的：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;

use crate::theme::Families;

use super::super::super::interaction::{self, Base};

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

/// 6.5：选中色 Primary 用 `Primary`，Secondary 用 `OnSurface`（**不是** Primary）。
#[test]
fn active_colors_follow_the_variant() {
    let ui = ui();
    assert_eq!(
        content_color(&ui, Base::CANVAS, true, Interaction::Enabled, true),
        ui.color(Role::Primary)
    );
    assert_eq!(
        content_color(&ui, Base::CANVAS, false, Interaction::Enabled, true),
        ui.color(Role::OnSurface),
        "6.5：Secondary 的选中项是 OnSurface"
    );
    assert_ne!(
        content_color(&ui, Base::CANVAS, false, Interaction::Enabled, true),
        ui.color(Role::Primary),
        "把 Secondary 的选中色写成 Primary 是最常见的串线"
    );
}

/// 未选中静息 `OnSurfaceVariant`；hover / focus / pressed 提到 `OnSurface`。
#[test]
fn inactive_states_lift_to_on_surface() {
    let ui = ui();
    for primary in [true, false] {
        assert_eq!(
            content_color(&ui, Base::CANVAS, primary, Interaction::Enabled, false),
            ui.color(Role::OnSurfaceVariant)
        );
        for state in [
            Interaction::Hovered,
            Interaction::Focused,
            Interaction::Pressed,
        ] {
            assert_eq!(
                content_color(&ui, Base::CANVAS, primary, state, false),
                ui.color(Role::OnSurface),
                "{state:?} 必须提到 OnSurface"
            );
        }
    }
}

/// 6.2：Primary 只有选中项有 3px `Primary` 条；禁用项降档。
#[test]
fn primary_bar_only_on_the_selected_tab() {
    let ui = ui();
    assert_eq!(
        bar_color(&ui, Base::CANVAS, Interaction::Enabled, false),
        None
    );
    assert_eq!(
        bar_color(&ui, Base::CANVAS, Interaction::Enabled, true),
        Some(ui.color(Role::Primary))
    );
    assert_eq!(
        bar_color(&ui, Base::CANVAS, Interaction::Disabled, true),
        Some(interaction::disabled_content_over_role(
            &ui,
            Base::CANVAS,
            Role::OnSurface,
            false
        )),
        "禁用项「点不动却亮着」是错的"
    );
}

/// 6.6：Secondary 的分隔线整行一条，颜色 `SurfaceVariant`（不是 §22 的 `OutlineVariant`）。
#[test]
fn secondary_line_is_surface_variant() {
    let ui = ui();
    assert_eq!(secondary_line_color(&ui), ui.color(Role::SurfaceVariant));
    assert_ne!(
        secondary_line_color(&ui),
        ui.color(Role::OutlineVariant),
        "OutlineVariant 是 §22 分隔线的角色，Tabs 不用它"
    );
}

/// 禁用是**修饰器**：只清消息，label / 选中态 / 图标全部保留。
#[test]
fn disabled_only_clears_the_message() {
    let tab = Tab::new("Mods", ())
        .icon(unlucky_design::icons::IconKind::Settings)
        .selected()
        .disabled();
    assert!(tab.on_press.is_none());
    assert!(tab.icon.is_some());
    assert!(tab.selected);
    assert_eq!(tab.label, "Mods");
}

/// 指示条宽度 = tab 宽 = 文本宽 + 2×16（这条是「跟随选中项宽度」的公式面）。
#[test]
fn indicator_width_is_the_tab_width() {
    let geometry = TabsGeometry::of(&ui());
    let tab_width = |text_width: f32| text_width + geometry.pad * 2.0;
    assert_eq!(tab_width(0.0), 32.0, "空文本 = 2×16");
    // 两段不同长度的文案 ⇒ 指示条宽度必然不同（渲染层断言用它打红）
    let short = tab_width(40.0);
    let long = tab_width(120.0);
    assert_eq!(long - short, 80.0);
    assert!(long > short);
}

/// 6.1：带图标的 Tabs 容器高 64（`IconAndLabelTextContainerHeight`），纯文字 48。
#[test]
fn icon_tabs_use_the_taller_container() {
    let ui = ui();
    assert_eq!(TABS_ICON_HEIGHT, 64.0);
    assert!(TABS_ICON_HEIGHT > TabsGeometry::of(&ui).height);
}

/// 3 个 tab 的树能建出来（含选中 / 禁用 / 带图标三条路径）。
#[test]
fn view_builds_a_tree() {
    let ui = ui();
    let _: Element<'_, ()> = tabs(
        &ui,
        Tabs::primary()
            .tab(Tab::new("General", ()).selected())
            .tab(Tab::new("Mods", ()).icon(unlucky_design::icons::IconKind::Settings))
            .tab(Tab::new("Logs", ()).disabled()),
    );
    let _: Element<'_, ()> = tabs(
        &ui,
        Tabs::secondary()
            .tab(Tab::new("Release", ()).selected())
            .tab(Tab::new("Snapshot", ())),
    );
}
