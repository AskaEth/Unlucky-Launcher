//! 「减少动效」减到多少、依据是什么（M396）。从 `tests/modal.rs` 拆出来守 400 行硬线。
//!
//! 这一组只判**量**：减半而不是归零、减完每条腿还在令牌阶梯上。策略文档在
//! `components::motion::reduce`，真机取证在 `DevDocs/artifacts/motionclose/`。

use super::super::pump::FRAME_MS;
use super::super::transition::{modal_spec, page_spec, Kind, PageFrame};
use super::super::*;
use super::settle;
use unlucky_design::motion::MotionScale;

/// **「减少动效」减到多少：帧数减半，而且不是 0**（M396 的量化判据）。
///
/// 同一段 `Boot` 换页（`medium2` 300 ms 进场 / `medium1` 250 ms 退场）：
/// 标准档 19 帧（16 ms/帧），减半档 10 帧。两条都是**真的在动**。
///
/// 抓得住：`Reduced` 退回 `NONE`（帧数变 0 ⇒ 第一条断言红），或减过头（把系数写成 0.25 ⇒ 帧数 4）。
/// 抓不住：10 帧在真机上顺不顺眼（那是观感，要录像，见 `DevDocs/artifacts/motionclose`）。
#[test]
fn reduce_halves_the_frames_instead_of_killing_them() {
    use crate::platform::motion::ReduceMotion;

    let run = |reduce: ReduceMotion| {
        let mut pump = Pump::new();
        pump.configure(effective_scale(MotionScale::STANDARD, reduce, false), None);
        assert!(pump.observe(Some(1), Some(0), None), "换页必须起跑一条轨道");
        let first = pump.page_frame();
        assert_ne!(first.enter, (0.0, 0.0), "起手那一帧必须真的偏了（还在动）");
        let frames = settle(&mut pump);
        assert_eq!(pump.page_frame(), PageFrame::STILL, "走完必须归零");
        frames
    };

    let full = run(ReduceMotion::AnimationsOn);
    let reduced = run(ReduceMotion::Reduced);
    assert_eq!(full, 19, "标准档 300 ms / {FRAME_MS} ms ≈ 19 帧");
    assert_eq!(reduced, 10, "减半档 150 ms / {FRAME_MS} ms ≈ 10 帧");
    assert!(reduced > 0, "**减少动效不是关掉动效**（0 帧就是全无）");
}

/// 减半后每条腿仍在令牌阶梯的下沿**之上**（最短 `short1` = 50 ms）——「快」但不至于读成硬跳。
///
/// 抓得住：谁把系数再往下压（0.25 ⇒ `short3` 只剩 37 ms，跌破 50 ms 的令牌下沿），或往表里加一条
/// 比 `short3` 更短的腿（那样「减半」就开始等于「看不见」了）。
/// 抓不住：75 ms 与 50 ms 在人眼里的差别（观感，不是断言）。
#[test]
fn reduced_legs_stay_above_the_shortest_token() {
    use super::super::transition::{progress_spec, rail_spec, row_spec, status_spec};
    use crate::app::components::motion::reduce::REDUCED_FACTOR;
    use unlucky_design::motion::DurationToken;

    let mut legs = vec![];
    for kind in [
        Kind::Boot,
        Kind::Peer { forward: true },
        Kind::Peer { forward: false },
        Kind::Push,
        Kind::Pop,
    ] {
        let spec = page_spec(kind);
        legs.push(spec.enter.duration);
        legs.push(spec.exit.duration);
    }
    let modal = modal_spec();
    legs.extend([modal.enter.duration, modal.exit.duration]);
    let rail = rail_spec();
    legs.extend([rail.enter.duration, rail.exit.duration]);
    let row = row_spec();
    legs.extend([row.enter.duration, row.exit.duration]);
    let progress = progress_spec();
    legs.extend([progress.advance.duration, progress.arrive.duration]);
    legs.push(status_spec().duration);

    let floor = DurationToken::Short1.ms() as f32;
    for leg in &legs {
        let reduced = leg.ms() as f32 * REDUCED_FACTOR;
        assert!(
            reduced >= floor,
            "{} 减半后只剩 {reduced} ms，跌破了令牌下沿 {floor} ms",
            leg.name()
        );
    }

    // 错位间隔（`row_spec().stagger` = `short1` 50 ms）不在这条判据里：它决定「相邻两行隔多久起跑」，
    // 不是一条腿，减半不会让动效变硬跳。但它**不许**被减到采样间隔以下（那等于错位消失）。
    let stagger = row.stagger.ms() as f32 * REDUCED_FACTOR;
    assert!(
        stagger >= FRAME_MS as f32,
        "错位间隔减半后只剩 {stagger} ms，低于采样间隔 {FRAME_MS} ms（错位会被取整抹平）"
    );
}
