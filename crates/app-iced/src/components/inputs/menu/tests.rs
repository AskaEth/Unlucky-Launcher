//! menu 的单测（从 `menu.rs` 拆出：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;
use unlucky_design::color::SchemeFlavor;
use unlucky_design::glass::{GlassSpec, GlassTier, OutlineSpec, RenderTier, RimEdge};
use unlucky_design::typography::Platform;

use crate::theme::{to_iced, Families};

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
    // 六档的具体数值由令牌自己的用例钉死
    // （`unlucky_design::elevation::tests::shadow_ladder_matches_the_visual_spec_table`）；
    // 这里只钉菜单这一侧的语义：菜单是浮层，必须拿到可见阴影。
    assert!(shadow.is_visible(), "菜单是浮层，必须有可见阴影");
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
    // 选中 + pressed：M296 判据 2 的单测半边——10% 与 hover 的 8% 必须能区分
    let pressed = item_palette(&ui, PANEL_BASE, Interaction::Pressed, true);
    assert_ne!(pressed.fill, selected.fill, "选中 + pressed 必须与静息不同");
    assert_ne!(pressed.fill, hovered.fill, "8% 与 10% 必须能区分");

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

/// **未选中项也吃 A8 的时间轴**（M314 / P27-2，与 `navigation/list_item.rs` 同一条口径）。
///
/// 抓得住：未选中支被改回 `overlay_color`（不看 `layer::current()` ⇒ 中间帧 == 端点，第二句
/// `assert_ne` 红）。抓不住：观感与真机像素（50ms 腿上只差几级色阶，只有图能判）。
#[test]
fn an_unselected_item_fades_its_state_layer_instead_of_snapping() {
    use crate::app::components::interaction::layer;

    layer::reset();
    let ui = ui(SchemeFlavor::Light);

    // ① 端点逐位等于旧口径（没有取样控件时「静息 / 到位帧不变」）。
    let hovered_end = interaction::overlay_color(
        &ui,
        Interaction::Hovered,
        Role::OnSurface,
        PANEL_BASE.role(),
    );
    assert_eq!(
        item_palette(&ui, PANEL_BASE, Interaction::Hovered, false).fill,
        hovered_end,
        "端点必须逐位等于 `overlay_color`（取证图与旧基线一致）"
    );

    // ② 中间帧：插值色，既不是端点也不是面板底，且不透明。
    let mut state = layer::Layer::new();
    state.retarget(
        Interaction::Hovered,
        unlucky_design::motion::MotionScale::STANDARD,
    );
    layer::advance_by(16.0);
    let mid = layer::with_current(state, || {
        item_palette(&ui, PANEL_BASE, Interaction::Hovered, false).fill
    })
    .expect("过渡中必须有底色");
    assert_eq!(mid.a, 1.0, "中间帧也必须不透明（D27）");
    assert_ne!(
        Some(mid),
        hovered_end,
        "中间帧不许等于终点色 —— 相等就等于硬切（未选中支被改回 `overlay_color` 时红在这里）"
    );
    assert_ne!(
        mid,
        ui.color(PANEL_BASE.role()),
        "中间帧也不许停在「没有状态层」的面板底上"
    );
    layer::reset();
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
    // 数值一律从令牌读：这条钉的是「`panel_shadow` 把 spec 原样透传」，不是某个具体数。
    assert_eq!(elevated.blur_radius, spec.blur_px, "浮层：blur 原样上身");
    assert_eq!(
        (elevated.offset.x, elevated.offset.y),
        (0.0, spec.offset_y_px),
        "浮层：offset 是 (0, offset_y)，原样上身"
    );
    assert_eq!(elevated.color.a, spec.alpha, "浮层：令牌给的 α 原样上身");

    let flat = panel_shadow(&ui, spec, false);
    assert_eq!(
        (flat.color.a, flat.blur_radius),
        (0.0, 0.0),
        "内联列表：α 与 blur 都归零（tiny-skia 见 α = 0 直接跳过整块阴影，不是淡一点）"
    );
}

/// M443：面板边界 = S3 的 rim（GPU 档）/ 1px `outline_variant`（CPU 档，§1.6）。
///
/// 抓得住：① CPU 档也接上 rim（§1.6 要求无 rim）；② 有人把 rim 的「颜色×α」原样交给 `Border`
/// 而不预合成（`border.color` 会带上 α，`a == 1.0` 那条红——GPU 后端会把边框像素的填充换掉，
/// 那道边会掉到 +3 级、肉眼不可见，见 `components/rim.rs`）；③ 有人自己拼 outline 的颜色；
/// ④ 有人把圆角换成 S3 的 12。
/// 抓不住：这圈边**画出来**的像素（`Element` 没有样式内省）——那条的证据是
/// `DevDocs/artifacts/m443/` 里 CPU / GPU、浅色 / 深色各一对改前改后图。
#[test]
fn the_panel_edge_is_the_s3_rim_on_gpu_and_a_hairline_outline_on_cpu() {
    let geometry = MenuGeometry::of(&ui(SchemeFlavor::Light));
    let rim = GlassSpec::for_tier(GlassTier::S3, RenderTier::Gpu).rim;
    assert_eq!(
        rim.edge,
        RimEdge::All,
        "S3 是四边全画，与 iced 的 Border 一一对应"
    );

    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let base = ui(flavor);
        let radius = base.radius_dp(geometry.shape);
        let fill = base.color(PANEL_BASE.role());

        // ① GPU 档：S3 的 rim（1px，四边同色；§1.2 顶部光源 = 暗白 / 亮黑），预合成成实色
        let border = panel_border(
            &base.clone().with_render_tier(RenderTier::Gpu),
            PANEL_TIER,
            fill,
            radius,
        );
        assert_eq!(border.width, rim.width_px, "宽度只能来自令牌");
        let ink = to_iced(rim.color.get(flavor));
        let alpha = rim.alpha.get(flavor);
        let rest = 1.0 - alpha;
        assert_eq!(
            [border.color.r, border.color.g, border.color.b],
            [
                ink.r * alpha + fill.r * rest,
                ink.g * alpha + fill.g * rest,
                ink.b * alpha + fill.b * rest
            ],
            "{flavor:?}：边 = 面板底 ⊕ rim（预合成，不是原始 rim 色）"
        );
        assert_eq!(border.color.a, 1.0, "{flavor:?}：预合成后必须是实色");

        // ② CPU 档：没有 rim，改 1px `outline_variant`——角色与宽度都来自令牌
        let border = panel_border(
            &base.clone().with_render_tier(RenderTier::Cpu),
            PANEL_TIER,
            fill,
            radius,
        );
        assert_eq!(
            border.color,
            base.color(Role::OutlineVariant),
            "CPU 档的边界就是 outline_variant（逐位相等 ⇒ 没自己拼色）"
        );
        assert_eq!(border.width, OutlineSpec::OUTLINE_VARIANT.width_px);
        // ③ 两档圆角一致（§1.6：降级只换材质，不换几何）；口径冲突（Menu=4 vs S3=12）保持现状
        assert_eq!(border.radius.top_left, radius.top_left);
        assert_eq!(border.radius.top_left, 4.0, "令牌表 Menu = ExtraSmall 4");
    }
}

/// 预合成的**前提**：面板底不透明（`components/rim.rs` 的 `fill ⊕ rim` 只在实底上成立）。
///
/// 这条红了的含义不是回归，而是「前提没了」：等 tint α 落地把面板变成玻璃，预合成的底必须换成
/// 模糊后的背景，或改回真描边（M443 报告里的 follow-up）。
#[test]
fn the_rim_composite_assumes_an_opaque_panel() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ui = ui(flavor);
        assert_eq!(
            ui.color(PANEL_BASE.role()).a,
            1.0,
            "{flavor:?}：面板底必须不透明"
        );
    }
    // 玻璃表自己的 tint α 是半透明的（0.86），但**今天没接到面板上**——所以前提成立。
    let tint = GlassSpec::for_tier(GlassTier::S3, RenderTier::Gpu)
        .tint_alpha
        .light;
    assert!(
        tint < 1.0,
        "S3 的 tint α 是半透明的（{tint}），接上就不满足本前提"
    );
}

/// M440 的硬结论：**只接 rim 一列**——玻璃表的 tint 与菜单面板底同值，shadow 那列不是菜单这一档。
#[test]
fn the_glass_columns_we_do_not_take_stay_out_of_the_panel() {
    let glass = GlassSpec::for_tier(GlassTier::S3, RenderTier::Gpu);
    // tint：与 `PANEL_BASE` 同一个角色 ⇒ 接不接都不改面板色（M440：同值）
    assert_eq!(glass.tint, PANEL_BASE.role(), "S3 的 tint 就是菜单面板底");
    // shadow：S3 那格是 L4，菜单按令牌表是 L2 ⇒ 取了要么叠两层、要么顶替 L2
    assert_eq!(glass.shadow, Some(Elevation::Level4.shadow()));
    assert_ne!(
        glass.shadow,
        Some(Component::Menu.spec().elevation.shadow()),
        "S3 的 shadow 列不是菜单那一档：不许取"
    );
}
