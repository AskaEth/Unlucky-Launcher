//! ELEVATED CARD 的几何/排版断言（对着规格卡原值，不看渲染结果）。
//!
//! 抓得住：圆角退回组件表的 12、内距/字级被改回规格卡的 24/16（元信息卡会折行）、
//! 上标注变成 500/700 这些「悄悄改数」。
//! 抓不住：底色对不对（要看 `Ui::color(SurfaceContainer)` 的来源，那条由 `theme` 的色板测试守）、
//! 以及卡在页面里好不好看（D30 视觉判定）。

use super::*;
use unlucky_design::shape::ShapeScale;

const FAMILIES: crate::theme::Families = crate::theme::Families {
    text: "Segoe UI Variable Text",
    display: "Segoe UI Variable Display",
    mono: "Cascadia Mono",
};

fn ui(flavor: unlucky_design::color::SchemeFlavor) -> Ui {
    Ui::with_platform(
        flavor,
        unlucky_design::typography::Platform::Windows,
        FAMILIES,
    )
}

#[test]
fn the_card_takes_the_spec_corner_and_the_tile_padding() {
    let ui = ui(unlucky_design::color::SchemeFlavor::Dark);
    assert_eq!(ui.card_shape(), ShapeScale::Large, "规格卡 rx=16");
    assert_eq!(ui.card_box().radius_px(), 16.0);
    // 内距 **20**（不是规格卡的 24）：24 会让元信息卡的值串折行（复审 P1-2），
    // 20 与描边卡同源（`LayoutMetrics::card_padding`），卡片宽度契约不变。
    assert_eq!(PAD, 20.0);
    assert_eq!(
        PAD,
        unlucky_design::spacing::LayoutMetrics::DESKTOP.card_padding
    );
    assert_eq!(PAD % unlucky_design::spacing::GRID, 0.0, "仍在 4px 栅格上");
}

#[test]
fn the_typography_matches_the_spec_rows() {
    let style = overline_style();
    assert_eq!(style.size_px, 11.0, "上标注 11px");
    assert_eq!(style.weight, Weight::SemiBold, "上标注 600（不是 500/700）");

    // 值的字级：**12**（BodySmall），不是规格卡展示卡的 16 —— 16px 下时戳墨水宽 138px，
    // 而 160 外框减两倍内距只有 120（复审 P1-2）。这条与 `pages/instances/tests.rs` 的不等式同源。
    let title = TITLE_ROLE.style();
    assert_eq!(TITLE_ROLE, TypeRole::BodySmall);
    assert_eq!(title.size_px, 12.0, "值 12px");
    assert_eq!(title.weight, Weight::Regular, "值 400");

    let support = TypeRole::BodyMedium.style();
    assert_eq!(support.size_px, 14.0, "副文案 13 → 取较大档 14");
    assert_eq!(support.weight, Weight::Regular);
}

/// 卡片是**靠层级差站立**的：底色必须是 `SurfaceContainer`（画布上一档），不是画布色本身。
#[test]
fn the_card_surface_sits_one_step_above_the_canvas() {
    for board in unlucky_design::color::BrandBoard::ALL {
        let scheme = board.scheme();
        assert_ne!(
            board.get(Role::SurfaceContainer),
            board.get(Role::Surface),
            "{}：卡底与画布同色，卡会「消失」",
            board.name()
        );
        // 无描边：卡的存在感只能来自这一档色差，所以必须 > 1.0（差多少属观感，D30 判）。
        let card_vs_canvas = scheme.contrast(Role::SurfaceContainer, Role::Surface);
        assert!(
            card_vs_canvas > 1.0,
            "{}：卡底对画布 {card_vs_canvas:.3}:1，看不出层级",
            board.name()
        );
    }
}

/// **M234**：卡片的底在代码里只出现两次——[`card`] 的 `background` 与导出的 [`BASE`]，
/// 这两处必须同角色，否则卡里的禁用控件会合到另一层底上。
///
/// 抓得住：有人只改 `BASE`（或只改 `card`）导致两边漂开；抓不住：「`card()` 真的用了
/// `BASE` 画底」这件事（`container::Style` 闭包不可内省，只能靠代码审阅与
/// `gallery::probe` 的 disabledbase 段——那边的像素判据要求卡列里出现「叠卡片底」的禁用容器）。
#[test]
fn the_card_base_constant_is_the_role_the_card_paints() {
    assert_eq!(
        super::BASE.role(),
        Role::SurfaceContainer,
        "`elevated::BASE` 必须就是卡底的角色（`card()` 的 background 用的是同一个值）"
    );
}
