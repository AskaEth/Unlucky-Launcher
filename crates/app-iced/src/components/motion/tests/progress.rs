//! **进度插值**的守卫（M305）：追值、单调、到点、收束一次、`NONE` 严格等于目标、不卡中间值。
//!
//! ## 变异账（T41：只改一处 ⇒ 期望红在哪条用例哪句断言）
//!
//! | 变异（只改这一处） | 期望红在哪 |
//! |---|---|
//! | `pump/progress.rs` 的 `Progress::value` 里把插值那一行换成 `Some(track.to)`（**改回硬跳**） | `the_value_chases_the_target_instead_of_jumping` 的「没有一帧落在两端之间」（实测见报告） |
//! | `pump/progress.rs` 的 `expire` 里删掉「`track.to >= 1.0` 才排收束」 | `the_settle_fires_once_and_is_zero_at_both_ends`（中途跳变也会亮一下） |
//! | `pump/progress.rs` 的 `settle_amount` 去掉 `if t >= 1.0 { return 0.0 }` | 同一条用例的「末帧必须严格为 0」（脉冲会停在中途） |
//! | `pump/progress.rs` 的 `observe_progress` 里删掉「没有轨道但目标没变 ⇒ return false」那一段 | `a_repeated_reading_after_the_pulse_does_not_fire_it_again`（r1 P1-1） |
//! | `pump/progress.rs` 的 `observe_progress` 里删掉 `is_off()` 那一支 | `no_scale_renders_the_target_itself`（`samples()` 非 0） |
//! | `pump/progress.rs` 的 `progress_value` 里删掉「目标不符就给目标」 | `a_model_that_skips_the_observe_is_not_stuck_midway` |
//! | `pump/progress.rs` 的 `expire` 把返回的到达时刻换回 `now_ms`（M386 之前） | `the_pulse_is_anchored_at_the_arrival_instant`（脉冲晚一帧起，钉住相位时恒 0） |
//! | `pump/progress.rs` 的 `Progress::settle_start` 删掉「轨道越过终点现算」那一支 | `pinned_phase_reaches_the_pulse_without_a_tick`（出图 harness 不推进泵 ⇒ 一帧脉冲都没有） |
//! | `pump/mod.rs` 的 `start_now` 改成恒 `now_ms` | `pinned_phase_stops_the_value_mid_leg`（相位随机器漂） |

use super::super::pump::{ProgressFrame, Pump};
use super::super::transition::progress_spec;
use super::super::*;
use unlucky_design::motion::MotionScale;

fn standard() -> Pump {
    let mut pump = Pump::new();
    pump.configure(MotionScale::STANDARD, None);
    pump
}

/// 推进到没有轨道为止（每帧 16ms，上限 20s）。
fn settle(pump: &mut Pump) {
    for _ in 0..1250 {
        if !pump.is_animating() {
            break;
        }
        pump.advance(FRAME_MS);
    }
}

/// 真实进度一跳，画出来的值**沿一条腿追上去**（不是整数值硬跳），且单调不回弹。
///
/// 抓得住：把插值改回 `to`（第一帧就到位）；或者插值方向做反（值往回走）。
/// 抓不住：观感（出图在 `DevDocs/artifacts/m305/`）。
#[test]
fn the_value_chases_the_target_instead_of_jumping() {
    let mut pump = standard();
    // 第一次报 = 认条，不追值（没有「上一根的尾巴」可追）
    assert!(!pump.observe_progress("install-1", 0.20));
    assert_eq!(pump.progress_value("install-1", 0.20), 0.20);

    // 后端跳了 60 个百分点：画出来要一路追
    assert!(pump.observe_progress("install-1", 0.80));
    let mut seen = vec![pump.progress_value("install-1", 0.80)];
    assert_eq!(seen[0], 0.20, "起跑那一帧必须还是旧值（不许硬跳）");
    for _ in 0..1250 {
        if !pump.is_animating() {
            break;
        }
        pump.advance(FRAME_MS);
        seen.push(pump.progress_value("install-1", 0.80));
    }
    assert!(seen.len() > 4, "追值要跨多帧：{seen:?}");
    assert!(
        seen.windows(2).all(|pair| pair[1] >= pair[0] - 1.0e-6),
        "进度不许倒退（那读起来像卡了）：{seen:?}"
    );
    // 中间帧必须**真的落在两端之间**：这条是「改回硬跳」那一路的落点（硬跳时 seen 只有 0.20 与 0.80）
    assert!(
        seen.iter()
            .any(|value| *value > 0.20 + 1.0e-3 && *value < 0.80 - 1.0e-3),
        "没有一帧落在两端之间 ⇒ 插值退回了硬跳：{seen:?}"
    );
    let end = *seen.last().unwrap();
    assert!((end - 0.80).abs() < 1.0e-6, "收尾必须严格到目标：{end}");
}

/// 到达 100% 有**一次**强调收束：端点严格 0、中间非 0、**不循环**。
#[test]
fn the_settle_fires_once_and_is_zero_at_both_ends() {
    let mut pump = standard();
    pump.observe_progress("install-1", 0.90);
    settle(&mut pump);
    assert!(!pump.is_animating(), "前提：在追的那一腿已经走完");
    assert_eq!(
        pump.progress_settle("install-1", 0.90),
        0.0,
        "没到 100% 就没有收束"
    );

    assert!(pump.observe_progress("install-1", 1.0));
    // 追值那一腿走完的那一帧才开始排收束
    for _ in 0..1250 {
        if pump.progress_settle("install-1", 1.0) > 0.0 {
            break;
        }
        pump.advance(FRAME_MS);
    }
    let peak = pump.progress_settle("install-1", 1.0);
    assert!(peak > 0.0, "到达 100% 必须有一次收束");
    // 收束期间值恒为目标（不许被脉冲带偏）
    assert!(
        (pump.progress_value("install-1", 1.0) - 1.0).abs() < 1.0e-6,
        "收束只改强调，不改数值"
    );

    let spec = progress_spec();
    let mut samples = vec![peak];
    for _ in 0..1250 {
        if !pump.is_animating() {
            break;
        }
        pump.advance(FRAME_MS);
        samples.push(pump.progress_settle("install-1", 1.0));
    }
    assert!(
        *samples.last().unwrap() == 0.0,
        "收束必须**严格**归零（末帧与静息逐位相同）：{samples:?}"
    );
    assert!(
        samples.iter().all(|sample| *sample <= 1.0),
        "脉冲强度不许越界"
    );
    // 不循环：收尾之后一直为 0
    for _ in 0..10 {
        pump.advance(FRAME_MS);
        assert_eq!(pump.progress_settle("install-1", 1.0), 0.0, "不许循环");
    }
    let _ = spec;
}

/// **同一条读数再报一次不许再排一次收束**（r1 P1-1，产品可达）。
///
/// 场景：最后一个文件下完之后 `ratio()` 恒为 1.0，而 `Installing` / `Registering` 阶段每条 stage
/// 事件都会再走一遍 `observe`（`host/install.rs:216,239` → refresh_view → `from_snapshot`）。
/// 少了那条短路，每来一条事件就插一条 `from == to` 的空轨道 ⇒ 200ms 后**再闪一次** 150ms 脉冲，
/// 既违反「一次收束不循环」，也空烧 350ms 的帧。
///
/// 抓得住：删掉 `observe_progress` 里那条 `track.is_none() && 目标没变` 的短路。
/// 抓不住：脉冲本身的观感（出图负责）。
#[test]
fn a_repeated_reading_after_the_pulse_does_not_fire_it_again() {
    let mut pump = standard();
    pump.observe_progress("install-1", 0.98);
    settle(&mut pump);
    assert!(pump.observe_progress("install-1", 1.0));
    // 推完追值 + 收束（一次）
    settle(&mut pump);
    assert!(!pump.is_animating(), "前提：脉冲已经走完");

    for beat in 0..5 {
        assert!(
            !pump.observe_progress("install-1", 1.0),
            "第 {beat} 条事件不该起轨道（同一条读数）"
        );
        assert!(!pump.is_animating(), "第 {beat} 条事件不该让泵再转起来");
        assert_eq!(
            pump.progress_settle("install-1", 1.0),
            0.0,
            "第 {beat} 条事件不该再闪一次收束"
        );
    }
    // 真变了才动（0.5 是回落，重装/重试）
    assert!(pump.observe_progress("install-1", 0.5));
}

/// **`MotionScale::NONE` 严格等于目标**、零采样（任务里那条「值仍是最终值」的断言版）。
#[test]
fn no_scale_renders_the_target_itself() {
    let mut pump = Pump::new();
    pump.configure(MotionScale::NONE, None);
    pump.observe_progress("install-1", 0.10);
    assert!(!pump.observe_progress("install-1", 0.95));
    assert_eq!(pump.progress_value("install-1", 0.95), 0.95);
    assert!(!pump.is_animating());
    assert_eq!(pump.samples(), 0);

    assert!(!pump.observe_progress("install-1", 1.0));
    assert_eq!(
        pump.progress_settle("install-1", 1.0),
        0.0,
        "关掉动效不许有收束脉冲"
    );
    assert_eq!(pump.samples(), 0, "NONE 档一帧都不许采样");
}

/// 模型跳过了 `observe`（截图 harness / 单测直接构造模型）：**宁可无动效，也不许卡在中间值**。
#[test]
fn a_model_that_skips_the_observe_is_not_stuck_midway() {
    let mut pump = standard();
    pump.observe_progress("install-1", 0.20);
    assert!(pump.observe_progress("install-1", 0.40));
    pump.advance(FRAME_MS);
    let mid = pump.progress_value("install-1", 0.40);
    assert!(mid > 0.20 && mid < 0.40, "前提：正在追（{mid}）");

    // 有人把目标直接改成 0.90 却没走 observe：必须直接给 0.90
    assert_eq!(
        pump.progress_value("install-1", 0.90),
        0.90,
        "目标不符时必须给目标（不许画一个谁也说不清的中间值）"
    );
    // 换了一根条（另一个安装）同理
    assert_eq!(pump.progress_value("install-2", 0.50), 0.50);
}

/// 换一根条**不追值**：新安装不该从上一根的位置爬过来。
#[test]
fn a_new_bar_does_not_chase_from_the_previous_one() {
    let mut pump = standard();
    pump.observe_progress("install-1", 0.10);
    assert!(pump.observe_progress("install-1", 0.90));
    pump.advance(FRAME_MS);

    assert!(!pump.observe_progress("install-2", 0.30));
    assert_eq!(
        pump.progress_value("install-2", 0.30),
        0.30,
        "换条必须直接落目标（否则会从上一根的 0.2 爬到 0.3）"
    );
}

// ---------------------------------------------------------------- M386：钉住相位
//
// 这三条是 I1 出图的**可复现性判据**：同一份 argv 每次都停在同一个相位，图才谈得上「逐字节
// 对照」。相位怎么换算写在 `pump/progress.rs` 的模块文档表格里。

/// 钉住相位 ⇒ 追值停在**中段**，且帧推进不再让值漂。
///
/// `start_now()` 在 pin 下给 0、`elapsed_ms()` 给 pin ⇒ `value = from + (to−from)·easing(pin/200)`。
/// pin=100 ⇒ 半程（`short4` 200ms）。
///
/// 抓得住：`start_now` 改成恒 `now_ms`（相位变成「离起跑多久」，随机器变，图不可复现）。
/// 抓不住：`easing` 表本身漂（那要像素，见 `DevTools/artifacts/quickiced`）。
#[test]
fn pinned_phase_stops_the_value_mid_leg() {
    let mut pump = Pump::new();
    pump.configure(MotionScale::STANDARD, Some(100.0));
    pump.observe_progress("install-1", 0.20);
    assert!(
        pump.observe_progress("install-1", 0.80),
        "跳了 60 个点，必须起轨道"
    );
    let mid = pump.progress_value("install-1", 0.80);
    assert!(mid > 0.20 && mid < 0.80, "pin=100 必须落在两端之间：{mid}");
    // 帧照常到达，但时钟被钉住 ⇒ 同一相位、同一个值
    for _ in 0..3 {
        pump.advance(FRAME_MS);
        assert_eq!(
            pump.progress_value("install-1", 0.80),
            mid,
            "钉住相位后值不许漂"
        );
    }
}

/// 钉住相位 ⇒ 收束脉冲**也在**（`pin = 到达时刻 + 脉冲半程`），而且**不用推进泵**。
///
/// 出图 harness（`pages_demo`）的 `Message::Frame` 只数帧与派采样，**不调** `motion::advance()`
/// ⇒ `expire` 在那条通道里从不跑 ⇒ 脉冲若只由 `expire` 排，出图通道里一帧都拍不到。
/// 这一条就是那个出口的判据（[`Progress::settle_start`] 的第二支）。
#[test]
fn pinned_phase_reaches_the_pulse_without_a_tick() {
    let mut pump = Pump::new();
    // 到达时刻 = 0 + 200（short4）；脉冲半程 = +75（short3 150ms 的一半）⇒ 顶点
    pump.configure(MotionScale::STANDARD, Some(275.0));
    pump.observe_progress("install-1", 0.50);
    assert!(pump.observe_progress("install-1", 1.0));
    // **一次 advance 都不调**：模拟出图 harness
    let pulse = pump.progress_settle("install-1", 1.0);
    assert!(pulse > 0.99, "pin=275 该落在脉冲顶点（1.0）：{pulse}");
    assert_eq!(
        pump.progress_value("install-1", 1.0),
        1.0,
        "收束只改强调，不改数值"
    );
}

/// 脉冲的两端**严格**是 0：`pin < 到达时刻` 与 `pin ≥ 到达 + 时长` 都必须是 0。
///
/// 抓得住：`settle_amount` 去掉 `t >= 1.0 || t <= 0.0` 那一条（端点会留一个非 0 的尾巴，
/// 图上看是「亮着不灭」）。
#[test]
fn the_pinned_pulse_is_zero_outside_its_window() {
    for (pin, what) in [
        (100.0, "还没到 100%"),
        (350.0, "脉冲已经走完"),
        (500.0, "早就走完了"),
    ] {
        let mut pump = Pump::new();
        pump.configure(MotionScale::STANDARD, Some(pin));
        pump.observe_progress("install-1", 0.50);
        assert!(pump.observe_progress("install-1", 1.0));
        assert_eq!(
            pump.progress_settle("install-1", 1.0),
            0.0,
            "pin={pin}（{what}）不该有脉冲"
        );
    }
}

/// 产品路径（不钉相位）：脉冲的起点是**到达时刻**，不是「发现它到期的那一帧」。
///
/// 200ms 那条腿在第 13 帧（208ms）才被 `expire` 发现；脉冲若从 208ms 起算，那一帧的强度是 0
/// （`t = 0`），而正确值是 `sin(π · 8/150) ≈ 0.167`。
///
/// 抓得住：`expire` 把 `now_ms` 当到达时刻（M386 之前就是这么写的，钉住相位时恒为 0）。
/// 抓不住：脉冲的观感（出图负责）。
#[test]
fn the_pulse_is_anchored_at_the_arrival_instant() {
    let mut pump = standard();
    pump.observe_progress("install-1", 0.50);
    assert!(pump.observe_progress("install-1", 1.0));
    for _ in 0..13 {
        pump.advance(FRAME_MS);
    }
    let pulse = pump.progress_settle("install-1", 1.0);
    assert!(
        pulse > 0.1,
        "脉冲必须从到达时刻（200ms）起算，不是这一帧（208ms）：{pulse}"
    );
}

/// 自证日志**只在取证通道**出现：产品路径（没有 pin）一行都不许打。
///
/// 抓得住：把 `progress_probe_line` 的 `self.pin?` 去掉（产品路径每帧一行，日志被刷爆）。
#[test]
fn the_probe_line_only_exists_for_the_evidence_channel() {
    let pinned = {
        let mut pump = Pump::new();
        pump.configure(MotionScale::STANDARD, Some(100.0));
        pump.observe_progress("install-1", 0.20);
        pump.observe_progress("install-1", 0.80);
        let frame = ProgressFrame {
            value: pump.progress_value("install-1", 0.80),
            settle: pump.progress_settle("install-1", 0.80),
        };
        pump.progress_probe_line("install-1", 0.80, &frame)
    };
    let line = pinned.expect("钉住相位时必须有自证行");
    assert!(line.contains("phase=100ms"), "{line}");
    assert!(line.contains("leg=0.200->0.800"), "{line}");
    assert!(line.contains("value="), "{line}");

    let product = {
        let mut pump = standard();
        pump.observe_progress("install-1", 0.20);
        pump.observe_progress("install-1", 0.80);
        let frame = ProgressFrame {
            value: pump.progress_value("install-1", 0.80),
            settle: 0.0,
        };
        pump.progress_probe_line("install-1", 0.80, &frame)
    };
    assert!(product.is_none(), "产品路径不许打这行：{product:?}");
}
