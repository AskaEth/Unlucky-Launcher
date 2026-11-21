//! 弹簧腿的判据（M432）：端点、帧数、单调性、回弹段、`MotionScale::HALF` 的语义。
//!
//! 口径同 §7-7：**泵帧数 + 端点 + 单调性**，不是像素比对。另加两条只有弹簧才有的：
//! ①回弹段必须**量化**（峰值与稳定时间写在同一行日志里，不许只说"有一点点弹"）；
//! ②`MotionScale::HALF` 对弹簧是「压缩时间轴」（等价于 `k/factor²`、ζ 不变），不是"半速"。
//!
//! 这里只接**一条**消费点：页面切换的进场腿（`Pump::page_frame` 的 `enter`）。退场腿仍是令牌时长。

use super::super::pump::{Pump, SpringLeg, FRAME_MS};
use super::super::transition::{page_spec, Kind, PageFrame};
use super::{settle, standard};
use unlucky_design::motion::{
    MotionScale, SPRING_EXPRESSIVE_SPATIAL_DEFAULT, SPRING_EXPRESSIVE_SPATIAL_SLOW,
};

/// 一路采样到轨道结束（含起点与终点）。
fn frames(pump: &mut Pump) -> Vec<PageFrame> {
    let mut out = vec![pump.page_frame()];
    let mut guard = 0;
    while pump.is_animating() {
        pump.advance(FRAME_MS);
        out.push(pump.page_frame());
        guard += 1;
        assert!(guard < 200, "轨道没跑完（终止条件疑似失效）");
    }
    out
}

/// 把泵推到「同级向右换页」的起点。
fn arm_peer(pump: &mut Pump) {
    pump.observe(Some(1), Some(0), None);
    settle(pump);
    assert_eq!(pump.page_frame(), PageFrame::STILL);
}

/// **端点**：`t <= 0` 严格 0、`t >= 稳定时间` 严格 1；且稳定时间落在 M3E 短档（150~300ms）。
///
/// 抓得住：把 `progress` 的 `elapsed >= total_ms ⇒ 1.0` 那一句删掉（到点只到 0.99 或 1.01，
/// 端点断言立刻红）；也抓得住「直接拿 `spring_ms`（阈值 0.001）当总时长」——那样 12px 是 476ms，
/// 300ms 的硬线断言红。
/// 抓不住：观感好不好（那是截图的事）。
#[test]
fn the_spring_leg_lands_on_both_endpoints_inside_the_short_band() {
    let near = SpringLeg::new(
        SPRING_EXPRESSIVE_SPATIAL_DEFAULT,
        12.0,
        MotionScale::STANDARD,
    );
    let far = SpringLeg::new(
        SPRING_EXPRESSIVE_SPATIAL_DEFAULT,
        16.0,
        MotionScale::STANDARD,
    );
    println!(
        "expressive 空间默认：12px 稳定 {:.1}ms / 16px 稳定 {:.1}ms",
        near.total_ms(),
        far.total_ms()
    );

    for (name, leg) in [("12px", near), ("16px", far)] {
        assert_eq!(leg.progress(0.0), 0.0, "{name}: t=0 必须是 0");
        assert_eq!(leg.progress(-8.0), 0.0, "{name}: t<0 必须是 0");
        assert_eq!(
            leg.progress(leg.total_ms()),
            1.0,
            "{name}: t=settle 必须**严格**是 1.0"
        );
        assert!(
            (leg.progress(leg.total_ms() * 4.0) - 1.0).abs() < 1.0e-3,
            "{name}: 过冲点之后不许再动"
        );
        assert!(
            (150.0..=300.0).contains(&leg.total_ms()),
            "{name}: 稳定时间 {:.1}ms 超出 M3E 短档（150~300）",
            leg.total_ms()
        );
    }
}

/// **帧数相近**：同一段操作，旧路径 20 帧、弹簧腿 17 帧（同一档 12~20 的窗口内）。
///
/// 抓得住：把弹簧腿接到 `spring_ms`（0.001 口径）上——11px 之外会跑出 20 帧的窗口；
/// 或者把 `page_lifetime_ms` 改回只看令牌（弹簧比令牌短时看不出来，所以另有一条 slow 的用例）。
/// 抓不住：两帧之间真实插值出来的中间像素（不采样亚帧）。
#[test]
fn the_spring_leg_keeps_the_frame_count_and_never_moves_backwards() {
    let mut token = standard();
    arm_peer(&mut token);
    assert!(token.observe(Some(3), Some(1), None));

    let mut spring = standard();
    arm_peer(&mut spring);
    spring.use_page_spring(Some(SPRING_EXPRESSIVE_SPATIAL_DEFAULT));
    assert!(spring.observe(Some(3), Some(1), None));

    let old = frames(&mut token);
    let new = frames(&mut spring);
    println!(
        "同一段同级换页：令牌腿 {} 帧（{:.0}ms）/ 弹簧腿 {} 帧",
        old.len(),
        old.len().saturating_sub(1) as f64 * FRAME_MS,
        new.len()
    );

    // 端点：首帧满幅、末帧归位
    assert_eq!(new[0].enter, (12.0, 0.0), "首帧必须满幅（进度 0）");
    assert_eq!(
        *new.last().expect("至少一帧"),
        PageFrame::STILL,
        "末帧必须归位"
    );
    // 帧数：仍在 M3E 窗口内，且与旧路径相近（差不超过 3 帧）
    assert!(
        (12..=20).contains(&new.len()),
        "弹簧腿帧数 {} 出窗",
        new.len()
    );
    assert!(
        new.len().abs_diff(old.len()) <= 3,
        "与旧路径帧数相差太大：{} vs {}",
        new.len(),
        old.len()
    );
    // 单调：位移逐帧不增，且永不为负（回弹被取整吸收，见下一条用例）
    for pair in new.windows(2) {
        assert!(
            pair[1].enter.0 <= pair[0].enter.0,
            "位移不许回升：{:?} → {:?}",
            pair[0].enter,
            pair[1].enter
        );
        assert!(pair[1].enter.0 >= 0.0, "位移不许反向：{:?}", pair[1].enter);
    }
    // 退场腿不受影响：在**两条轨道都还活着**的那些帧里逐帧相同。
    // （弹簧腿先到期——它的寿命是 `max(236.5, 250) = 250ms`，令牌腿是 300ms——最后一帧是轨道
    //  删掉之后的静息，没有可比帧。）
    for (index, (old_frame, new_frame)) in old.iter().zip(new.iter()).enumerate() {
        if index + 1 == new.len() {
            break;
        }
        assert_eq!(old_frame.exit, new_frame.exit, "第 {index} 帧：退场腿改了");
        assert_eq!(
            old_frame.exiting, new_frame.exiting,
            "第 {index} 帧：退场判定改了"
        );
    }
}

/// **回弹段要量化**：欠阻尼弹簧的进度会短暂越过 1.0，但在 12px 上被 `enter_offset` 的 `clamp` +
/// `round` 吸收（过冲 < 0.5px ⇒ 画出来一直是 0）。
///
/// 抓得住：把 `enter_offset` 里的 `clamp(0,1)` 去掉（位移会变成负的 −1px 抖动，上一条用例的
/// 「不许反向」会红）；或把 ζ 调到 0.3（峰值 0.37 超过 1.15 ⇒ 这里红）。
/// 抓不住：真实屏幕上一帧一像素的抖动（那是渲染时序，不在泵里）。
#[test]
fn the_rebound_segment_is_measured_even_though_rounding_hides_it() {
    let leg = SpringLeg::new(
        SPRING_EXPRESSIVE_SPATIAL_DEFAULT,
        12.0,
        MotionScale::STANDARD,
    );
    let mut peak: f32 = 0.0;
    let mut rebound_from: Option<u32> = None;
    for ms in 1..=(leg.total_ms() as u32) {
        let progress = leg.progress(f64::from(ms));
        if progress > 1.0 && rebound_from.is_none() {
            rebound_from = Some(ms);
        }
        peak = peak.max(progress);
    }
    let from = rebound_from.expect("expressive 空间默认是欠阻尼（ζ=0.8），必然有过冲段");
    println!(
        "回弹段 {from}~{:.0}ms，进度峰值 {peak:.4}（超出 {:.2}%），稳定时间 {:.1}ms",
        leg.total_ms(),
        (peak - 1.0) * 100.0,
        leg.total_ms()
    );
    assert!(peak > 1.0, "欠阻尼弹簧必然有回弹段");
    assert!(peak < 1.02, "回弹峰值 {peak} 太大（ζ 被改过？）");
    // 位移层面看不见：过冲幅度不到半个像素 ⇒ 取整后与终点逐位相同
    assert!(
        (peak - 1.0) * 12.0 < 0.5,
        "过冲 {:.3}px 超过半个像素，取整吸收不掉",
        (peak - 1.0) * 12.0
    );
}

/// **`page_lifetime_ms` 必须问腿自己**：比令牌更慢的弹簧不许被令牌时长掐掉。
///
/// 用 `SPRING_EXPRESSIVE_SPATIAL_SLOW`（12px ⇒ 327ms）：它**故意**超出 300ms 短档，产品路径**不接**
/// 这一档——这条用例存在的理由只是把「到期判据」的错法钉住。
///
/// 抓得住：把 `page_lifetime_ms` 改回 `enter.duration.max(exit.duration)`（= 300ms）——
/// 轨道会在 300ms 被删掉，帧数从 22 掉到 20，本条红。
/// 抓不住：慢弹簧的观感（超档的取舍是产品决定，不是算术）。
#[test]
fn a_slow_spring_is_not_cut_off_by_the_token_duration() {
    let leg = SpringLeg::new(SPRING_EXPRESSIVE_SPATIAL_SLOW, 12.0, MotionScale::STANDARD);
    assert!(
        leg.total_ms() > 300.0,
        "这一档是为了越线才选的（实测 {:.1}ms）",
        leg.total_ms()
    );

    let mut pump = standard();
    arm_peer(&mut pump);
    pump.use_page_spring(Some(SPRING_EXPRESSIVE_SPATIAL_SLOW));
    assert!(pump.observe(Some(3), Some(1), None));
    let trace = frames(&mut pump);
    println!(
        "slow 弹簧：稳定 {:.0}ms ⇒ {} 帧（令牌腿是 20 帧）",
        leg.total_ms(),
        trace.len()
    );
    assert!(
        trace.len() > 20,
        "轨道被令牌时长（300ms）掐掉了：只有 {} 帧",
        trace.len()
    );
    assert_eq!(*trace.last().expect("至少一帧"), PageFrame::STILL);
}

/// **默认关闭**：不调 `use_page_spring` 时逐帧输出与「碰过 API 又关回去」完全一样。
///
/// 抓得住：把弹簧接成默认（`page_spring` 出厂 `Some`）——两条轨迹的帧数立刻不同。
/// 抓不住：谁能调这个开关（接线点不在本 crate 的 motion 模块里，见报告）。
#[test]
fn the_token_leg_is_untouched_until_a_spring_is_selected() {
    let mut never = standard();
    arm_peer(&mut never);
    never.observe(Some(3), Some(1), None);
    let untouched = frames(&mut never);

    let mut round_trip = standard();
    arm_peer(&mut round_trip);
    round_trip.use_page_spring(Some(SPRING_EXPRESSIVE_SPATIAL_DEFAULT));
    round_trip.use_page_spring(None);
    round_trip.observe(Some(3), Some(1), None);
    let restored = frames(&mut round_trip);

    assert_eq!(untouched, restored, "关回去之后必须逐帧回到旧路径");
    assert!(
        (12..=20).contains(&untouched.len()),
        "旧路径帧数 {} 出窗",
        untouched.len()
    );
}

/// **`MotionScale::NONE` 的硬契约对弹簧同样成立**：不插轨道、位移严格 0、一帧都不要。
///
/// 抓得住：让 `play_page` 在 `is_off()` 时仍然插一条「0 时长的弹簧腿」（`is_animating()` 会有一拍为真）。
/// 抓不住：真机上系统"减少动效"开关有没有读到（那是 `platform::motion` 的事）。
#[test]
fn a_spring_never_breaks_the_none_scale_contract() {
    let mut pump = Pump::new(); // 出厂 NONE
    pump.use_page_spring(Some(SPRING_EXPRESSIVE_SPATIAL_DEFAULT));
    assert!(!pump.observe(Some(1), Some(0), None), "NONE 档不许起跑");
    assert_eq!(pump.page_frame(), PageFrame::STILL);
    assert!(!pump.is_animating());
    for _ in 0..30 {
        assert!(!pump.advance(FRAME_MS));
    }
    assert_eq!(pump.samples(), 0);

    // 直接问腿：NONE 档的 factor 是 0 ⇒ 除 t=0 外一律终态（与 `is_off` 的硬契约一致）
    let leg = SpringLeg::new(SPRING_EXPRESSIVE_SPATIAL_DEFAULT, 12.0, MotionScale::NONE);
    assert_eq!(leg.total_ms(), 0.0);
    assert_eq!(leg.progress(0.0), 0.0);
    assert_eq!(leg.progress(1.0), 1.0);
}

/// **`MotionScale::HALF` 对弹簧 = 压缩时间轴**（等价于 `k/factor²`，ζ 不变）⇒ 峰值不变、稳定减半。
///
/// 抓得住：把 `HALF` 实现成"振幅减半"或"半速"（`half.total_ms()` 会等于或翻倍于 `full`，本条红）。
/// 抓不住：`HALF` 该不该这样用于弹簧——那是策略决定（本 mission 的选择写在报告里）。
#[test]
fn half_scale_compresses_the_spring_timeline_not_its_amplitude() {
    let full = SpringLeg::new(
        SPRING_EXPRESSIVE_SPATIAL_DEFAULT,
        12.0,
        MotionScale::STANDARD,
    );
    let half = SpringLeg::new(SPRING_EXPRESSIVE_SPATIAL_DEFAULT, 12.0, MotionScale::HALF);
    println!(
        "STANDARD 稳定 {:.1}ms / HALF 稳定 {:.1}ms",
        full.total_ms(),
        half.total_ms()
    );
    assert!(
        (half.total_ms() - full.total_ms() / 2.0).abs() < 1.0e-3,
        "HALF 必须把稳定时间减半：{:.1} vs {:.1}",
        half.total_ms(),
        full.total_ms()
    );
    // 时间轴压缩：半程进度 = 全程在「两倍时刻」的进度（ζ / 峰值都不变）
    let mid_full = full.progress(full.total_ms() * 0.25);
    let mid_half = half.progress(half.total_ms() * 0.25);
    assert!(
        (mid_full - mid_half).abs() < 1.0e-4,
        "同一个相位（全程 25%）的进度必须相同：{mid_full} vs {mid_half}"
    );
}

/// 五档切换在弹簧腿下都落在端点上（不是只有同级换页那一档）。
///
/// 抓得住：把弹簧腿只接到 `Peer`（`Push` / `Pop` / `Boot` 会退回旧的令牌腿——帧数与 16px 的
/// 稳定时间对不上，本条的稳定时间断言红）。
#[test]
fn every_kind_lands_on_its_endpoint_with_a_spring_leg() {
    let cases = [
        Kind::Boot,
        Kind::Peer { forward: true },
        Kind::Peer { forward: false },
        Kind::Push,
        Kind::Pop,
    ];
    for kind in cases {
        let mut pump = standard();
        pump.use_page_spring(Some(SPRING_EXPRESSIVE_SPATIAL_DEFAULT));
        match kind {
            Kind::Boot => {}
            Kind::Peer { .. } | Kind::Push => arm_peer(&mut pump),
            Kind::Pop => {
                arm_peer(&mut pump);
                pump.observe(Some(1), Some(0), Some("instance-7"));
                settle(&mut pump);
            }
        }
        let started = match kind {
            Kind::Boot => pump.observe(Some(2), Some(1), None),
            Kind::Peer { forward: true } => pump.observe(Some(3), Some(1), None),
            Kind::Peer { forward: false } => pump.observe(Some(0), Some(0), None),
            Kind::Push => pump.observe(Some(1), Some(0), Some("instance-7")),
            Kind::Pop => pump.observe(Some(1), Some(0), None),
        };
        assert!(started, "{}: 必须起跑", kind.name());
        let trace = frames(&mut pump);
        println!("{}: {} 帧", kind.name(), trace.len());
        assert_eq!(
            *trace.last().expect("至少一帧"),
            PageFrame::STILL,
            "{}: 末帧必须归位",
            kind.name()
        );
        assert!(
            (12..=20).contains(&trace.len()),
            "{}: {} 帧出窗（12~20）",
            kind.name(),
            trace.len()
        );
        // 起点满幅（进度 0 ⇒ 位移恰好是规格里的起点）
        assert_eq!(
            trace[0].enter,
            page_spec(kind).enter_from,
            "{}: 首帧必须满幅",
            kind.name()
        );
    }
}
