//! **阴影免疫**的两条像素判据：逐位相等版的 `bands` / `band_width`（M430 r1 P2-2）。
//!
//! ## 为什么不能复用 [`super::super::pixel::bands`]
//!
//! 那边按 [`coverage`](super::super::pixel::coverage) `>= 0.5` 判墨，而覆盖率是把像素投影到
//! 「`under` → `fill`」这条混色轴上、再把比例**夹到 1.0** —— 比 `fill` 更暗的像素（浮层阴影正是）
//! 因此也算「满填充」。两处实测：
//!
//! | 段 | 现象 |
//! |---|---|
//! | Snackbar | 条顶上那一行阴影 `(142,143,145)` 的投影比例 0.53 ⇒ 52 高的条量成 **53**，接着「框顶到条顶」差 1 |
//! | Dialog | 卡色与画布只差 21/通道，阴影缓坡里「离卡色 ≤10」的一圈横贯整行 ⇒ 卡带粘连（`[(162,534),(660,759)]`） |
//!
//! Dialog 段的修法在 `panels::check_dialog` 里就地做（它本来就在自己数行与列剖面）；这里放的是
//! Snackbar 段要的那两条。
//!
//! ## 代价：iso-brightness 等值线
//!
//! 阴影是一段渐变的亮度缓坡，坡上**恰好**等于填充色的那一圈会留下 1~2 行的假带。逐位相等消掉了
//! 99% 的干扰，但那一条等值线还在，所以调用方必须按「带高」滤（`check_snackbar` 的「带高 ≥40」、
//! `panels::check_dialog` 的 `MIN_CARD_BAND`）。
//!
//! ## 适用范围
//!
//! 只用于**实色填充的矩形**（提示条 / 卡片）。渐变面、半透明面仍然要用 [`super::super::pixel::bands`]。

use iced::Color;

use super::super::pixel::{rgb, Shot};

/// 逐位相等版的 `bands`：连续行里「恰好等于 `fill` 的像素 ≥ `min_ink`」→ 聚成竖带。
pub(super) fn bands_exact(shot: &Shot, fill: Color, min_ink: usize) -> Vec<(usize, usize)> {
    let ink = rgb(fill);
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut current: Option<usize> = None;
    for y in 0..shot.height {
        let hit = (0..shot.width).filter(|x| shot.at(*x, y) == ink).count() >= min_ink;
        match (hit, current) {
            (true, None) => current = Some(y),
            (false, Some(top)) => {
                out.push((top, y - 1));
                current = None;
            }
            _ => {}
        }
    }
    if let Some(top) = current {
        out.push((top, shot.height - 1));
    }
    out
}

/// 逐位相等版的 `band_width`：竖带里「最长连通段」的最大值（阴影免疫）。
pub(super) fn band_width_exact(shot: &Shot, band: (usize, usize), fill: Color) -> usize {
    let ink = rgb(fill);
    (band.0..=band.1)
        .map(|y| {
            let (mut run, mut best) = (0usize, 0usize);
            for x in 0..shot.width {
                if shot.at(x, y) == ink {
                    run += 1;
                    best = best.max(run);
                } else {
                    run = 0;
                }
            }
            best
        })
        .max()
        .unwrap_or(0)
}
