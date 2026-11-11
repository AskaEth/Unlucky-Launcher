//! 帧泵的守卫（M192）。全部用**本地** `Pump` 摆时间轴：不起窗口、不等真实时间，所以不会假绿。
//!
//! 接线级的守卫（真消息 / 真订阅表）在 `app/subscription.rs` 的 `mod tests`：这一组只管算术。

use super::pump::{offset, spec, Pump, Slot, FRAME_MS, MAX_STEP_MS};
use super::*;

/// 标准档、没有钉相位的泵（出厂态是 `NONE`，见 [`Pump::new`]）。
fn standard() -> Pump {
    let mut pump = Pump::new();
    pump.configure(MotionScale::STANDARD, None);
    pump
}

/// 按「每帧 16 ms」推进到全部轨道结束（上限 20 s，防死循环）。
fn settle(pump: &mut Pump) -> u64 {
    let mut frames = 0;
    for _ in 0..1250 {
        if !pump.is_animating() {
            break;
        }
        if pump.advance(FRAME_MS) {
            frames += 1;
        }
    }
    frames
}

// ------------------------------------------------------------------ 令牌表

/// 规格**全部**来自 design 的令牌表，且落在 M3 的常规区间（改表会让这条红）。
#[test]
fn every_slot_spec_is_a_registered_token_pair() {
    for slot in Slot::ALL {
        let spec = spec(slot);
        let ms = spec.duration.ms();
        assert!(
            (250..=500).contains(&ms),
            "{}: 时长 {}ms 不在换页类动效的合理区间（克制优先）",
            slot.name(),
            ms
        );
        // 缓动必须是真的曲线（端点 = 0/1）
        assert!(spec.easing.sample(0.0).abs() < 1.0e-6);
        assert!((spec.easing.sample(1.0) - 1.0).abs() < 1.0e-6);
        // 位移必须来自间距栅格（4 的倍数），且不为零
        let (dx, dy) = offset(slot);
        assert!(
            dx > 0.0 || dy > 0.0,
            "{}: 位移为 0 就没有入场感",
            slot.name()
        );
        assert_eq!(dx % 4.0, 0.0);
        assert_eq!(dy % 4.0, 0.0);
        assert!(dx.max(dy) <= 24.0, "{}: 位移过大（克制优先）", slot.name());
    }
    // 表本身钉住：谁改了这三档，报告与出图都要跟着改
    assert_eq!(spec(Slot::Screen).duration.ms(), 300);
    assert_eq!(spec(Slot::Detail).duration.ms(), 350);
    assert_eq!(spec(Slot::Boot).duration.ms(), 400);
}

// ------------------------------------------------------------------ 有界泵

/// **空闲不烧帧**：动画跑完 ⇒ `is_animating()` 立刻为假，且不再推进。
///
/// 抓得住：把 `advance` 里的「到期即删除」删掉（轨道永远留着 ⇒ 帧订阅永不退订），或把
/// `is_animating` 改成恒真。两条都让本用例红。
#[test]
fn a_finished_animation_stops_asking_for_frames() {
    let mut pump = standard();
    assert!(pump.play(Slot::Screen), "起跑必须成功");
    assert!(pump.is_animating(), "起跑后必须有轨道（订阅帧的唯一理由）");

    settle(&mut pump);
    assert!(
        !pump.is_animating(),
        "动画结束必须**当场**退订（占着轨道 = 空闲烧帧）"
    );
    assert_eq!(pump.progress(Slot::Screen), 1.0, "结束即到位");
    assert_eq!(pump.offsets(), (0.0, 0.0), "结束即归位");

    // 再推进也不该有任何采样（订阅表里已经没有帧了）
    let before = pump.samples();
    for _ in 0..60 {
        assert!(!pump.advance(FRAME_MS), "没有轨道时推进必须返回 false");
    }
    assert_eq!(pump.samples(), before, "空闲态一次采样都不该发生");
}

/// **采样频率上限**：帧到得再快，时间轴也只按 16 ms 结算（≈62.5 次/秒）。
///
/// 注意这条**不管整窗重绘**：重绘跟随渲染器（见 [`FRAME_MS`] 文档）。
/// 抓得住：把 `pending_ms` 那道闸门删掉（每来一帧就推进 ⇒ 软件渲染下几百次/秒的重排）。
#[test]
fn frames_are_capped_at_sixty_per_second() {
    let mut pump = standard();
    pump.play(Slot::Screen);

    // 200 帧、每帧 1 ms（模拟 1000 fps 的渲染器）
    let mut advanced = 0;
    for _ in 0..200 {
        if pump.advance(1.0) {
            advanced += 1;
        }
    }
    assert!(
        advanced <= 13,
        "200 ms 内最多 13 次推进（200/16），实测 {advanced} 次"
    );
    assert!(
        advanced >= 11,
        "也别少得离谱（掉帧会看起来卡），实测 {advanced}"
    );
    assert_eq!(pump.samples(), advanced);
    // 时间轴按真实时间走，不受「帧被合并」影响
    assert!(
        (pump.now_ms() - 192.0).abs() <= 16.0,
        "时间轴应约 192 ms（12 × 16），实测 {}",
        pump.now_ms()
    );
}

/// **stale 帧基准不许把轨道一帧杀完**（M192 r1 复审 P1-1 的算术面）。
///
/// 运行时真出现过：空闲几分钟后第一条消息起跑轨道，第一帧的 dt 是整段空闲时长 ⇒ 立即到期 ⇒
/// 动画一帧走完，用户只看到一次偏移闪跳。两道兜底：起跑时重置帧基准（`motion::observe`）与
/// 单次最大步进 [`MAX_STEP_MS`]；这条守的是第二道。
#[test]
fn a_stale_baseline_cannot_kill_the_track_in_one_sample() {
    let mut pump = standard();
    pump.play(Slot::Screen);

    // 60 秒的「空闲间隔」当 dt
    assert!(pump.advance(60_000.0));
    assert!(
        pump.is_animating(),
        "一次超长 dt 不许把轨道当场删掉（那就等于没有动画）"
    );
    assert!(
        pump.now_ms() <= MAX_STEP_MS + 1.0e-6,
        "单次推进最多 {:?} ms，实测 {}",
        MAX_STEP_MS,
        pump.now_ms()
    );
    assert!(
        pump.progress(Slot::Screen) < 1.0,
        "进度不许一步到位（那就是闪跳）"
    );

    // 正常帧间隔照样把动画跑完（没被抖动卡死）
    settle(&mut pump);
    assert!(!pump.is_animating());
}

/// 采样必须单调、且末尾回到 1.0（回弹类曲线不在本批，见 [`spec`] 的表）。
#[test]
fn progress_is_monotone_and_lands_on_one() {
    let mut pump = standard();
    pump.play(Slot::Detail);
    let mut previous = 0.0;
    for _ in 0..40 {
        let value = pump.progress(Slot::Detail);
        assert!(value >= previous, "进度倒退了：{previous} → {value}");
        previous = value;
        if !pump.advance(FRAME_MS) {
            break;
        }
    }
    assert_eq!(previous, 1.0, "末帧必须到位");
}

// ------------------------------------------------------------------ NONE 降级

/// **`MotionScale::NONE` 是真归零**：不起轨道、不采样、位移恒为 0。
///
/// 抓得住：把 `play` 里的 `is_off()` 分支删掉（变成「跑一遍 0 时长动画」）——那样这个用例里
/// `is_animating()` 会短暂为真、`samples` 会增长，帧订阅次数就不是 0 了。
#[test]
fn none_scale_never_starts_a_track() {
    let mut pump = Pump::new(); // 出厂即 NONE
    assert!(pump.scale().is_off());

    // 首屏 + 换页 + 换详情：三次状态变化都不许留下轨道
    assert!(!pump.observe(Some(1), None), "关掉动效时不许起跑");
    assert_eq!(pump.offsets(), (0.0, 0.0), "关掉动效必须直接落终态");
    assert!(!pump.is_animating());
    assert!(!pump.observe(Some(2), None));
    assert_eq!(pump.offsets(), (0.0, 0.0));
    assert!(!pump.observe(Some(2), Some("instance-7")));
    assert_eq!(pump.offsets(), (0.0, 0.0));
    assert!(!pump.is_animating());

    for _ in 0..120 {
        assert!(!pump.advance(FRAME_MS), "关掉动效后没有任何可推进的");
    }
    assert_eq!(pump.samples(), 0, "NONE 档的帧订阅次数必须是 0");
}

/// 运行期把关掉动效（`configure(NONE)`）也要立刻落终态，而不是把在跑的动画「缩到 0 时长」。
#[test]
fn switching_to_none_mid_flight_settles_immediately() {
    let mut pump = standard();
    pump.play(Slot::Screen);
    pump.advance(FRAME_MS);
    assert!(pump.is_animating());

    pump.configure(MotionScale::NONE, None);
    assert!(!pump.is_animating(), "关掉动效必须当场清空轨道");
    assert_eq!(pump.progress(Slot::Screen), 1.0);
    assert_eq!(pump.offsets(), (0.0, 0.0));
}

/// 半速档（CPU 渲染兜底）真的把时长减半：同样的时间轴推进，轨道更早结束。
#[test]
fn half_scale_finishes_in_half_the_frames() {
    let mut full = standard();
    full.play(Slot::Screen);
    let full_frames = settle(&mut full);

    let mut half = Pump::new();
    half.configure(MotionScale::HALF, None);
    half.play(Slot::Screen);
    let half_frames = settle(&mut half);

    assert!(half_frames > 0 && full_frames > 0);
    assert!(
        half_frames * 2 <= full_frames + 2,
        "半速档应约用一半帧数：full={full_frames} half={half_frames}"
    );
}

// ------------------------------------------------------------------ 触发判据

/// 三个槽的触发优先级：首屏 Boot → 换屏 Screen → 同屏换详情 Detail。
#[test]
fn identity_changes_pick_the_right_slot() {
    let mut pump = standard();

    // 首帧：Boot（位移是纵向 16）
    assert!(pump.observe(Some(1), None), "首帧要跑 Boot 槽");
    let (dx, dy) = pump.offsets();
    assert_eq!(dx, 0.0);
    assert_eq!(dy, offset(Slot::Boot).1, "首屏应有一个向下的入场位移");
    assert_eq!(
        pump.progress(Slot::Screen),
        1.0,
        "首帧不该顺带起跑 Screen 槽"
    );
    settle(&mut pump);
    assert!(!pump.observe(Some(1), None), "标识没变 ⇒ 不起跑");
    assert_eq!(pump.offsets(), (0.0, 0.0));

    // 换屏：Screen（纵向）
    assert!(pump.observe(Some(2), None));
    let (dx, dy) = pump.offsets();
    assert_eq!(dx, 0.0);
    assert_eq!(dy, offset(Slot::Screen).1);
    assert!(pump.progress(Slot::Screen) < 1.0, "换屏要跑 Screen 槽");
    settle(&mut pump);

    // 同屏换详情：Detail（横向）
    assert!(pump.observe(Some(2), Some("instance-7")));
    let (dx, dy) = pump.offsets();
    assert_eq!(dx, offset(Slot::Detail).0, "列表↔详情是横向入场");
    assert_eq!(dy, 0.0);
    assert!(pump.progress(Slot::Detail) < 1.0);
    settle(&mut pump);
    assert!(!pump.observe(Some(2), Some("instance-7")));
    assert_eq!(pump.offsets(), (0.0, 0.0));
}

/// 不归 pages 画的屏（账户页 / AI 独立页，`screen == None`）**不起跑**；回到某一屏照样跑。
#[test]
fn screens_rendered_elsewhere_never_start_a_track() {
    let mut pump = standard();
    assert!(pump.observe(Some(1), None), "首屏开机跑 Boot");
    settle(&mut pump);

    assert!(!pump.observe(None, None), "没有 pages 树可动 ⇒ 不许白开帧");
    assert!(!pump.is_animating());
    assert_eq!(pump.offsets(), (0.0, 0.0));

    assert!(
        pump.observe(Some(1), None),
        "从账户页回到某一屏仍然是换页，要跑"
    );
    assert_eq!(pump.offsets().1, offset(Slot::Screen).1);
}

/// 换页会**取代**首屏那一档：位移不叠加（12px，不是 16+12）。
#[test]
fn a_new_transition_supersedes_the_previous_one() {
    let mut pump = standard();
    assert_eq!(
        pump.offsets().1,
        0.0,
        "还没 observe 时没有位移（采样是纯函数）"
    );
    pump.observe(Some(1), None);
    assert_eq!(pump.offsets().1, offset(Slot::Boot).1);

    // 首屏还没跑完就换页：位移必须是 Screen 那一档，而不是两档相加
    pump.observe(Some(2), None);
    assert_eq!(
        pump.offsets().1,
        offset(Slot::Screen).1,
        "新的一次内容变化应取代上一次（叠加会让位移变成两档之和）"
    );
}

/// 同一屏同一详情重复绘制**永远不会**重新起跑（否则每帧都在重启动画 = 永远停不下来）。
#[test]
fn repeated_paints_never_restart_the_animation() {
    let mut pump = standard();
    pump.observe(Some(3), Some("instance-9"));
    let frames = settle(&mut pump);
    assert!(frames > 0);

    for _ in 0..30 {
        assert!(!pump.observe(Some(3), Some("instance-9")));
        assert_eq!(pump.offsets(), (0.0, 0.0));
    }
    assert!(!pump.is_animating(), "重复绘制不该起新轨道");
}

// ------------------------------------------------------------------ 取证相位

/// `ULL_MOTION_AT` 的钉住相位：时间轴不前进，进度恒定（多帧序列图靠它可复现）。
#[test]
fn pinned_phase_freezes_the_clock() {
    let mut pump = Pump::new();
    pump.configure(MotionScale::STANDARD, Some(150.0));
    pump.play(Slot::Screen); // 300 ms 档 ⇒ 钉在 150 = 一半

    for _ in 0..10 {
        pump.advance(FRAME_MS);
    }
    let progress = pump.progress(Slot::Screen);
    assert!(
        (progress - spec(Slot::Screen).easing.sample(0.5)).abs() < 1.0e-6,
        "钉在 150/300 的进度应等于缓动曲线的中点，实测 {progress}"
    );
    assert!(pump.is_animating(), "钉住的相位不会自然结束（取证专用）");
}
