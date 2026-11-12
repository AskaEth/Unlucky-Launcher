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
    let selected = item_palette(&ui, PANEL_BASE, Interaction::Enabled, true);
    assert_eq!(selected.fill, Some(ui.color(Role::SecondaryContainer)));
    assert_eq!(selected.ink, ui.color(Role::OnSecondaryContainer));
    assert_eq!(selected.icon, ui.color(Role::OnSecondaryContainer));

    // 选中 + hover：状态层叠在 SecondaryContainer 上（不是满档底色）
    let hovered = item_palette(&ui, PANEL_BASE, Interaction::Hovered, true);
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

    let disabled = item_palette(&ui, PANEL_BASE, Interaction::Disabled, true);
    assert_eq!(disabled.fill, None, "禁用项不留满档选中底色");
    assert_eq!(
        disabled.ink,
        interaction::disabled_content_over_role(&ui, PANEL_BASE, Role::OnSurface, false)
    );
    assert_ne!(disabled.ink, ui.color(Role::OnSurface));
}

/// 五态：hover 8% / pressed 10% / focus 10% 都叠在 SurfaceContainer 上（不透明）。
#[test]
fn item_states_blend_on_the_menu_container() {
    let ui = ui(SchemeFlavor::Light);
    let rest = item_palette(&ui, PANEL_BASE, Interaction::Enabled, false);
    assert_eq!(rest.fill, None, "静息不叠色（菜单底色透出来）");
    for state in [
        Interaction::Hovered,
        Interaction::Pressed,
        Interaction::Focused,
    ] {
        let fill = item_palette(&ui, PANEL_BASE, state, false)
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

/// M238：`flat()`（内联列表口径）**整块阴影不画**，不是「把 α 调小」。
///
/// 抓得住：有人把 `flat` 实现成「α × 0.5」或只改 blur（`Shadow{ alpha>0, blur=0 }` 在 tiny-skia 里
/// 仍会走阴影分支），以及有人把 `menu()` 里那个 `panel_shadow(.., menu.shadow)` 的第三参写死成
/// `true`（那样 flat 菜单又带上阴影、漏回视口外）。
/// 抓不住：阴影**画出来**的效果（`Element` 没有样式内省）；这条只钉决策函数，
/// 画出来的证据是 736 那张像素图（`tests/options_shot/compact.rs`）。
#[test]
fn flat_menus_skip_the_shadow_instead_of_toning_it_down() {
    let ui = ui(SchemeFlavor::Light);
    let spec = Elevation::Level2.shadow();

    let elevated = panel_shadow(&ui, spec, true);
    assert_eq!(elevated.blur_radius, 6.0);
    assert_eq!((elevated.offset.x, elevated.offset.y), (0.0, 2.0));
    assert_eq!(elevated.color.a, 0.14, "浮层：令牌给的 α 原样上身");

    let flat = panel_shadow(&ui, spec, false);
    assert_eq!(
        (flat.color.a, flat.blur_radius),
        (0.0, 0.0),
        "内联列表：α 与 blur 都归零（tiny-skia 见 α = 0 直接跳过整块阴影，不是淡一点）"
    );
}
