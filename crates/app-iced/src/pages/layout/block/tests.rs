//! `block` 的断言：三个纯量的数值口径。
//!
//! 「块摆在哪」量不到（iced 无 Element 内省），所以这里只钉**数**；
//! 「盒子是不是照这个数摆的」由 `pages/tests.rs` 的源码结构断言负责
//! （`frame_body_is_built_from_the_block_quantities`）。

use super::*;
use crate::pages::layout::{frame_metrics, measure_cap_px, rail_width, PaneKit};

/// 贴左：**任何**版面、任何窗宽，块偏移都是 0。
///
/// 抓得住：有人把偏移改成居中/右对齐（那它就不再是 0）；
/// 抓不住：`frame::body` 有没有真的用这个数（源码结构断言负责）。
#[test]
fn the_block_is_always_left_aligned() {
    for kit in [PaneKit::Single, PaneKit::Supporting, PaneKit::ListDetail] {
        for width in [320.0, 600.0, 720.0, 840.0, 900.0, 1180.0, 1440.0, 2560.0] {
            assert_eq!(
                body_offset(frame_metrics(kit, width)),
                0.0,
                "{kit:?} @{width} 的块偏移必须贴左"
            );
        }
    }
}

/// 块宽 = pane 宽（单列 = 可用宽度，两列 = 主列 pane），未到上限时不缩水。
#[test]
fn body_width_is_the_pane_until_the_cap() {
    for width in [320.0, 600.0, 840.0, 1180.0] {
        let single = frame_metrics(PaneKit::Single, width);
        assert_eq!(body_width(single), single.available, "单列 @{width}");
        assert_eq!(body_offset(single), 0.0, "@{width}");
    }

    // 两列版面：块宽 = 主列 pane（低于门槛时 list-detail 会退化成单列，那时块宽回到可用宽度）
    let two = frame_metrics(PaneKit::ListDetail, 1180.0);
    assert_eq!(body_width(two), two.main_pane);
    assert!(two.main_pane < two.available);
    let degraded = frame_metrics(PaneKit::ListDetail, 700.0);
    assert_eq!(body_width(degraded), degraded.available);

    // 1440 上限只拦极宽窗：1180 可用 1044、2560 可用 2424
    assert_eq!(frame_metrics(PaneKit::Single, 1180.0).available, 1044.0);
    assert_eq!(frame_metrics(PaneKit::Single, 2560.0).available, 2424.0);
    assert_eq!(
        body_width(frame_metrics(PaneKit::Single, 2560.0)),
        BODY_MAX_WIDTH
    );
    // 两列版面同样封顶（2560 下主列 pane 是 2064）
    let wide_two = frame_metrics(PaneKit::ListDetail, 2560.0);
    assert!(wide_two.main_pane > BODY_MAX_WIDTH);
    assert_eq!(body_width(wide_two), BODY_MAX_WIDTH);

    // 封顶之后左缘不变、右侧留白（不是居中）
    let huge = frame_metrics(PaneKit::Single, 2560.0);
    let left = rail_width() + huge.gutter;
    assert_eq!(left, 112.0);
    let right_gap = 2560.0 - (left + body_width(huge)) - huge.gutter;
    assert_eq!(right_gap, 984.0, "2560 宽下封顶后右侧留白");
    assert!(right_gap > 900.0);
}

/// 散文列宽 = `FrameMetrics::column`，永远 ≤ 行宽上限（与块宽脱钩）。
#[test]
fn prose_width_stays_at_the_measure_cap() {
    for width in [600.0, 840.0, 900.0, 1180.0, 2560.0] {
        let metrics = frame_metrics(PaneKit::Single, width);
        assert_eq!(prose_width(metrics), metrics.column, "@{width}");
        assert!(prose_width(metrics) <= measure_cap_px());
    }

    // 块已经封顶的地方，散文列仍是 424 —— 两个量确实分开了
    let huge = frame_metrics(PaneKit::Single, 2560.0);
    assert_eq!(body_width(huge), BODY_MAX_WIDTH);
    assert_eq!(prose_width(huge), measure_cap_px());
    assert_eq!(prose_width(huge), 424.0);
}

/// 上限本身：在 4px 栅格上，且远宽于散文列（否则「把两个量拆开」这件事是白做的）。
#[test]
fn the_cap_is_on_the_grid_and_far_wider_than_the_measure() {
    assert_eq!(BODY_MAX_WIDTH % 4.0, 0.0, "块宽上限必须在 4px 栅格上");
    assert!(
        BODY_MAX_WIDTH >= measure_cap_px() * 3.0,
        "块宽上限 {BODY_MAX_WIDTH} 必须远宽于散文列 {}",
        measure_cap_px()
    );
}
