//! `ULL_GALLERY_SECTION=divider` 的两条判据（M456 从 `gallery/probe.rs` 拆出：那个文件正好 400 行、零余量）。
//!
//! ## 判据是什么
//!
//! ① **线是几条、每条摊几行**：§22 这一屏正好两条线（通栏 + 列表内缩），每条 1px 最多摊 2 行。
//! ② **每条线的墨量覆盖率 ≈ 1.0**：一条 1dp 的线竖着摊开的覆盖率之和应 ≈ 1.00。
//!
//! ## M456：② 为什么曾在 100% 上量到 0.83
//!
//! 旧口径用「像素落在底色的 ±10 容差内 ⇒ 算底色」去找每行的墨迹段，再用「最长连续非底色段 > 900」
//! 筛「线行」。而 1px 线退出 `crisp` 吸附之后（M454 的 `divider::hairline`）在 100% 上摊成
//! `0.09 + 0.88`：9% 那一行每通道只偏离底色 ~2，落在 ±10 里 ⇒ **整行被当成底色丢掉**。
//! 一条真值 0.97 的线只量到 0.88，两条合计 1.65 / 2 = **0.83**（M454 实测），离下限 0.80 只剩 0.03。
//!
//! 现在分两层，**只改了第二层**：
//!
//! - **锚**（哪些行是线）：口径不变，仍是「每通道 ±10 判底色 + 最长段 > 900」。这一层刻意不动——
//!   把判底色的容差收小，会让「与底色只差 1/255 的整片区域」变成线行。
//! - **摊**（这条线到底多重）：从锚行往上下各走，只要**锚跨度上每一个像素**都朝线色移动过
//!   （`delta_ratio > 0`），就把该行算进同一条线。幽灵行是「整段被亚像素覆盖」，所以它整段都动了；
//!   文字行不可能做到「整段都动」（笔画之间总有精确等于底色的像素），因此不会被误收。
//!
//! ## 抓得住 / 抓不住（写清楚，免得下一个人以为它是万能的）
//!
//! | | |
//! |---|---|
//! | 抓得住 | 幽灵行被整行丢掉（0.83 → 0.96）；线变 2px（覆盖率 ≈2.0 越界）；线摊成 3 行（每条 1..=2 行越界）；线断成两段（最长段 < 900 ⇒ 该行不算线 ⇒ 条数不对） |
//! | 抓不住 | 整条线都落在半像素上、**没有任何一行满覆盖**时锚定不到（红在「条数」而不是「覆盖率」）；线色换了角色（本判据按 `OutlineVariant` 算满档，换角色只会让覆盖率偏低，不会被判成「画粗了」） |

use unlucky_design::color::Role;

use crate::theme::Ui;

use super::super::scan::{matches, rgb, Shot};

/// 「这条线有多重」的判据下限 / 上限（**不许为了让测试变绿而放宽**，M456 只改口径不改区间）。
const COVERAGE_MIN: f32 = 0.8;
const COVERAGE_MAX: f32 = 1.3;
/// 一条 1px 线最多摊几行（1 行实色 + 1 行亚像素幽灵）。
const MAX_ROWS_PER_LINE: usize = 2;

/// `ULL_GALLERY_SECTION=divider` 的两条判据。
pub(super) fn check(shot: &Shot, ui: &Ui) {
    let surface = rgb(ui.color(Role::Surface));
    let line = rgb(ui.color(Role::OutlineVariant));

    // ① 锚：旧口径不动（±10 判底色 + 最长段 > 900）
    let anchors: Vec<(usize, usize, usize)> = (0..shot.height)
        .filter_map(|y| {
            let (x0, x1) = background_run(shot, y, surface, 900)?;
            Some((y, x0, x1))
        })
        .collect();
    // ② 摊：把每条线的幽灵行并进来（`anchors` 里相邻的行属于同一条线）
    let mut bands: Vec<Vec<(usize, usize, usize)>> = Vec::new();
    for anchor in anchors {
        match bands.last_mut() {
            Some(band) if band.last().is_some_and(|last| last.0 + 1 == anchor.0) => {
                band.push(anchor)
            }
            _ => bands.push(vec![anchor]),
        }
    }
    for band in &mut bands {
        let (_, x0, x1) = band[0];
        let top = band[0].0;
        let bottom = band.last().map_or(top, |last| last.0);
        if let Some(ghost) = ghost_row(shot, top.checked_sub(1), x0, x1, surface, line) {
            band.insert(0, (ghost.0, x0, x1));
        }
        if let Some(ghost) = ghost_row(shot, Some(bottom + 1), x0, x1, surface, line) {
            band.push((ghost.0, x0, x1));
        }
    }

    assert_eq!(
        bands.len(),
        2,
        "§22 这一屏应正好 2 条分隔线（通栏 + 列表内缩），实测 {} 条：{bands:?}——\
         线断成两段、多出一条、或整条线都落在半像素上（锚定不到满覆盖行）都会落在这里",
        bands.len()
    );
    for (index, band) in bands.iter().enumerate() {
        assert!(
            band.len() <= MAX_ROWS_PER_LINE,
            "第 {index} 条线摊了 {} 行（1px 线最多 1 行实色 + 1 行亚像素幽灵）：{band:?}",
            band.len()
        );
    }

    // ③ 覆盖率 = 每条线「各行的平均覆盖率之和」
    let mut per_line = Vec::new();
    for band in &bands {
        let mut total = 0.0;
        for (y, x0, x1) in band {
            total += row_coverage(shot, *y, *x0, *x1, surface, line);
        }
        per_line.push(total);
    }
    for (index, coverage) in per_line.iter().enumerate() {
        assert!(
            (COVERAGE_MIN..=COVERAGE_MAX).contains(coverage),
            "第 {index} 条分隔线的墨量覆盖率应 ≈1.0（= 1dp），实测 {coverage:.2}\
             （1.5dp 会到 ~1.5；影子行被丢掉时 1px 线会掉到 0.83~0.88）"
        );
        // **机制判据**（M456 的核心）：一条 1px 线要么「一行满覆盖」，要么「实色一行 + 幽灵一行」。
        // 「只有 1 行、且这一行没满覆盖」= 亚像素幽灵行没被算进墨量，那正是旧口径的缺陷
        // （±10 判底色把 9% 覆盖的幽灵行整行当成底色）。上面那条 0.8..=1.3 的区间**抓不住**它
        // （0.83 还落在区间里，M454 实测），所以这里直接把机制写成断言。
        assert!(
            bands[index].len() >= 2 || *coverage >= 0.95,
            "第 {index} 条线只摊了 1 行、覆盖率只有 {coverage:.2}——亚像素幽灵行没被算进墨量。\
             口径只要回退成「±10 判底色」，这条 1px 线就会被量成 0.83~0.88"
        );
    }
    eprintln!(
        "[probe] OK section=divider：分隔线 {} 条，逐条覆盖率 {per_line:?}，\
         行数 {:?}",
        bands.len(),
        bands.iter().map(Vec::len).collect::<Vec<_>>()
    );
}

/// 第 `y` 行上**最长的一段「与底色不同」的像素**（`(x0, x1)` 闭区间，长度 ≥ `min_run`）。
///
/// 「与底色不同」= 每通道都落在 `±TOLERANCE` 内算底色（`scan::matches` 的同一口径）。
fn background_run(
    shot: &Shot,
    y: usize,
    surface: [u8; 3],
    min_run: usize,
) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut start = 0;
    let mut len = 0;
    for x in 0..shot.width {
        if matches(shot.at(x, y), surface) {
            len = 0;
        } else {
            if len == 0 {
                start = x;
            }
            len += 1;
            if best.is_none_or(|(a, b)| b - a + 1 < len) {
                best = Some((start, x));
            }
        }
    }
    best.filter(|(a, b)| b - a + 1 >= min_run)
}

/// 一条线正上方 / 正下方那一行若是「整段都被亚像素覆盖」，返回 `(y, 覆盖率)`。
///
/// 判据是**整段都动了**，不是「平均覆盖率够高」：文字行也能有 0.3 的平均覆盖率（笔画占 30%），
/// 但笔画之间必有精确等于底色的像素 ⇒ `delta_ratio == 0`，于是被拒。
fn ghost_row(
    shot: &Shot,
    y: Option<usize>,
    x0: usize,
    x1: usize,
    surface: [u8; 3],
    line: [u8; 3],
) -> Option<(usize, f32)> {
    let y = y.filter(|y| *y < shot.height)?;
    let uniform = (x0..=x1).all(|x| delta_ratio(shot.at(x, y), surface, line) > 0.0);
    uniform.then(|| (y, row_coverage(shot, y, x0, x1, surface, line)))
}

/// 第 `y` 行在 `x0..=x1` 上的平均覆盖率（0..=1；满覆盖的实线 = 1.0）。
fn row_coverage(
    shot: &Shot,
    y: usize,
    x0: usize,
    x1: usize,
    surface: [u8; 3],
    line: [u8; 3],
) -> f32 {
    let mut sum = 0.0;
    for x in x0..=x1 {
        sum += delta_ratio(shot.at(x, y), surface, line);
    }
    sum / (x1 - x0 + 1) as f32
}

/// `pixel` 相对 `surface` 的偏移占「满档 line 色偏移」的比例（0..=1）。
fn delta_ratio(pixel: [u8; 3], surface: [u8; 3], line: [u8; 3]) -> f32 {
    let channel = |i: usize| {
        let full = f32::from(line[i]) - f32::from(surface[i]);
        if full.abs() < 1.0 {
            return 0.0;
        }
        ((f32::from(pixel[i]) - f32::from(surface[i])) / full).clamp(0.0, 1.0)
    };
    let (dr, dg, db) = (channel(0), channel(1), channel(2));
    (dr + dg + db) / 3.0
}

#[cfg(test)]
mod tests {
    //! 口径本身的判据：只证明「幽灵行在覆盖率口径下仍然『动过』、而在旧的底色口径下被判成底色」。
    //! 整图的两条判据（条数 / 覆盖率）跑在真截图上，由 `gallery::probe` 的主测试调 [`super::check`]。

    use super::*;

    const SURFACE: [u8; 3] = [246, 250, 255];
    const LINE: [u8; 3] = [200, 212, 224];

    /// 一个「被垂直覆盖了 `fraction`」的像素（0 = 纯底色，1 = 满档线色）。
    fn pixel(fraction: f32) -> [u8; 3] {
        let mut pixel = [0u8; 3];
        for i in 0..3 {
            let full = f32::from(LINE[i]) - f32::from(SURFACE[i]);
            pixel[i] = (f32::from(SURFACE[i]) + full * fraction).round() as u8;
        }
        pixel
    }

    /// 9% 的幽灵行：`delta_ratio` 看得出「动过」，旧的 ±10 底色容差看不出——这就是 0.83 的成因。
    #[test]
    fn delta_ratio_keeps_sub_pixel_ghosts() {
        let ghost = pixel(0.09);
        assert!(
            delta_ratio(ghost, SURFACE, LINE) > 0.0,
            "9% 的幽灵行必须仍算「动过」，否则幽灵行会被整行丢掉（覆盖率 0.97 → 0.88）"
        );
        assert!(
            matches(ghost, SURFACE),
            "前提：幽灵行落在 ±10 的底色容差里（旧口径正是这么把它丢掉的，M454 实测 1.00 → 0.83）"
        );
        // 两条一起才是「口径改对了」：锚定那一层不变，摊薄那一层把它算进来。
        assert!((delta_ratio(pixel(1.0), SURFACE, LINE) - 1.0).abs() < 0.01);
        assert_eq!(delta_ratio(SURFACE, SURFACE, LINE), 0.0);
    }
}
