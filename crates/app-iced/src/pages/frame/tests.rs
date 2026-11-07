//! `frame` 的断言：空态 / 卡片 / 页头规格与一屏字号档。
use super::pieces::{EMPTY_ICON_BLOCK, EMPTY_ICON_GLYPH};
use super::*;

use unlucky_design::color::SchemeFlavor;
use unlucky_design::shape::ShapeScale;
use unlucky_design::typography::Platform;

use crate::pages::layout::{frame_metrics, PaneKit};
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

#[test]
fn empty_state_follows_the_12b_reference() {
    // 15.1 图标块 96×96、圆角 Large(16)；15.2 标题 HeadlineSmall；15.3 副文案 BodySmall
    let spec = Component::EmptyState.spec();
    assert_eq!(EMPTY_ICON_BLOCK, 96.0);
    assert_eq!(spec.shape, ShapeScale::Large);
    assert_eq!(spec.text, TypeRole::HeadlineSmall);
    assert_eq!(ui().style(spec.text).size_px, 24.0);
    assert_eq!(spec.support, Some(TypeRole::BodySmall));
    assert_eq!(ui().style(TypeRole::BodySmall).size_px, 12.0);
    // 字形 32~48（[估]）
    assert!((32.0..=48.0).contains(&EMPTY_ICON_GLYPH));
    const { assert!(EMPTY_ICON_GLYPH < EMPTY_ICON_BLOCK) };
}

#[test]
fn card_uses_the_outlined_card_spec() {
    let spec = Component::OutlinedCard.spec();
    assert_eq!(spec.shape, ShapeScale::Medium);
    assert_eq!(ui().radius_dp(spec.shape).top_left, 12.0);
    assert_eq!(spec.text, TypeRole::TitleMedium);
    assert_eq!(spec.support, Some(TypeRole::BodySmall));
}

#[test]
fn page_header_uses_the_two_documented_heights() {
    let ui = ui();
    assert_eq!(ui.metrics().header_height(false), 56.0);
    assert_eq!(ui.metrics().header_height(true), 84.0);
}

/// 一屏字号 ≤4 档（rubric D4）。规格表来源：12b §16.4 / §15 / 3.x OutlinedCard。
#[test]
fn every_screen_keeps_at_most_four_text_sizes() {
    let ui = ui();

    let header = ui.emphasized(Component::TopAppBar.spec().text).size_px;
    let row_title = ui.style(Component::ListItem.spec().text).size_px;
    let row_support = ui
        .style(Component::ListItem.spec().support.unwrap())
        .size_px;
    let action = ui.style(Component::TextButton.spec().text).size_px;
    let body = ui.style(TypeRole::BodyMedium).size_px;
    let empty_title = ui.style(Component::EmptyState.spec().text).size_px;

    let screens: [(&str, &[f32]); 3] = [
        // 实例页（list-detail 与窄窗单列）：页头 + 行标题 + 行副文案 + 操作/正文
        ("instances", &[header, row_title, row_support, action, body]),
        // 设置页：页头 + 分组项/操作 + 行标签 + 行值 + 空态标题
        (
            "settings",
            &[header, action, row_support, body, empty_title],
        ),
        // 下载 / 日志：页头 + 空态标题 + 空态副文案 + 操作
        ("empty-pages", &[header, empty_title, row_support, action]),
    ];

    for (name, sizes) in screens {
        let mut unique = sizes.to_vec();
        unique.sort_by(f32::total_cmp);
        unique.dedup();
        assert!(
            unique.len() <= 4,
            "{name} 一屏用了 {} 档字号 {unique:?}，超过 rubric D4 的 4 档",
            unique.len()
        );
        assert!(unique[0] >= 12.0, "{name} 出现小于 12px 的字号");
        assert!(unique[unique.len() - 1] <= 24.0, "{name} 字号过大");
    }
}

/// 版面几何是页面唯一宽度来源：两列版面的正文列必须窄于可用宽度。
#[test]
fn two_pane_body_is_narrower_than_the_available_width() {
    let wide = frame_metrics(PaneKit::ListDetail, 1180.0);
    assert!(wide.column < wide.available);
    assert!(wide.secondary.is_some());
}
