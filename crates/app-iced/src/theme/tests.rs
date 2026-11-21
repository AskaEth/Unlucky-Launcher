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

// ---------------------------------------------------------------------------
// M442：渲染档（D27）在 `Ui` 上的载体 —— 默认值 / 只改一个字段 / 真值翻译
// ---------------------------------------------------------------------------

/// ① 默认值 = GPU：**每一条构造路径**都得如此（不只在 `new` 上验）。
///
/// 抓得住：某个构造函数漏了 `render_tier` 那一行（默认值只剩一条路径对）。
/// 抓不住：默认值选得对不对（那是 `theme::tier` 模块文档里的产品口径，测试判不了）。
#[test]
fn every_constructor_defaults_the_render_tier_to_gpu() {
    let scheme = *ui(SchemeFlavor::Dark).scheme();
    let cases = [
        ("with_platform", ui(SchemeFlavor::Light)),
        ("new", Ui::new(SchemeFlavor::Dark, FAMILIES)),
        ("brand", Ui::brand(BrandBoard::Ink, FAMILIES)),
        (
            "with_board",
            Ui::with_board(BrandBoard::Silver, Platform::Windows, FAMILIES),
        ),
        ("baseline", Ui::baseline(SchemeFlavor::Light, FAMILIES)),
        ("from_scheme", Ui::from_scheme(scheme, FAMILIES)),
        (
            "with_accent",
            Ui::with_accent(
                SchemeFlavor::Dark,
                unlucky_design::color::Color::rgb(0x0F, 0x9D, 0x58),
                FAMILIES,
            ),
        ),
        (
            "tokens",
            tokens(SchemeFlavor::Light, None, None, FAMILIES).0,
        ),
    ];
    for (label, evaluator) in cases {
        assert_eq!(
            evaluator.render_tier(),
            RenderTier::Gpu,
            "{label} 的默认档不是 GPU（D27 产品默认路径）"
        );
    }
}

/// ② `with_render_tier` 只改那一个字段：49 个角色 / 三个材质层 / 面板 α / 主题名 / 深浅 / 色板 /
/// 骨架 / 字体族逐项与改之前相同。
///
/// 抓得住：把档位写进了颜色或 α（「顺手改了渲染」就是这一类）。
/// 抓不住：玻璃那头有没有真的按档位取值（本 mission 不接玻璃，见 `theme::tier` 的模块文档）。
#[test]
fn with_render_tier_changes_the_tier_and_nothing_else() {
    let base = ui(SchemeFlavor::Dark).with_panel_alpha(0.72);
    let cpu = base.clone().with_render_tier(RenderTier::Cpu);
    assert_eq!(cpu.render_tier(), RenderTier::Cpu);
    // 也能改回 GPU（构造器不是单向的）
    assert_eq!(
        cpu.clone().with_render_tier(RenderTier::Gpu).render_tier(),
        RenderTier::Gpu
    );

    for role in Role::ALL {
        assert_eq!(cpu.color(role), base.color(role), "{role:?} 被改了");
    }
    for layer in [
        MaterialLayer::Canvas,
        MaterialLayer::Card,
        MaterialLayer::Overlay,
    ] {
        assert_eq!(cpu.layer(layer), base.layer(layer), "{layer:?} 被改了");
    }
    assert_eq!(cpu.panel_alpha(), base.panel_alpha());
    assert_eq!(cpu.name(), base.name());
    assert_eq!(cpu.board(), base.board());
    assert_eq!(cpu.flavor(), base.flavor());
    assert_eq!(cpu.platform(), base.platform());
    assert_eq!(cpu.metrics(), base.metrics());
    assert_eq!(cpu.families(), base.families());
    assert_eq!(cpu.card_shape(), base.card_shape());
    assert_eq!(cpu.theme().to_string(), base.theme().to_string());
}

/// ③ 真值翻译：`ICED_BACKEND` 的每一项都有对应档位，`None` 不编一个「未知态」出来。
///
/// 抓得住：映射反了（CPU 机器拿到 GPU 档 = D27 会放行玻璃）、日志措辞变了（验收靠这行）。
/// 抓不住：产品路径有没有真的把值传进 `Ui`（那条由真二进制的 `[render]` 日志举证）。
#[test]
fn of_kind_maps_the_backend_and_line_reports_its_source() {
    use crate::renderer::Kind;

    assert_eq!(tier::of_kind(Some(Kind::Wgpu)), RenderTier::Gpu);
    assert_eq!(tier::of_kind(Some(Kind::TinySkia)), RenderTier::Cpu);
    // 未设 = 「不是本 crate 判的」（单测 / 夹具 / 外部手设）⇒ 产品默认档，不是第三种状态。
    assert_eq!(tier::of_kind(None), RenderTier::Gpu);

    assert_eq!(tier::name(RenderTier::Gpu), "gpu");
    assert_eq!(tier::name(RenderTier::Cpu), "cpu");
    assert_eq!(
        tier::line(Some(Kind::TinySkia)),
        "tier=cpu source=ICED_BACKEND=tiny-skia"
    );
    assert_eq!(
        tier::line(Some(Kind::Wgpu)),
        "tier=gpu source=ICED_BACKEND=wgpu"
    );
    assert_eq!(tier::line(None), "tier=gpu source=ICED_BACKEND 未设");
}
