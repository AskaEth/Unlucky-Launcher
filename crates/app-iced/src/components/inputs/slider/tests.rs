//! slider 的单测（从 `slider.rs` 拆出：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;
use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;

use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

fn track() -> Rectangle {
    Rectangle {
        x: 100.0,
        y: 200.0,
        width: 300.0,
        height: 48.0,
    }
}

/// 11.1 / 11.2 / 11.4：轨道 16、竖条 4×44、上下各溢出 14。
#[test]
fn track_and_handle_are_the_m3_expressive_shapes() {
    let g = SliderGeometry::of(&ui());
    assert_eq!(g.track, 16.0);
    assert_eq!((g.handle, g.handle_tall), (4.0, 44.0));
    assert_eq!((g.handle_tall - g.track) / 2.0, 14.0);
    assert_eq!(g.stop, 4.0);
    assert_eq!(g.height, 48.0, "命中区 = LM.touch_target");
}

/// 0% / 50% / 100% 三档的位置：竖条**永不越界**，激活段与轨道分界一致。
#[test]
fn thumb_stays_inside_the_track_at_both_ends() {
    let bounds = track();
    let zero = TrackGeometry::of(bounds, 0.0);
    assert_eq!(zero.handle_left, bounds.x, "0% 时竖条左缘 = 轨道左缘");
    assert!(zero.handle_left >= bounds.x);
    assert_eq!(zero.active_width, 2.0, "0% 时激活段只剩竖条半宽");

    let half = TrackGeometry::of(bounds, 0.5);
    assert!((half.handle_left - (bounds.x + bounds.width / 2.0 - 2.0)).abs() < 0.01);

    let full = TrackGeometry::of(bounds, 1.0);
    assert_eq!(
        full.handle_right,
        bounds.x + bounds.width,
        "100% 时右缘贴轨道右缘"
    );
    assert!(full.handle_right <= bounds.x + bounds.width);
    // 激活段画到**竖条中心**为止（= 右缘 − 2），不是画到右缘：100% 时也被竖条自己盖住
    assert_eq!(full.active_width, bounds.width - 2.0);
}

/// 越界的比例（<0 或 >1）被 clamp，不会把竖条推到轨道外。
#[test]
fn out_of_range_fractions_are_clamped() {
    let bounds = track();
    for fraction in [-3.0, -0.01, 0.0, 1.0, 1.5, 9.0] {
        let geometry = TrackGeometry::of(bounds, fraction);
        assert!(
            geometry.handle_left >= bounds.x,
            "{fraction} 的竖条左缘越界：{}",
            geometry.handle_left
        );
        assert!(
            geometry.handle_right <= bounds.x + bounds.width,
            "{fraction} 的竖条右缘越界：{}",
            geometry.handle_right
        );
    }
}

/// 离散值：4 段 → 1/4 步进；连续值原样。
#[test]
fn discrete_sliders_quantize_to_the_stops() {
    assert_eq!(quantize(0.3, Some(4)), 0.25);
    assert_eq!(quantize(0.4, Some(4)), 0.5);
    assert_eq!(quantize(1.4, Some(4)), 1.0);
    assert_eq!(quantize(0.3, None), 0.3);
    // 停靠点位置 = 五等分，且都落在轨道内
    let bounds = track();
    for index in 0..=4 {
        let geometry = TrackGeometry::of(bounds, index as f32 / 4.0);
        assert!(geometry.handle_left >= bounds.x);
        assert!(geometry.handle_right <= bounds.x + bounds.width);
    }
}

/// 拖动换算：指针在轨道左缘 → 0、右缘 → 1，中间线性。
#[test]
fn fraction_at_is_the_inverse_of_handle_center() {
    let bounds = track();
    for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let centre = TrackGeometry::of(bounds, fraction).handle_centre();
        let back = fraction_at(bounds, centre, 4.0);
        assert!((back - fraction).abs() < 1e-4, "{fraction} → {back}");
    }
    assert_eq!(fraction_at(bounds, bounds.x, 4.0), 0.0);
    assert_eq!(fraction_at(bounds, bounds.x + bounds.width, 4.0), 1.0);
}

/// 没有回调 = 禁用：不吃拖动，也不响应状态机。
#[test]
fn slider_without_a_callback_is_disabled() {
    let ui = ui();
    let slider: Slider<'_, u8> = Slider::new(&ui, 0.5);
    assert!(!slider.enabled());
    let enabled: Slider<'_, u8> = Slider::new(&ui, 0.5).on_change(|_| 0_u8);
    assert!(enabled.enabled());
    let forced: Slider<'_, u8> = Slider::new(&ui, 0.5)
        .on_change(|_| 0_u8)
        .state(Interaction::Disabled);
    assert!(!forced.enabled(), "强制禁用时不吃事件");
}
