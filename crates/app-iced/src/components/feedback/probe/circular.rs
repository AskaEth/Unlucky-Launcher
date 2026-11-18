//! 环形进度的渲染层判据（`probe` 的子模块，`inputs/probe/` 的同一先例）。
//!
//! ## 判据：六个**固定窗**逐个测存在性与尺寸
//!
//! gallery 里这一排环在**固定 x 栅格**上：左缘 `24 + 56·i`（环宽 40、间距 16），共 6 个
//! （0% / 25% / 50% / 75% / 100% / ind 0.30）。判据是「每个窗里都必须有一圈
//! **40(±2)×40(±2)** 的墨迹（轨道 ∪ 激活色的并集）」：
//!
//! - 少一个环 → 该窗没有墨迹 → 红；
//! - 某个环被画小了（例如 20×20）→ 该窗墨迹的外接框变小 → 红；
//! - 0% 那格只有轨道、没有激活色 → 用「并集」判，不把「没有激活色」当失败。
//!
//! 早先用「主色连通簇数」判（只要求非空）：把 5 个环涂掉仍然绿（复审 r3 P2-3 的反证），
//! 所以 G18-i 的处置从「不修」改成**已用固定窗收口**。簇数仍在日志里打印供复核。
//!
//! 不定态的**节奏**（4 圈/周期、跨周期不跳变）不在这里量：那是时间的函数，截图只有一帧，
//! 判据在 `tests/progress.rs::circular_indeterminate_rotation_is_four_turns_and_continuous`。
//!
//! 欠账 **G18-l**（登记不修）：`snackbar` 的「取景框」判据会把条自身的抗锯齿底行算成框
//! （覆盖率 0.5008，余量 0.0008），方向是**误报**不是假绿；本轮给 snackbar 加了「框高 = 96」
//! 之后触发概率下降，但那 0.0008 的余量仍在。

use iced::Color;

use super::super::pixel::{blobs, coverage, dims, rgb, BBox, Shot};
use unlucky_design::color::Role;

use crate::theme::Ui;

/// 环直径（A1 `CircularProgressIndicatorTokens.Size = 40`）。
const DIAMETER: usize = 40;
/// 环排栅格的左缘（页内边距 24）。
const RING_X0: usize = 24;
/// 栅格间距（40 环 + 16 = `Space::X4`）。
const RING_PITCH: usize = 56;
/// 窗数（0 / 25 / 50 / 75 / 100 / ind）。
const RINGS: usize = 6;

/// 某个像素算不算「这块墨」（覆盖率 ≥50%，与 `pixel::runs` 同一口径）。
fn is_ink(pixel: [u8; 3], fill: Color, under: Color) -> bool {
    coverage(pixel, rgb(fill), rgb(under)) >= 0.5
}

/// 环形那一排的判据：逐窗测「有没有一圈 40×40 的墨迹」。
pub(super) fn check(shot: &Shot, ui: &Ui) {
    let active = ui.color(Role::Primary);
    // M248：轨道改成容器面角色（与线性同源），参考值写字面角色，不从 `progress_roles()` 读回。
    let track = ui.color(Role::SurfaceContainerHighest);
    let surface = ui.color(Role::Surface);

    // 环带的 y：以第一个窗（0% 环，只有轨道色）的**圆环签名**定位 —— 中间那几十行在窗的
    // 左右两侧都有墨迹；再向上/下各扩到「窗内还有墨」为止（把圆顶/圆底那两行收回来）。
    //
    // 为什么不按「每行墨点数 4..20」判：圆顶那一行笔画横跨 ~20~28 px，会落在区间外，
    // 按连续段会把 40 高的带子切成 32 行；为什么不按「首行..末行」：深色档里正文的抗锯齿
    // 像素也会落在轨道色的混色线上，跨度会扩到 274 行（两种都实测过）。
    let ink_count = |y: usize, from: usize, to: usize| -> usize {
        (from..to)
            .filter(|x| is_ink(shot.at(*x, y), track, surface))
            .count()
    };
    let left = RING_X0;
    let right = RING_X0 + DIAMETER;

    // 环带 = 第一个窗里「有墨」的**连续行组**中、高度 ≈40 的那一组。
    //
    // 走过的两条弯路都实测过：
    // ①「每行墨点 4..20」→ 圆顶那一行笔画横跨 ~20~28 px，被排除，带子切成 32 行；
    // ②「左右两侧都有墨」→ 进度条那一行也满足（通栏），会认成 4 行高的假带。
    // 连续行组 + 高度窗口同时把「进度条」（4~5 行）与「正文行」排除在外。
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    for y in 0..shot.height {
        match (ink_count(y, left, right) > 0, current) {
            (true, None) => current = Some((y, y)),
            (true, Some((top, _))) => current = Some((top, y)),
            (false, Some(run)) => {
                runs.push(run);
                current = None;
            }
            (false, None) => {}
        }
    }
    if let Some(run) = current {
        runs.push(run);
    }
    let (top, bottom) = runs
        .iter()
        .copied()
        .find(|(top, bottom)| (DIAMETER - 6..=DIAMETER + 6).contains(&(bottom - top + 1)))
        .unwrap_or_else(|| panic!("没找到高约 {DIAMETER} 的环形带；窗口 0 的连续行组：{runs:?}"));

    let mut report = Vec::new();
    for index in 0..RINGS {
        let left = RING_X0 + RING_PITCH * index;
        let right = left + DIAMETER - 1;
        let mut ink: Option<BBox> = None;
        for y in top..=bottom {
            for x in left..=right {
                let pixel = shot.at(x, y);
                if is_ink(pixel, active, surface) || is_ink(pixel, track, surface) {
                    match &mut ink {
                        Some(found) => found.include(x, y),
                        None => {
                            ink = Some(BBox {
                                left: x,
                                right: x,
                                top: y,
                                bottom: y,
                            })
                        }
                    }
                }
            }
        }
        let ink = ink.unwrap_or_else(|| {
            panic!("第 {index} 个窗（x {left}..{right}）里没有任何环墨迹：环丢了")
        });
        assert!(
            (DIAMETER - 2..=DIAMETER + 2).contains(&ink.width())
                && (DIAMETER - 2..=DIAMETER + 2).contains(&ink.height()),
            "第 {index} 个窗（x {left}..{right}）里的环应 {DIAMETER}×{DIAMETER}(±2)，实测 {}×{}",
            ink.width(),
            ink.height()
        );
        assert!(
            ink.left >= left && ink.right <= right,
            "第 {index} 个窗的墨迹越界：{}..{} 不在 {left}..{right}",
            ink.left,
            ink.right
        );
        report.push(format!("{index}:{}×{}", ink.width(), ink.height()));
    }

    // 激活色（弧）必须出现在 1.. 号窗里（0% 那格本来就只有轨道）。
    for index in 1..RINGS {
        let left = RING_X0 + RING_PITCH * index;
        let count: usize = (top..=bottom)
            .map(|y| {
                (left..left + DIAMETER)
                    .filter(|x| is_ink(shot.at(*x, y), active, surface))
                    .count()
            })
            .sum();
        assert!(
            count > 40,
            "第 {index} 个窗（x {left}..）的激活色弧只有 {count} px：环在但弧没画"
        );
    }

    let clusters = blobs(shot, active, 10);
    eprintln!(
        "[feedback-probe] circular：固定窗 {RINGS} 个逐窗 [{ }]（y {top}..{bottom}）· \
         主色连通簇 {} 个（仅打印，判据是窗）",
        report.join(" / "),
        clusters.len()
    );
    let _ = dims(&clusters);
}
