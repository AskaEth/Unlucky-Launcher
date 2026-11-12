//! 模态 / 系统偏好 / 有界泵的降级守卫（M256）。从 `tests/mod.rs` 拆出来守 400 行硬线。

use super::super::pump::FRAME_MS;
use super::super::transition::{modal_spec, page_spec, Kind, ModalFrame, PageFrame};
use super::super::*;
use super::{settle, standard};
use unlucky_design::motion::{DurationToken, MotionScale};

/// 模态表：进 200 / 出 150，且起点缩放来自 design 令牌（0.96）。
///
/// 抓得住：谁把某一腿改长/改短到不再"进长于出"，或把起点缩放从令牌换回字面量。
/// 抓不住：这套时长在真机上顺不顺眼（D30 的事）。
#[test]
fn modal_spec_is_short_and_asymmetric() {
    let spec = modal_spec();
    assert_eq!(spec.enter.duration, DurationToken::Short4); // 200
    assert_eq!(spec.exit.duration, DurationToken::Short3); // 150
    assert!(spec.enter.duration.ms() > spec.exit.duration.ms());
    assert!((modal_scale(0.0) - unlucky_design::motion::DIALOG_ENTER_SCALE).abs() < 1.0e-6);
    assert!((modal_scale(1.0) - 1.0).abs() < 1.0e-6);
    // 单调（不弹跳：M3E 的对话框不许回弹）
    let mut previous = modal_scale(0.0);
    for step in 1..=20 {
        let value = modal_scale(step as f32 / 20.0);
        assert!(value >= previous, "缩放倒退了：{previous} → {value}");
        previous = value;
    }
}

// ------------------------------------------------------------------ 模态

/// 模态：进场 12~13 帧（200ms）、展开后**不再要帧**、退场反向且更短、走完交回一次「已关」。
///
/// 抓得住：①进场时长被改成 >300ms（帧数跑出去）；②`Open` 还占着轨道（`is_animating` 为真 ⇒
/// 对话框开着就一直在烧帧）；③退场方向反了（`progress` 变成递增）；④`take_modal_closed`
/// 不返回 `true`（框永远关不掉）。
/// 抓不住：缩放在**屏幕上**是从中心还是从某个角长出来的（命中与几何都没验，见报告）。
#[test]
fn a_modal_opens_stops_burning_frames_and_closes_in_reverse() {
    let mut pump = standard();
    assert!(pump.observe_modal(true), "开框要起跑进场");
    assert!(pump.modal_frame().visible);
    assert!(pump.modal_frame().progress < 0.5, "起手时是收起状态");

    let enter_frames = settle(&mut pump) as usize;
    assert!(
        (10..=14).contains(&enter_frames),
        "进场 200ms 应约 12 帧，实测 {enter_frames}"
    );
    assert_eq!(
        pump.modal_frame(),
        ModalFrame {
            visible: true,
            progress: 1.0
        }
    );
    assert!(
        !pump.is_animating(),
        "**展开后不许占帧**（对话框可能开几分钟，那不是动画）"
    );
    assert!(!pump.observe_modal(true), "已经开着 ⇒ 不重新起跑");

    // 退场：反向（1 → 0），且比进场短
    assert!(!pump.begin_modal_close(), "有动效时不许当场关");
    assert!(pump.modal_closing());
    let mut trace = vec![pump.modal_frame().progress];
    while pump.is_animating() {
        pump.advance(FRAME_MS);
        trace.push(pump.modal_frame().progress);
    }
    let exit_frames = trace.len() - 1;
    assert!(
        exit_frames < enter_frames,
        "退场（{exit_frames} 帧）必须短于进场（{enter_frames} 帧）"
    );
    assert!(
        trace.windows(2).all(|pair| pair[0] >= pair[1]),
        "退场进度必须单调下降：{trace:?}"
    );
    assert_eq!(pump.modal_frame(), ModalFrame::HIDDEN, "走完即收起");
    assert!(pump.take_modal_closed(), "必须交回一次「已关」");
    assert!(
        !pump.take_modal_closed(),
        "只交回一次（重复关会把下一个框也关掉）"
    );
}

/// 弹窗被别处**硬关**（不是走 `begin_modal_close`）时不玩退场：直接收起、不残留「待关」。
///
/// 抓得住：把 `observe_modal(false)` 改成也起退场 ⇒ `is_animating()` 为真，本条红。
/// 抓不住：那条硬关路径在真机上多久发生一次（`state/refresh.rs` 的 settle 分支，未统计）。
#[test]
fn a_dialog_closed_elsewhere_does_not_leave_a_pending_close() {
    let mut pump = standard();
    pump.observe_modal(true);
    settle(&mut pump);
    assert!(pump.modal_frame().visible);

    assert!(!pump.observe_modal(false));
    assert!(!pump.is_animating());
    assert_eq!(pump.modal_frame(), ModalFrame::HIDDEN);
    assert!(!pump.take_modal_closed(), "没有退场就没有「待关」");
}

/// 运行期把关掉动效（`configure(NONE)`）要立刻落终态，而不是把动画「缩到 0 时长」。
///
/// 抓得住：把 `configure` 里 `tracks.clear()` 那段删掉（轨道留着 ⇒ 仍在占帧）。
/// 抓不住：真实用户什么时候会中途改系统开关（那要改系统设置，见报告的未验清单）。
#[test]
fn switching_to_none_mid_flight_settles_immediately() {
    let mut pump = standard();
    pump.observe(Some(1), Some(0), None);
    pump.advance(FRAME_MS);
    assert!(pump.is_animating());

    pump.configure(MotionScale::NONE, None);
    assert!(!pump.is_animating(), "关掉动效必须当场清空轨道");
    assert_eq!(pump.page_frame(), PageFrame::STILL);
}

// ------------------------------------------------------------------ 系统偏好合成

/// 系统「减少动效」三态与档位的合成（塔的硬约束 C）：**`Unknown` 什么都不改**。
///
/// 抓得住：把 `Unknown` 也降级成 `NONE`（"探针失败就静默关动效"），或把 `Reduced` 漏了。
/// 抓不住：真机注册表/系统设置里的值（那要改系统设置，见报告"没验的部分"）。
#[test]
fn reduced_motion_only_triggers_on_an_explicit_reduced() {
    use crate::platform::motion::ReduceMotion;

    for scale in [MotionScale::STANDARD, MotionScale::HALF, MotionScale::NONE] {
        assert_eq!(
            effective_scale(scale, ReduceMotion::Reduced, false),
            MotionScale::NONE,
            "系统明确要求减少动效 ⇒ 归零"
        );
        assert_eq!(
            effective_scale(scale, ReduceMotion::AnimationsOn, false),
            scale,
            "系统里动效开着 ⇒ 用原档"
        );
        assert_eq!(
            effective_scale(scale, ReduceMotion::Unknown, false),
            scale,
            "**读不到 ≠ 要减少**：探针失败不许静默关掉动效"
        );
        assert_eq!(
            effective_scale(scale, ReduceMotion::Reduced, true),
            scale,
            "取证相位钉住时不被系统偏好降级（多帧序列图要可复现）"
        );
    }
}

// ------------------------------------------------------------------ 取证相位

/// `ULL_MOTION_AT` 的钉住相位：时间轴不前进，进度恒定（多帧序列图靠它可复现）。
///
/// 抓得住：把 `start_now`/`elapsed_ms` 的钉住分支删掉（进度会随时间漂，序列图不再可复现）。
/// 抓不住：**出图本身**——这条只证明"相位恒定"，图得另外拍（见报告的未验清单）。
#[test]
fn pinned_phase_freezes_the_clock() {
    let mut pump = Pump::new();
    pump.configure(MotionScale::STANDARD, Some(150.0));
    pump.observe(Some(1), Some(0), None); // 300ms 档 ⇒ 钉在 150 = 一半

    for _ in 0..10 {
        pump.advance(FRAME_MS);
    }
    let frame = pump.page_frame();
    let expected = 16.0 * (1.0 - page_spec(Kind::Boot).enter.easing.sample(0.5));
    assert!(
        (frame.enter.1 - expected.round()).abs() < 1.0,
        "钉在 150/300 的位移应约 {expected}，实测 {}",
        frame.enter.1
    );
    assert!(pump.is_animating(), "钉住的相位不会自然结束（取证专用）");
}

/// **退场途中来了新框**：当场收尾旧退场、重新走进场（复审 P2-2）。
///
/// 覆盖三条后果里的 ①（不再按递减进度倒放）与 ③（退场到期那拍不会整层消失一帧）；
/// ②（闸门把新框的按钮全吞）由 `!modal_closing()` 间接证明——闸门判据就是它。
///
/// 抓得住：把 `observe_modal` 里「`Closing` 先当场收尾」那段删掉 ⇒ 本条红（progress 递减、
/// 且 `modal_closing()` 仍为真）。
/// 抓不住：真实 UI 上「框 A 缩到一半、框 B 从 0.96 长起来」那一帧的观感（D30）。
#[test]
fn a_new_modal_during_the_exit_restarts_the_entrance() {
    let mut pump = standard();
    assert!(pump.observe_modal(true), "框 A 进场");
    settle(&mut pump);
    assert!(!pump.begin_modal_close(), "关 A：退场开始");
    for _ in 0..3 {
        pump.advance(FRAME_MS);
    }
    let halfway = pump.modal_frame().progress;
    assert!(
        halfway > 0.0 && halfway < 1.0,
        "退场走到一半，实测 {halfway}"
    );

    // 框 B 这时弹出来
    assert!(pump.observe_modal(true), "新框必须起跑新的进场轨道");
    assert_eq!(
        pump.modal_frame().progress,
        0.0,
        "①重新走进场：进度从 0 开始，而不是接着倒放"
    );
    assert!(!pump.modal_closing(), "②闸门必须松开（否则新框按钮全被吞）");
    assert!(
        pump.take_modal_closed(),
        "旧退场被当场收尾，交回一次「已关」"
    );

    // ③之后每一帧都必须可见（不许闪一帧空的模态层）
    let mut frames = 0;
    while pump.is_animating() {
        assert!(pump.modal_frame().visible, "③中途不许整层消失");
        pump.advance(FRAME_MS);
        frames += 1;
    }
    assert!(frames > 0);
    assert_eq!(pump.modal_frame().progress, 1.0, "进场必须走到完全展开");
}

/// **产品入口真的把系统偏好合成进来**（复审 P2-5 要求的「故意走真探针」那条）。
///
/// 断言的是**自洽**而不是某个具体值：`configure` 的结果必须等于
/// `effective_scale(STANDARD, ReduceMotion::probe(), false)`。于是在「系统关掉动画」的机器上
/// 它仍然是绿的（那时两边都是 `NONE`），而一旦有人把产品入口接回「直接用传入档位」就会红。
///
/// 抓得住：`motion::configure` 里那一行 `effective_scale(...)` 被去掉。
/// 抓不住：宿主机上「用户设的那一格到底读对没有」（那要改系统设置，见报告的未验清单）。
#[test]
fn configure_goes_through_the_real_os_probe() {
    use crate::platform::motion::ReduceMotion;

    reset();
    configure(MotionScale::STANDARD, None); // 产品入口：不绕过探针
    let expected = effective_scale(MotionScale::STANDARD, ReduceMotion::probe(), false);
    assert_eq!(
        scale(),
        expected,
        "产品入口必须真的把系统偏好合成进来（读不到 = 不改档）"
    );
    reset();
}

/// **进场途中被关掉**：进度必须从**当前值**接着缩，不许先跳到 1.0 再退（复审 r2 的 P2-2）。
///
/// 抓得住：把 `begin_modal_close` 的起点改回恒 `1.0`（旧实现）⇒ 下面那条
/// `（关掉后首帧）<=（关掉前一帧）` 立刻红——那就是"反向硬跳"。
/// 抓不住：那一帧在真实屏幕上有多刺眼（D30 的观感，不是断言）。
#[test]
fn closing_during_the_entrance_does_not_jump_to_fully_open() {
    let mut pump = standard();
    assert!(pump.observe_modal(true), "起跑进场");
    for _ in 0..3 {
        pump.advance(FRAME_MS); // 走到进场的 1/4 左右
    }
    let before = pump.modal_frame().progress;
    assert!(
        before > 0.0 && before < 1.0,
        "前提：进场走到半路，实测 {before}"
    );

    assert!(!pump.begin_modal_close(), "有动效 ⇒ 起跑退场");
    let first = pump.modal_frame().progress;
    assert!(
        first <= before + 1.0e-6,
        "关掉后的首帧进度不许**变大**（{before} → {first} 就是反向硬跳）"
    );
    assert!(
        (first - before).abs() < 0.05,
        "起点必须接在当前进度上（{before} → {first}）"
    );

    // 之后单调缩到 0，且必须真的走完（交回一次「已关」）
    let mut previous = first;
    while pump.is_animating() {
        pump.advance(FRAME_MS);
        let now = pump.modal_frame().progress;
        assert!(
            now <= previous + 1.0e-6,
            "退场必须单调缩小：{previous} → {now}"
        );
        previous = now;
    }
    assert_eq!(pump.modal_frame(), ModalFrame::HIDDEN);
    assert!(pump.take_modal_closed());
}

/// **钉住的档压过任何来源的取值**（复审 r2 的根因机制）。
///
/// `App::on_boot` 会再调一次 `configure`（`app/update.rs:276`），值来自
/// `boot_env::motion_scale(...)`（本机默认 HALF、`ULL_MOTION=off` 时 NONE）。本用例把那次调用
/// 的**取值**直接喂进来：钉住 STANDARD 之后，`configure(HALF)` / `configure(NONE)` 都改不动它。
///
/// 抓得住：删掉 `configure` 里「`FORCED_SCALE` 优先」那一段 ⇒ 两行断言全红（= r2 之前的真红）。
/// 抓不住：`boot_env` 自己那张分支表（env / 后端 tier 的取值规则，不在本 mission 的 scope）。
#[test]
fn a_pinned_scale_survives_every_later_configure() {
    reset();
    configure_for_test(MotionScale::STANDARD, None);
    configure(MotionScale::HALF, None); // = 本机 ICED_BACKEND 未设时 boot_env 的默认档
    assert_eq!(scale(), MotionScale::STANDARD);
    configure(MotionScale::NONE, None); // = ULL_MOTION=off 时 boot_env 的取值
    assert_eq!(
        scale(),
        MotionScale::STANDARD,
        "钉住的档必须压过 on_boot 的取值"
    );
    reset();
}
