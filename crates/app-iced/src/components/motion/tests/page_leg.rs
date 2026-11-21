//! 页面切换**进场腿**的逐帧取证（M432）：同一段操作在「改前 / 改后」必须逐帧一致。
//!
//! M432 在泵里加了一条**弹簧驱动**的进场腿（`pump/spring_leg.rs`），默认**关闭**。所以「旧路径
//! 逐帧未变」不能只靠读代码保证：本文件把五档切换的整条轨迹打出来，改前 / 改后各跑一次，
//! 两份日志 `diff` 为空才算数（报告里贴的是这条事实，不是「看起来一样」）。
//!
//! 所以本文件**只依赖旧 API**（`observe` / `page_frame` / `advance`）：它在改前的树上也能编译，
//! 「改前」那一半日志才有可信来源。弹簧那条路的判据在 [`spring_leg`](super::spring_leg)。
//!
//! 顺带守三条（改前改后都成立）：首帧满幅、末帧 [`PageFrame::STILL`]、位移逐帧不增。

use super::super::pump::{Pump, FRAME_MS};
use super::super::transition::{page_spec, Kind, PageFrame};
use super::{settle, standard};

/// 五档切换 + 稳定名字（日志用）。
const CASES: [(Kind, &str); 5] = [
    (Kind::Boot, "boot"),
    (Kind::Peer { forward: true }, "peer-forward"),
    (Kind::Peer { forward: false }, "peer-back"),
    (Kind::Push, "push"),
    (Kind::Pop, "pop"),
];

/// 把泵推到某一档切换的**起点**（首屏那一次先走完，否则判据会被 Boot 吃掉）。
fn arm(pump: &mut Pump, kind: Kind) {
    match kind {
        Kind::Boot => {}
        Kind::Peer { forward: true } | Kind::Peer { forward: false } => {
            pump.observe(Some(1), Some(0), None);
            settle(pump);
        }
        // 进 / 出深层要先有一个**屏**：Pop 还得先真的进过一层，否则详情标识没变、不起跑。
        Kind::Push => {
            pump.observe(Some(1), Some(0), None);
            settle(pump);
        }
        Kind::Pop => {
            pump.observe(Some(1), Some(0), None);
            settle(pump);
            pump.observe(Some(1), Some(0), Some("instance-7"));
            settle(pump);
        }
    }
    assert_eq!(pump.page_frame(), PageFrame::STILL, "起点必须是静息");
}

/// 每帧 16 ms 一路采样到轨道结束（含起点与终点）。
fn trace(kind: Kind) -> Vec<PageFrame> {
    let mut pump = standard();
    arm(&mut pump, kind);
    let started = match kind {
        Kind::Boot => pump.observe(Some(2), Some(1), None),
        Kind::Peer { forward: true } => pump.observe(Some(3), Some(1), None),
        Kind::Peer { forward: false } => pump.observe(Some(0), Some(0), None),
        Kind::Push => pump.observe(Some(1), Some(0), Some("instance-7")),
        Kind::Pop => pump.observe(Some(1), Some(0), None),
    };
    assert!(started, "{}: 必须起跑", kind.name());
    assert_eq!(pump.kind(), Some(kind), "{}: 判档必须一致", kind.name());

    let mut out = vec![pump.page_frame()];
    let mut guard = 0;
    while pump.is_animating() {
        pump.advance(FRAME_MS);
        out.push(pump.page_frame());
        guard += 1;
        assert!(
            guard < 200,
            "{}: 轨道没跑完（终止条件疑似失效）",
            kind.name()
        );
    }
    out
}

/// **旧路径的逐帧输出**（改前 / 改后 `diff` 用的那一半）。
///
/// 抓得住：谁把默认路径（`page_spring = None`）动了哪怕一个像素——整条轨迹有一帧不同，
/// 与改前日志的 `diff` 就不为空。
/// 抓不住：弹簧腿自己的端点与单调性（那是 [`spring_leg`](super::spring_leg) 的用例）。
#[test]
fn the_page_enter_leg_trace_is_frozen_for_before_after_comparison() {
    for (kind, name) in CASES {
        let frames = trace(kind);
        println!("=== {name} : {} 帧 ===", frames.len());
        for (index, frame) in frames.iter().enumerate() {
            println!("{index:02} {frame:?}");
        }

        let spec = page_spec(kind);
        assert_eq!(frames[0].enter, spec.enter_from, "{name}: 首帧必须满幅");
        assert_eq!(frames[0].exit, (0.0, 0.0), "{name}: 旧页从 0 起");
        assert_eq!(
            *frames.last().expect("轨迹至少一帧"),
            PageFrame::STILL,
            "{name}: 末帧必须归位"
        );
        assert!(
            (12..=20).contains(&frames.len()),
            "{name}: 进入应落在 12~20 帧，实测 {} 帧",
            frames.len()
        );

        // 新页位移**逐帧不增**（进场的旧路径是单调缓动 + 取整，反弹一次就红）
        for pair in frames.windows(2) {
            assert!(
                pair[1].enter.0.abs() <= pair[0].enter.0.abs(),
                "{name}: 水平位移不许回升：{:?} → {:?}",
                pair[0].enter,
                pair[1].enter
            );
            assert!(
                pair[1].enter.1.abs() <= pair[0].enter.1.abs(),
                "{name}: 纵向位移不许回升：{:?} → {:?}",
                pair[0].enter,
                pair[1].enter
            );
        }
        // 旧页反向退场：幅度**逐帧不降**。最后一帧是轨道删掉之后的静息（位移回到 `(0,0)`，
        // 那时早就不画旧页了），不算退场的一帧。
        for pair in frames[..frames.len() - 1].windows(2) {
            assert!(
                pair[1].exit.0.abs() >= pair[0].exit.0.abs(),
                "{name}: 旧页退场不许倒退：{:?} → {:?}",
                pair[0].exit,
                pair[1].exit
            );
            assert!(
                pair[1].exit.1.abs() >= pair[0].exit.1.abs(),
                "{name}: 旧页退场不许倒退：{:?} → {:?}",
                pair[0].exit,
                pair[1].exit
            );
        }
    }
}
