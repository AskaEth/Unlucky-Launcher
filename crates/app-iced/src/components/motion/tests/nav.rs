//! 左轨（导航轨）宽度轨道（M300 `NAVCOLLAPSE`）的判据：与右栗**并列**、同一张表、端点精确。
//!
//! 判据口径同 M257（`14-motion-and-animation.md` §7-7）：**不靠像素比对**。像素只能证明「某一张图
//! 里宽度是多少」，证明不了「中间有十几个相位」与「结束时不多要一帧」。
//!
//! 全部用**本地** `Pump` 连拍 `advance(FRAME_MS)`（T37 的「从驱动源连拍」）：这里不经过
//! `App`、不等真实时钟，于是「状态已切换但没有事件」那段完全是自然覆盖的——两次 `advance`
//! 之间没有任何别的调用，时间轴照样往前走。

use super::{settle, standard};
use crate::app::components::motion::*;
use unlucky_design::motion::MotionScale;

/// 一路采样左轨进度（含终点），用完自己 settle。
fn nav_samples(pump: &mut Pump) -> Vec<f32> {
    let mut out = vec![pump.nav_progress()];
    let mut guard = 0;
    while pump.is_animating() {
        pump.advance(FRAME_MS);
        out.push(pump.nav_progress());
        guard += 1;
        assert!(guard < 200, "左轨轨道没在跑完，疑似终止条件失效");
    }
    out
}

/// **展开：帧数落在 12~20、端点精确、中间严格单调**（`medium1` 250ms / 16ms）。
///
/// 抓得住：①时长被改成 600ms（帧数窜到 38）或 0（硬跳）；②`expire` 漏了新轨道（`while` 跑满
/// 200 帧的兜底断言红）；③插值写成阶跃（单调性断言的 `>` 会变成 `==`）。
/// 抓不住：观感是否像 M3E——那要出图（`DevDocs/artifacts/m300/`）。
#[test]
fn expanding_the_nav_rail_runs_the_expected_frames_and_lands_on_one() {
    let mut pump = standard();
    pump.observe_nav(false);
    settle(&mut pump);
    assert_eq!(pump.nav_progress(), 0.0, "前提：先落到折叠档");

    assert!(pump.observe_nav(true), "目标变了必须起跑");
    let values = nav_samples(&mut pump);
    let frames = values.len() - 1;
    assert!(
        (12..=20).contains(&frames),
        "250ms / 16ms 应落在 12~20 帧，实测 {frames} 帧"
    );
    assert_eq!(values.last().copied(), Some(1.0), "结束必须**严格**是 1.0");
    for pair in values.windows(2) {
        assert!(pair[1] > pair[0], "展开必须严格单调：{:?}", pair);
    }
    assert!(!pump.is_animating(), "跑完必须清空轨道（否则空闲烧帧）");
    assert_eq!(pump.nav_progress(), 1.0, "没有轨道时是终值");
}

/// **折叠：反向同一条判据**（`short4` 200ms ⇒ 10~20 帧）。
#[test]
fn collapsing_the_nav_rail_is_monotone_downwards_and_stops_at_zero() {
    let mut pump = standard();
    assert!(pump.observe_nav(false), "出厂展开 ⇒ 折叠必须起跑");
    let values = nav_samples(&mut pump);
    let frames = values.len() - 1;
    assert!(
        (10..=20).contains(&frames),
        "200ms / 16ms 应落在 10~20 帧，实测 {frames} 帧"
    );
    assert_eq!(values.last().copied(), Some(0.0), "结束必须严格是 0.0");
    for pair in values.windows(2) {
        assert!(pair[1] < pair[0], "折叠必须严格单调下降");
    }
    assert!(!pump.is_animating());
}

/// **`MotionScale::NONE`：严格等于终值，且一帧都不要。**
///
/// 抓得住：把 `rail_track` 的 `is_off()` 分支删掉（插一条 0 时长轨道 ⇒ 进度仍是终值，但
/// `is_animating()` 会有一拍为真 ⇒ 「一帧都不要」红）。
/// 抓不住：中间帧该不该存在（NONE 下不许有，这是定义）。
#[test]
fn motion_scale_none_puts_the_nav_rail_strictly_at_the_end_value() {
    let mut pump = Pump::new();
    pump.configure(MotionScale::NONE, None);

    assert!(!pump.observe_nav(false), "NONE 下不许起跑轨道");
    assert_eq!(pump.nav_progress(), 0.0, "折叠档终值必须逐位是 0.0");
    assert!(!pump.is_animating(), "NONE 下一帧都不许要");

    assert!(!pump.observe_nav(true));
    assert_eq!(pump.nav_progress(), 1.0);
    assert!(!pump.is_animating());
}

/// **两条宽度轨互不干扰**（tower 否掉「借用右栗那条轨道」的那个方案，这条就是它的判据）。
///
/// 场景：右栗先起跑展开，**同一拍**再折叠左轨。两条轨道各自从自己的起点走，谁也不改谁的
/// 目标；`Pump::is_animating()` 在两条都跑完之前一直为真（帧订阅不会提前关）。
///
/// 抓得住：①把 `observe_nav` 接回 `rail`/`rail_open`（那会当场改掉右栗的目标 ⇒ 第二条断言红）；
/// ②`expire` 只清一条轨道（先跑完的那条被留下 ⇒ 最后一条「谁都不在动」红）；
/// ③`is_animating` 漏了新轨道（跑到一半就 false）。
/// 抓不住：产品里两个按钮同时被点到的时序（那归 app 侧的 `ToggleNav` 用例）。
#[test]
fn the_two_width_tracks_never_retarget_each_other() {
    let mut pump = standard();
    pump.observe_rail(true);
    pump.advance(FRAME_MS);

    assert!(pump.observe_nav(false), "左轨折叠必须自己起一条轨道");
    // 右栗的目标没被左轨改掉：它继续往 1.0 走，而不是跳到左轨的方向。
    let right = pump.rail_progress();
    pump.advance(FRAME_MS);
    assert!(
        pump.rail_progress() > right,
        "右栗的进度必须继续上涨（{} → {}）",
        right,
        pump.rail_progress()
    );
    assert!(pump.nav_progress() < 1.0, "左轨同时在往下走");

    // 两条轨道都跑完：一条也不许留下（否则空闲烧帧）。
    for _ in 0..200 {
        if !pump.is_animating() {
            break;
        }
        pump.advance(FRAME_MS);
    }
    assert!(!pump.is_animating(), "两条轨道都必须到期删除");
    assert_eq!(pump.rail_progress(), 1.0);
    assert_eq!(pump.nav_progress(), 0.0);
}

/// **中途反向不跳变**：折叠到一半改主意展开，下一帧从当前值继续（不是回 0/1）。
#[test]
fn reversing_the_nav_rail_mid_flight_continues_from_the_current_progress() {
    let mut pump = standard();
    pump.observe_nav(false);
    // **走到真正的中间**再反向：收起走的是 `EmphasizedAccelerate`（起步很慢），固定推 4 帧
    // 只到 6%（实测 0.941），那不是「半路改主意」而是「刚起跑」。
    let mut guard = 0;
    while !(0.2..=0.8).contains(&pump.nav_progress()) {
        pump.advance(FRAME_MS);
        guard += 1;
        assert!(guard < 40, "没走到中间：{}", pump.nav_progress());
    }
    let before = pump.nav_progress();

    pump.observe_nav(true);
    let after = pump.nav_progress();
    assert!(
        (after - before).abs() < 1e-6,
        "反向那一帧不许跳变：{before} → {after}"
    );
}

/// **启动对表**：上次折叠着退出（落盘 true）时，第一次**展开**仍然有动画。
///
/// 这条守的是 `snap_nav` 存在的理由：泵的目标出厂是「展开」，若不对表，`observe_nav(true)`
/// 会走「目标没变」的早退分支 ⇒ 展开变成一次硬跳（T37：数值对，但没人让它跑）。
///
/// 抓得住：删掉 `snap_nav` 的调用点（`app/update/motion_sync.rs` 那一句）后，本用例在泵层面
/// 仍绿 ⇒ 真正的接线判据在 app 侧（`update::nav::tests`），这里只钉住泵的语义。
#[test]
fn snapping_to_the_persisted_state_keeps_the_first_expand_animated() {
    let mut pump = standard();
    // 出厂：目标=展开、没有轨道。模拟「上次折叠着退出」。
    assert_eq!(pump.nav_progress(), 1.0);
    pump.snap_nav(false);
    assert_eq!(pump.nav_progress(), 0.0, "对表之后必须严格是折叠档");
    assert!(!pump.is_animating(), "对表不许插轨道");

    assert!(pump.observe_nav(true), "第一次展开必须真的起跑");
    pump.advance(FRAME_MS);
    assert!(
        pump.nav_progress() > 0.0 && pump.nav_progress() < 1.0,
        "起跑之后必须处在中间，而不是直接到位"
    );
    settle(&mut pump);
    assert_eq!(pump.nav_progress(), 1.0);
}
