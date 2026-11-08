//! menu 的单测（从 `menu.rs` 拆出：单文件 ≤400 行，AGENTS.md §3-1）。

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

fn sample(flavor: SchemeFlavor) -> Element<'static, u8> {
    let ui = Box::leak(Box::new(ui(flavor)));
    let spec = Menu::new()
        .item(MenuItem::new("重命名", 1_u8).leading(IconKind::Folder))
        .item(
            MenuItem::new("复制实例", 2_u8)
                .selected()
                .trailing(IconKind::Check),
        )
        .divider()
        .item(MenuItem::disabled("删除").leading(IconKind::Trash));
    menu(ui, spec)
}

/// 8.2：容器 `SurfaceContainer` + L2；阴影参数来自高程令牌。
#[test]
fn container_uses_surface_container_and_level2_shadow() {
    let ui = ui(SchemeFlavor::Light);
    assert_eq!(Component::Menu.spec().elevation, Elevation::Level2);
    assert_eq!(
        Component::Menu.spec().elevation.surface_role(),
        Role::SurfaceContainer,
        "L2 的容器角色与 MenuTokens.ContainerColor 一致"
    );
    let shadow = Elevation::Level2.shadow();
    assert!(shadow.is_visible(), "菜单是浮层，必须有可见阴影");
    assert_eq!(shadow.blur_px, 6.0);
    assert_eq!(shadow.offset_y_px, 2.0);
    // 容器色**不是** MaterialLayer::Overlay（那会高一档）
    assert_ne!(
        ui.layer(unlucky_design::elevation::MaterialLayer::Overlay),
        ui.color(Role::SurfaceContainer)
    );
}

/// 8.6：选中 = SecondaryContainer + OnSecondaryContainer；禁用内容 38%。
#[test]
fn selected_and_disabled_items_use_the_right_roles() {
    let ui = ui(SchemeFlavor::Light);
    let selected = item_palette(&ui, Interaction::Enabled, true);
    assert_eq!(selected.fill, Some(ui.color(Role::SecondaryContainer)));
    assert_eq!(selected.ink, ui.color(Role::OnSecondaryContainer));
    assert_eq!(selected.icon, ui.color(Role::OnSecondaryContainer));

    // 选中 + hover：状态层叠在 SecondaryContainer 上（不是满档底色）
    let hovered = item_palette(&ui, Interaction::Hovered, true);
    assert_ne!(hovered.fill, selected.fill);
    assert_eq!(
        hovered.fill,
        interaction::overlay_color(
            &ui,
            Interaction::Hovered,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer
        )
    );

    let disabled = item_palette(&ui, Interaction::Disabled, true);
    assert_eq!(disabled.fill, None, "禁用项不留满档选中底色");
    assert_eq!(
        disabled.ink,
        interaction::disabled_content_over_role(&ui, Role::OnSurface, false)
    );
    assert_ne!(disabled.ink, ui.color(Role::OnSurface));
}

/// 五态：hover 8% / pressed 10% / focus 10% 都叠在 SurfaceContainer 上（不透明）。
#[test]
fn item_states_blend_on_the_menu_container() {
    let ui = ui(SchemeFlavor::Light);
    let rest = item_palette(&ui, Interaction::Enabled, false);
    assert_eq!(rest.fill, None, "静息不叠色（菜单底色透出来）");
    for state in [
        Interaction::Hovered,
        Interaction::Pressed,
        Interaction::Focused,
    ] {
        let fill = item_palette(&ui, state, false)
            .fill
            .expect("可交互态必须给底色");
        assert_eq!(fill.a, 1.0, "结果必须不透明（D27）");
        assert_ne!(Some(fill), rest.fill);
    }
}

/// 8.3 / 8.4：项高 48、水平 12（内容）、选中指示器内缩 4 ⇒ 内容仍在 12。
#[test]
fn item_geometry_matches_the_reference_table() {
    let ui = ui(SchemeFlavor::Light);
    let geometry = MenuGeometry::of(&ui);
    assert_eq!(geometry.item_height, 48.0);
    assert_eq!(geometry.pad_h, 12.0);
    assert_eq!(geometry.pad_v, 8.0);
    let inset = ui.space(Space::X1);
    assert_eq!(inset, 4.0);
    assert_eq!((geometry.pad_h - inset) + inset, 12.0, "内容落在 12 上");
}

/// 三种形态（带槽位 / 选中 / 禁用 / 分组线）都能构建。
#[test]
fn menu_shapes_build() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let _: Element<'static, u8> = sample(flavor);
    }
}
