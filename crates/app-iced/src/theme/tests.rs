//! `theme` 的单元测试（M229 从 `theme.rs` 纯搬移，守 400 行硬线：加双色板字段前该文件正好 400 行）。
//!
//! 搬移只把 `mod tests { … }` 的包裹层换成了独立文件；**搬移之后内容有改动**（不是一行未改）：
//! 主题名从 `ull-m3-*` 改成 `ull-ink/ull-silver`、「默认构造逐字段等于 baseline」改成
//! 「逐字段等于**品牌板**」，并新增了色板/卡片圆角/取色不改深浅等几条测试。

use super::*;

const FAMILIES: Families = Families {
    text: "Segoe UI Variable Text",
    display: "Segoe UI Variable Display",
    mono: "Cascadia Mono",
};

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

#[test]
fn theme_name_tracks_the_flavor() {
    assert_eq!(ui(SchemeFlavor::Light).name(), "ull-silver");
    assert_eq!(ui(SchemeFlavor::Dark).name(), "ull-ink");
    assert_ne!(
        ui(SchemeFlavor::Light).theme().to_string(),
        ui(SchemeFlavor::Dark).theme().to_string()
    );
}

#[test]
fn canvas_card_overlay_follow_surface_containers() {
    let ui = ui(SchemeFlavor::Light);
    assert_eq!(ui.layer(MaterialLayer::Canvas), ui.color(Role::Surface));
    assert_eq!(
        ui.layer(MaterialLayer::Card),
        ui.color(Role::SurfaceContainerLow)
    );
    assert_eq!(
        ui.layer(MaterialLayer::Overlay),
        ui.color(Role::SurfaceContainerHigh)
    );
}

#[test]
fn radii_only_come_from_the_scale() {
    let ui = ui(SchemeFlavor::Light);
    // 卡片 Medium(12)：边长够大时就是 12
    assert_eq!(ui.radius_dp(ShapeScale::Medium).top_left, 12.0);
    assert_eq!(ui.radius_dp(ShapeScale::ExtraSmall).top_left, 4.0);
    // 药丸 Full：按边长一半折算
    assert_eq!(ui.radius(ShapeScale::Full, 32.0).top_left, 16.0);
    // 不许超过边长一半
    assert_eq!(ui.radius(ShapeScale::ExtraLarge, 20.0).top_left, 10.0);
}

#[test]
fn outlines_are_one_pixel_and_use_outline_variant() {
    let ui = ui(SchemeFlavor::Light);
    let border = ui.outlined(ShapeScale::Medium);
    assert_eq!(border.width, STROKE_PX);
    assert_eq!(border.width, 1.0, "rubric G7：全站 1px");
    assert_eq!(border.color, ui.color(Role::OutlineVariant));
    assert_eq!(border.radius.top_left, 12.0);
}

#[test]
fn emphasized_weights_are_the_official_ones() {
    let ui = ui(SchemeFlavor::Light);
    // title-small 的强调档是 700（官方 Emphasized）
    assert_eq!(
        ui.emphasized_font(TypeRole::TitleSmall).weight,
        iced::font::Weight::Bold
    );
    // baseline 正文是 400
    assert_eq!(
        ui.font(TypeRole::BodyMedium).weight,
        iced::font::Weight::Normal
    );
}

#[test]
fn body_text_keeps_wcag_contrast_in_both_flavors() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let scheme = Scheme::baseline(flavor);
        assert!(
            scheme.contrast(Role::OnSurface, Role::Surface) >= 4.5,
            "{flavor:?} 的正文对比度不够"
        );
        assert!(
            scheme.contrast(Role::OnSecondaryContainer, Role::SecondaryContainer) >= 4.5,
            "{flavor:?} 的选中态药丸对比度不够"
        );
    }
}

#[test]
fn every_role_maps_to_an_iced_color() {
    for role in Role::ALL {
        let color = ui(SchemeFlavor::Dark).color(role);
        assert!((0.0..=1.0).contains(&color.a), "{role:?} 的 alpha 越界");
    }
}

#[test]
fn state_layers_are_opaque_so_cpu_and_gpu_match() {
    let ui = ui(SchemeFlavor::Light);
    let hovered = ui.state_over(StateLayer::Hover, Role::OnSurface, Role::Surface);
    assert_eq!(hovered.a, 1.0, "状态层必须给出不透明结果");
    assert_ne!(hovered, ui.color(Role::Surface));
}

/// D38：配色方案必须真的换色；显式传 baseline 时逐字段等于 baseline。
/// 抓不住：取色好不好看、设置页是否真切过去（后者由 app 侧日志举证）。
#[test]
fn a_resolved_scheme_replaces_the_baseline_colours() {
    use unlucky_design::palette::{resolve_source, CardKey, ImageInput};

    // 4×4 的纯红图：种子彩度远高于门槛，必然走到「背景图取色」这一级。
    let pixels = [0xC0_u8, 0x20, 0x20, 0xFF].repeat(16);
    let input = ImageInput::pixels(&pixels, 4, 4);
    let (scheme, provenance) =
        resolve_source(Some(input), None, Some(CardKey::Sky), SchemeFlavor::Light);
    assert_eq!(
        provenance.level,
        unlucky_design::palette::Level::BackgroundImage,
        "纯红图必须取色成功（否则本测试的输入不合格）"
    );

    let tinted = Ui::from_scheme(scheme, FAMILIES);
    let board = ui(SchemeFlavor::Light);
    assert_ne!(
        tinted.color(Role::Primary),
        board.color(Role::Primary),
        "取色方案必须换掉 primary"
    );
    // 深浅仍跟着方案走（方案自带 flavor），不会把 Light 变成 Dark；色板名也仍说实话。
    assert_eq!(tinted.flavor(), SchemeFlavor::Light);
    assert_eq!(tinted.name(), board.name());

    // 显式传 baseline 时逐块等于 baseline 口径（M229 之后产品默认不再是它，但这条通路必须诚实）。
    let explicit = Ui::from_scheme(Scheme::baseline(SchemeFlavor::Light), FAMILIES);
    let baseline = Ui::baseline(SchemeFlavor::Light, FAMILIES);
    for role in Role::ALL {
        assert_eq!(
            explicit.color(role),
            baseline.color(role),
            "{role:?}：显式传 baseline 时颜色必须与 baseline 口径一致"
        );
    }
}

/// M229：`Ui::new` 走**品牌色板**（Ink 深 / Silver 浅），不再走 tone 派生的 `baseline`。
///
/// 抓得住：换肤没接上（色值仍是 baseline）、深浅映射反了（Light 拿到 Ink）。
/// 抓不住：色板好不好看、页面有没有绕过 `Ui::color` 自己写色值（后者由 `no_hardcoded_tokens` 扫）。
#[test]
fn every_ui_is_built_from_a_brand_board() {
    for (flavor, board) in [
        (SchemeFlavor::Light, BrandBoard::Silver),
        (SchemeFlavor::Dark, BrandBoard::Ink),
    ] {
        let evaluator = ui(flavor);
        assert_eq!(evaluator.board(), board, "{flavor:?} 该用 {:?}", board);
        assert_eq!(evaluator.name(), board_name(board));
        for role in Role::ALL {
            assert_eq!(
                evaluator.color(role),
                to_iced(board.get(role)),
                "{flavor:?} 的 {role:?} 不是色板值"
            );
        }
        // 与 tone 派生的 baseline 确实不同（否则「换肤」这件事没发生）。
        let baseline = Ui::baseline(flavor, FAMILIES);
        assert_ne!(
            evaluator.color(Role::Primary),
            baseline.color(Role::Primary),
            "{flavor:?}：色板与 baseline 的 primary 撞了，换肤等于没接"
        );
        assert_eq!(evaluator.flavor(), flavor);
    }
}

/// 显式选板时深浅跟着板走（`Ink = Dark` / `Silver = Light`），「跟随系统」不必另造一种板。
#[test]
fn picking_a_board_sets_the_flavor_from_the_board() {
    let ink = Ui::with_board(BrandBoard::Ink, Platform::Windows, FAMILIES);
    let silver = Ui::with_board(BrandBoard::Silver, Platform::Windows, FAMILIES);
    assert_eq!(ink.flavor(), SchemeFlavor::Dark);
    assert_eq!(silver.flavor(), SchemeFlavor::Light);
    assert_ne!(ink.theme().to_string(), silver.theme().to_string());
}

/// 卡片方框走规格卡（`rx=16`），不是组件表的 `Medium(12)`。
#[test]
fn cards_take_the_spec_corner() {
    let evaluator = ui(SchemeFlavor::Light);
    assert_eq!(evaluator.card_shape(), ShapeScale::Large);
    assert_eq!(evaluator.card_box().radius_px(), 16.0);
    assert_eq!(
        evaluator.outlined(evaluator.card_shape()).radius.top_left,
        16.0
    );
}

fn board_name(board: BrandBoard) -> &'static str {
    match board {
        BrandBoard::Ink => "ull-ink",
        BrandBoard::Silver => "ull-silver",
    }
}
