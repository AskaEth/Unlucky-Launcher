//! 模态进场的**可判读数值取证**（M256 / tower 门槛 E）：缩放值与遮罩色逐档钉住。
//!
//! 抓得住：把淡入做回「调 α」（`a < 1` 即红，D27）、把 `progress = 1` 做成近似值（与
//! `backdrop_color` 不等即红）、把缩放方向做反。抓不住：**观感**（截图见 artifacts/motion1）。

use super::*;
use crate::app::components::motion;
use unlucky_design::color::SchemeFlavor;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::typography::Platform;

const FAMILIES: crate::theme::Families = crate::theme::Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn luma(color: Color) -> f32 {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b
}

/// 遮罩：`0` 是整块画布色（**把页面内容盖住**，不是透明）、`1` 是原遮罩（逐位相同）、
/// 全程不透明且单调变暗。
#[test]
fn the_scrim_fades_by_lerping_opaque_colors_never_by_alpha() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ui = crate::theme::Ui::with_platform(flavor, Platform::Windows, FAMILIES);
        let canvas = ui.layer(MaterialLayer::Canvas);
        let rest = backdrop_color(&ui);
        assert_eq!(
            backdrop_color_at(&ui, 0.0),
            canvas,
            "{flavor:?}: 起手同画布"
        );
        assert_eq!(
            backdrop_color_at(&ui, 1.0),
            rest,
            "{flavor:?}: 到位逐位相同"
        );

        let mut previous = luma(canvas);
        for step in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let color = backdrop_color_at(&ui, step);
            assert_eq!(color.a, 1.0, "{flavor:?}: 遮罩必须全程不透明（D27）");
            let current = luma(color);
            assert!(
                current <= previous + 1.0e-6,
                "{flavor:?}: 遮罩必须单调变暗（{previous} → {current}）"
            );
            previous = current;
            println!(
                "[motion1] {flavor:?} t={step:.2} scale={:.3} rgb({:.3},{:.3},{:.3})",
                motion::modal_scale(step),
                color.r,
                color.g,
                color.b
            );
        }
    }
}
