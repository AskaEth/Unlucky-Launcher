//! 有界泵的算术守卫（M256 从 `tests/mod.rs` 拆出，守 400 行硬线）：采样频率上限、到期即退订、
//! stale 帧基准不许一帧杀完、以及 `MotionScale::HALF` 的**语义**（时长减半 ≠ 半速）。
//!
//! 这些全是「本地 `Pump` + 摆时间轴」就能判死的算术性质，所以不需要窗口、不需要真实时间。

use super::super::pump::{Pump, FRAME_MS, MAX_STEP_MS};
use super::super::transition::PageFrame;
use super::{settle, standard};
use unlucky_design::motion::{DurationToken, MotionScale};

// ------------------------------------------------------------------ 有界泵

/// **空闲不烧帧**：动画跑完 ⇒ `is_animating()` 立刻为假，且不再推进。
///
/// 抓得住：把「到期即删除」删掉（轨道永远留着 ⇒ 帧订阅永不退订）。
/// 抓不住：整窗重绘次数（那跟随渲染器，见 `FRAME_MS` 的文档）。
#[test]
fn a_finished_animation_stops_asking_for_frames() {
    let mut pump = standard();
    assert!(pump.observe(Some(1), Some(0), None));
    assert!(pump.is_animating(), "起跑后必须有轨道（订阅帧的唯一理由）");

    settle(&mut pump);
    assert!(!pump.is_animating(), "动画结束必须**当场**退订");
    assert_eq!(pump.page_frame(), PageFrame::STILL);

    let before = pump.samples();
    for _ in 0..60 {
        assert!(!pump.advance(FRAME_MS), "没有轨道时推进必须返回 false");
    }
    assert_eq!(pump.samples(), before, "空闲态一次采样都不该发生");
}

/// **采样频率上限**：帧到得再快，时间轴也只按 16 ms 结算（≈62.5 次/秒）。
///
/// 抓得住：把 `pending_ms` 那道闸门删掉（每来一帧就推进 ⇒ 软件渲染下几百次/秒的重排）。
/// 抓不住：**整窗重绘**频率——那跟随渲染器，与这条闸门无关。
#[test]
fn frames_are_capped_at_sixty_per_second() {
    let mut pump = standard();
    pump.observe(Some(1), Some(0), None);

    let mut advanced = 0;
    for _ in 0..200 {
        if pump.advance(1.0) {
            advanced += 1;
        }
    }
    assert!(
        (11..=13).contains(&advanced),
        "200 ms 内应 12±1 次推进，实测 {advanced}"
    );
    assert_eq!(pump.samples(), advanced);
    assert!(
        (pump.now_ms() - 192.0).abs() <= 16.0,
        "时间轴应约 192 ms，实测 {}",
        pump.now_ms()
    );
}

/// **stale 帧基准不许把轨道一帧杀完**（M192 r1 复审 P1-1 的算术面）。
///
/// 抓得住：把 `MAX_STEP_MS` 的夹取删掉（一次 60 s 的 dt 会把轨道当场杀掉 ⇒ 动画一帧走完）。
/// 抓不住：真机上「空闲醒来第一帧的 dt 到底多长」（那要真窗口，见报告的未验清单）。
#[test]
fn a_stale_baseline_cannot_kill_the_track_in_one_sample() {
    let mut pump = standard();
    pump.observe(Some(1), Some(0), None);

    assert!(pump.advance(60_000.0)); // 60 秒的"空闲间隔"当 dt
    assert!(pump.is_animating(), "一次超长 dt 不许把轨道当场删掉");
    assert!(pump.now_ms() <= MAX_STEP_MS + 1.0e-6);
    assert!(pump.page_frame().enter.1 > 0.0, "进度不许一步到位");

    settle(&mut pump);
    assert!(!pump.is_animating());
}

/// 半时长档（CPU 渲染兜底）真的把时长减半：同样的推进，轨道更早结束。
///
/// 顺带钉住 `MotionScale::HALF` 的**语义**（M256 核清项②）：是「时长 ×0.5」＝更快，
/// 不是"半速"。抓得住：把 `HALF.factor` 改成 2.0（半速）或 1.0（无效）。
/// 抓不住：**观感上"快一倍"是否合适**（那是 D30 的截图/录屏判断，不是断言）。
#[test]
fn half_scale_halves_the_duration_not_the_speed() {
    assert_eq!(MotionScale::HALF.factor(), 0.5);
    assert_eq!(MotionScale::HALF.ms(DurationToken::Medium2), 150.0);

    let mut full = standard();
    full.observe(Some(1), Some(0), None);
    let full_frames = settle(&mut full);

    let mut half = Pump::new();
    half.configure(MotionScale::HALF, None);
    half.observe(Some(1), Some(0), None);
    let half_frames = settle(&mut half);

    assert!(half_frames > 0 && full_frames > 0);
    assert!(
        half_frames * 2 <= full_frames + 2,
        "HALF 应约用一半帧数：full={full_frames} half={half_frames}"
    );
}
