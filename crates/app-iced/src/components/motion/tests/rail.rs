//! 右栗折叠轨道（M257 / A2）的判据：泵帧数 + 端点精确 + 中间态严格单调。
//!
//! 判据口径同 M256（`14-motion-and-animation.md` §7-7）：**不靠像素比对**。像素只能证明「某一张图
//! 里宽度是多少」，证明不了「中间有 12 个相位」与「结束时不多要一帧」。

use super::{settle, standard};
use crate::app::components::motion::*;
use unlucky_design::motion::MotionScale;

/// 一路采样折叠进度（含终点），用完自己 settle。
fn samples(pump: &mut Pump) -> Vec<f32> {
    let mut out = vec![pump.rail_progress()];
    let mut guard = 0;
    while pump.is_animating() {
        pump.advance(FRAME_MS);
        out.push(pump.rail_progress());
        guard += 1;
        assert!(guard < 200, "折叠轨道没在跑完，疑似终止条件失效");
    }
    out
}

/// **展开：帧数落在 12~20 之间、端点精确、中间严格单调**。
///
/// 抓得住：①有人把 `medium1`(250) 改成 600ms（帧数窜到 38）或改成 0（硬跳，帧数 0）；
/// ②终止条件被删掉（`while is_animating` 直接跑满 200 帧的兜底断言红）；③插值写成阶跃
/// （单调性断言里的 `>` 会变成 `==`）。
/// 抓不住：观感是否「像 M3E」——那要出图（D30），本用例只保证相位数量与端点。
#[test]
fn expanding_runs_the_expected_number_of_frames_and_lands_exactly_on_one() {
    let mut pump = standard();
    assert!(pump.observe_rail(true), "目标变了必须起跑");
    assert_eq!(pump.rail_progress(), 0.0, "第一帧还在折叠档（端点精确）");

    let values = samples(&mut pump);
    let frames = values.len() - 1;
    assert!(
        (12..=20).contains(&frames),
        "250ms / 16ms 应落在 12~20 帧，实测 {frames} 帧"
    );
    assert_eq!(
        values.last().copied(),
        Some(1.0),
        "结束时必须**严格**是 1.0（不是 0.99x）"
    );
    for pair in values.windows(2) {
        assert!(
            pair[1] > pair[0],
            "展开必须严格单调：{:?} → {:?}",
            pair[0],
            pair[1]
        );
    }
    assert!(!pump.is_animating(), "跑完必须清空轨道（否则空闲烧帧）");
    assert_eq!(pump.rail_progress(), 1.0, "没有轨道时是终值");
}

/// **收起：同一条判据，反向**（`short4` 200ms ⇒ 12~13 帧）。
#[test]
fn collapsing_is_strictly_monotone_downwards_and_stops_at_zero() {
    let mut pump = standard();
    pump.observe_rail(true);
    settle(&mut pump);
    assert_eq!(pump.rail_progress(), 1.0);

    assert!(pump.observe_rail(false));
    let values = samples(&mut pump);
    let frames = values.len() - 1;
    assert!(
        (10..=20).contains(&frames),
        "200ms / 16ms 应落在 10~20 帧，实测 {frames} 帧"
    );
    assert_eq!(values.last().copied(), Some(0.0), "结束时必须严格是 0.0");
    for pair in values.windows(2) {
        assert!(pair[1] < pair[0], "收起必须严格单调下降");
    }
    assert!(!pump.is_animating());
}

/// **`MotionScale::NONE`：严格等于终值，且一帧都不要**（独立的用例，不靠别的用例顺带覆盖）。
///
/// 抓得住：把 `observe_rail` 里的 `is_off()` 分支删掉（那会插一条 0 时长轨道 ⇒ 进度仍是 1.0，
/// 但 `is_animating()` 会有一拍为真 ⇒ 本用例的「一帧都不要」断言红）；或把 `is_off()`
/// 写成「跑一遍 0 时长」（同样红）。
/// 抓不住：中间帧**该不该**存在（NONE 下不该有中间帧，这是定义，不是观感）。
#[test]
fn motion_scale_none_puts_the_rail_strictly_at_the_end_value() {
    let mut pump = Pump::new();
    pump.configure(MotionScale::NONE, None);

    assert!(
        !pump.observe_rail(true),
        "NONE 下不许起跑轨道（不是什么都没发生，是**不许有中间态**）"
    );
    assert_eq!(pump.rail_progress(), 1.0, "展开档的终值必须逐位是 1.0");
    assert!(!pump.is_animating(), "NONE 下一帧都不许要");

    assert!(!pump.observe_rail(false));
    assert_eq!(pump.rail_progress(), 0.0, "折叠档的终值必须逐位是 0.0");
    assert!(!pump.is_animating());
}

/// **中途反向不跳变**：折叠到一半改主意展开，下一帧的进度必须从当前值继续（不是回 0/1）。
///
/// 抓得住：把 `observe_rail` 里的 `from` 写成常量 0 / 1（那会让宽度在半路跳一下）。
/// 抓不住：真实手感（那个「拐点」顺不顺）——只保证数值连续。
#[test]
fn reversing_mid_flight_continues_from_the_current_progress() {
    let mut pump = standard();
    pump.observe_rail(true);
    for _ in 0..4 {
        pump.advance(FRAME_MS);
    }
    let before = pump.rail_progress();
    assert!(
        before > 0.1 && before < 0.9,
        "前提：应该处在中间（实测 {before}）"
    );

    pump.observe_rail(false);
    let after = pump.rail_progress();
    assert!(
        (after - before).abs() < 1e-6,
        "反向那一帧不许跳变：{before} → {after}"
    );
}

/// **拆出档不走过渡**：`snap_rail` 直接落终值，且把在跑的轨道掐掉。
///
/// 抓得住：有人把 `Detached` 也接进这条轨道（那会做一次「48 → 0」的缩水动画）。
/// 抓不住：拆分本身的观感（那是布局切换，不在动效范围里）。
#[test]
fn detaching_snaps_instead_of_animating() {
    let mut pump = standard();
    pump.observe_rail(true);
    pump.advance(FRAME_MS);
    assert!(pump.is_animating());

    assert!(pump.snap_rail(false), "有轨道在跑时落档要报「掐掉了」");
    assert_eq!(pump.rail_progress(), 0.0);
    assert!(!pump.is_animating());
}

/// **帧订阅的委托**：状态层过渡在跑时 `motion::needs_frames()` 必须为真。
///
/// 这是 `app/subscription.rs` 唯一的判断点（`motion::needs_frames()`）：A8 的时钟若没被委托进
/// 这里，产品里就一帧都不会来（状态层卡在起始值），而单元测试照样能靠 `advance_by` 摆出绿。
///
/// 抓得住：删掉 `motion::needs_frames` 里对 `interaction::layer::is_animating()` 的那一句。
/// 抓不住：真实窗口的帧率（那由 `window::frames()` 决定，见 `app/subscription.rs` 的表）。
#[test]
fn the_state_layer_clock_feeds_the_single_subscription_point() {
    use crate::app::components::interaction::layer;

    reset();
    assert!(!needs_frames(), "前提：静息时不要帧");

    let mut state = layer::Layer::new();
    state.retarget(
        crate::app::components::interaction::Interaction::Hovered,
        MotionScale::STANDARD,
    );
    assert!(
        needs_frames(),
        "悬停起跑之后**同一拍**就要帧（否则 draw 侧起跑的 0 进度会一直画不出来）"
    );

    // 推进到 50ms 之后：必须自己退订（不是「一直要帧」）。
    for _ in 0..6 {
        layer::advance_by(FRAME_MS);
    }
    assert!(!needs_frames(), "过渡走完必须立刻不要帧");
    layer::reset();
}
