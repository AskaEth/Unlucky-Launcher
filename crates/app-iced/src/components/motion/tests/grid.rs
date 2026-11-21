//! **磁贴视图的动效守卫**（M461）：切视图 / 换磁贴尺寸时逐格淡入。
//!
//! 判据按 M461 的原文写：**看泵不看像素**——①帧数够 ②端点精确 `0 → 1` 且单调不回弹
//! ③减少动效时零帧。每条都写清「抓得住什么 / 抓不住什么」，并给出变异与期望红。
//!
//! ## 帧数怎么算（N 的来源，不是拍的数）
//!
//! 进场腿 = `row_spec().enter` = `Short4` **200 ms**（`design/src/motion/mod.rs` 的令牌表）；
//! 错位 = `Short1` **50 ms**、`max_stagger = 3`（`transition.rs::row_spec`）。于是最后一格在
//! `3 × 50 = 150 ms` 起跑、`350 ms` 收尾；推进步长是 `FRAME_MS = 16 ms`
//! （`pump/mod.rs`，`advance` 不满 16 ms 不动），`350 / 16 = 21.875` ⇒ 到期需要 **22 帧**。
//! 断言取 **≥ 20**：留两帧余量（采样器只可能因为「最后一帧刚好跨过到期点」少算，不可能多算），
//! 而「一帧走完 / 硬跳」这类变异会掉到 1~2 帧，差着一个数量级。
//!
//! ## 变异账（只改一处 ⇒ 期望红在哪条用例哪句断言）
//!
//! **下表每一行都实测过**（改一处 → `cargo test --lib grid` → 复原），不是推演：
//!
//! | 变异（只改这一处） | 实测红在哪 |
//! |---|---|
//! | `pump/layout.rs` 的 `reenter_from` 恒返回 `1.0`（静息行也走 `1 → 1`） | `a_cell_walks_from_zero_to_one_monotonically`（首帧不是 0）**与** `pages/instances/grid/tests.rs::the_grid_actually_fades_while_the_pump_runs`（可见度和恒 6）——两条一起红 |
//! | `pump/layout.rs` 的 `replay` 删掉 `off` 那一支（NONE 也插轨道） | `no_scale_replays_zero_frames`（`samples()` 非 0）**与** `no_scale_renders_every_cell_at_rest`（网格画出了遮罩） |
//! | `pump/rows.rs` 的 `retarget` 里 `stagger.at(index)` 换成 `stagger.start_ms`（错位丢了） | `the_grid_fills_in_over_at_least_twenty_frames`（帧数掉到 13）**与** `no_scale_replays_zero_frames` 的 HALF 那一半（175 ms 掉了 50 ms 的错位） |
//! | `pages/instances/grid.rs` 的 `cell` 里把泵采样换成恒 `1.0`（网格不接泵） | `the_grid_actually_fades_while_the_pump_runs`（`fading == 0`） |
//! | `motion::rows::relayout` 里不调 `replay_rows`（接线断了） | 三条用例的第一句 `assert!(relayout(..))`（本轮**未**单独实测：`view_mode.rs` 的两处调用点没有用例，由出图那一条链证明——见 `DevDocs/artifacts/m461/shots.log` 的 `replay=true`） |
//!
//! ## 抓不住
//!
//! 磁贴**画出来**那一半（网格真的把可见度喂给 `motion::revealed`）不在本文件：那是
//! `pages/instances/grid/tests.rs` 的 `probe` 取样口。本文件只管泵的算术。

use super::super::pump::List;
use super::super::pump::Pump;
use super::super::pump::RowFrame;
use super::super::transition::row_spec;
use super::super::*;
use unlucky_design::motion::MotionScale;

/// 一个**已经对过表**的泵：六格（4 列 ⇒ 两排）静息就位。
fn seeded(scale: MotionScale) -> Pump {
    let mut pump = Pump::new();
    pump.configure(scale, None);
    pump.observe_rows(List::Instances, &["a", "b", "c", "d", "e", "f"]);
    pump
}

/// 一路采样到轨道结束（每帧 16 ms，上限 20 s），返回推进的帧数。
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

/// 判据 ①：**切一次布局，泵要跑 ≥ 20 帧**（N 的算法见模块文档）。
///
/// 抓得住：①「重放」其实没起轨道（接线断了 / `off` 分支写反）；②四格同一拍起跑（错位丢了 ⇒
/// 13 帧）；③时长被改成 `short2` 之类的短档（10 帧以内）。
/// 抓不住：观感，也抓不住**画出来**是不是逐格淡（那是 `grid/tests.rs` 的 `Fade`）。
#[test]
fn the_grid_fills_in_over_at_least_twenty_frames() {
    let mut pump = seeded(MotionScale::STANDARD);
    assert!(!pump.is_animating(), "前提：静息");

    assert!(
        pump.replay_rows(List::Instances),
        "切视图 / 换尺寸必须起跑（接线断了这里就是 false）"
    );
    assert!(pump.is_animating(), "起跑之后必须订阅帧");
    let frames = settle(&mut pump);
    assert!(
        frames >= 20,
        "逐格淡入至少要 20 帧（350ms / 16ms ≈ 22），实测 {frames} 帧"
    );
    assert!(
        !pump.is_animating(),
        "收尾必须当场删轨道（否则空闲还在要帧）"
    );
}

/// 判据 ②：**端点精确 `0 → 1`，且单调不回弹**（每一格单独成立，含错位）。
///
/// 抓得住：①`reenter_from` 恒 `1.0`（静息行 `1 → 1`：首帧不是 0，像素一帧都不变却占满帧）；
/// ②采样器用 `to` 硬跳（中间帧全是 0 或 1）；③缓动换成会过冲的曲线（`> 1.0` 那一句红）。
/// 抓不住：第 3 格起跑时刻的绝对正确性（错位档由 `row_spec` 的 `max_stagger` 决定，
/// 那条判据在 `rows.rs::the_stagger_is_capped_…`）。
#[test]
fn a_cell_walks_from_zero_to_one_monotonically() {
    let mut pump = seeded(MotionScale::STANDARD);
    pump.replay_rows(List::Instances);

    let ids = ["a", "b", "c", "d", "e", "f"];
    let trace = |pump: &Pump| -> Vec<f32> {
        ids.iter()
            .map(|id| pump.row_frame(List::Instances, id).visibility)
            .collect()
    };

    // 首帧：每一格都**严格** 0（错位的意思是「还没轮到它」，不是「从别的值起」）。
    let first = trace(&pump);
    assert_eq!(first, vec![0.0; ids.len()], "起手那一帧必须全是 0");

    let mut seen: Vec<Vec<f32>> = vec![first];
    for _ in 0..1250 {
        if !pump.is_animating() {
            break;
        }
        pump.advance(FRAME_MS);
        seen.push(trace(&pump));
    }

    // 末帧：每一格**逐位** 1.0（不是「差不多」）。
    let last = seen.last().expect("至少采到一帧");
    assert_eq!(last, &vec![1.0; ids.len()], "收尾必须严格到 1");

    // 单调、不越界、中途真的存在严格中间态。
    for index in 0..ids.len() {
        let column: Vec<f32> = seen.iter().map(|frame| frame[index]).collect();
        assert!(
            column.windows(2).all(|pair| pair[1] >= pair[0]),
            "第 {index} 格的可见度必须单调往上（回弹就是闪一下）：{column:?}"
        );
        assert!(
            column.iter().all(|value| (0.0..=1.0).contains(value)),
            "第 {index} 格越界（可见度必须留在 0..=1）：{column:?}"
        );
        assert!(
            column.iter().any(|value| *value > 0.0 && *value < 1.0),
            "第 {index} 格没有严格中间态（一帧走完 = 硬跳）：{column:?}"
        );
    }
}

/// 判据 ③：**减少动效 / 关掉动效时帧数为 0**，界面当场是终态。
///
/// 抓得住：`replay` 里删掉 `off` 那一支（NONE 也插轨道 ⇒ `samples()` 会涨、`is_animating()`
/// 为真、`row_frame` 会在中途给出 `< 1` 的可见度）。
/// 抓不住：系统偏好有没有真的把档位压到 `HALF`（那是 `motion/reduce.rs` 的用例与真机取证）。
///
/// `HALF` 不是零帧：那条腿跑得更短（`Short4` 200 ms ⇒ 100 ms），**转场照旧跑完**——M396 定的
/// 口径是「减轻」不是「全无」，所以这里钉的是「比 STANDARD 少、但仍然 > 0」。
#[test]
fn no_scale_replays_zero_frames() {
    let mut pump = seeded(MotionScale::NONE);
    assert!(!pump.replay_rows(List::Instances), "关掉动效不许起轨道");
    assert!(!pump.is_animating());
    for _ in 0..120 {
        assert!(!pump.advance(FRAME_MS), "NONE 档没有任何可推进的");
    }
    assert_eq!(pump.samples(), 0, "NONE 档的帧采样次数必须是 0");
    for id in ["a", "b", "c", "d", "e", "f"] {
        let frame = pump.row_frame(List::Instances, id);
        assert_eq!(
            frame,
            RowFrame::REST,
            "{id} 在 NONE 档必须是静息（可见度 1、偏移 0），实得 {frame:?}"
        );
    }

    // HALF：帧数减半左右，但**不是零**。
    let mut half = seeded(MotionScale::HALF);
    assert!(half.replay_rows(List::Instances), "HALF 档照旧起跑");
    let frames = settle(&mut half);
    let spec = row_spec();
    let full_ms = f64::from(spec.max_stagger) * f64::from(MotionScale::STANDARD.ms(spec.stagger))
        + f64::from(MotionScale::STANDARD.ms(spec.enter.duration));
    let half_ms = f64::from(spec.max_stagger) * f64::from(MotionScale::HALF.ms(spec.stagger))
        + f64::from(MotionScale::HALF.ms(spec.enter.duration));
    let expected = (half_ms / FRAME_MS).ceil();
    assert!(
        (expected - 2.0..=expected + 2.0).contains(&(frames as f64)),
        "HALF 档应当跑 {expected} 帧左右（{full_ms}ms 的一半是 {half_ms}ms），实测 {frames}"
    );
    assert!(
        frames > 0 && (frames as f64) < full_ms / FRAME_MS,
        "HALF 必须少于 STANDARD（{frames} 帧）"
    );
}
