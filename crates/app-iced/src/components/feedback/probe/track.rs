//! 线性进度**轨道本身**的判据（§12.1：厚度 / 宽度 / 圆角）。
//!
//! 从 `probe.rs` 拆出来（M248 r1 复审时那边正好 401 行，撞 400 红线）：顺着 `circular` /
//! `panels` 的同一先例，一族几何一个文件。
//!
//! ## 取带规则（M248 r1 P2-1 / P2-2 的教训）
//!
//! 轨道的配色从强调色族换成中性容器面（`SurfaceContainerHighest`）之后，「画布 → 轨道」这条
//! 混色轴正好穿过中性文字：按**计数**判（`bands`，一行里有多少个覆盖率 ≥50% 的像素）会把
//! 图注行也数成轨道（浅色板实测多出 3 条假带：6/2/6 行）。所以这里改成按**连续段**判
//! （`pixel::bands_by_run`）。
//!
//! 复审判过这条改动：「换判据」的方向对，但第一版把阈值定成 200，**顺手丢了半个守卫**——
//! 把 after 图里 0% 那条轨道右半截涂回画布色（连续段 160px = 轨道被宿主裁短的形态）之后，
//! 那一行直接不再成带，`check_linear` 靠另一条带照样绿。现在分两条判据：
//!
//! | 判据 | 常量 | 决定什么 | 实测口径（两套主题一致） |
//! |---|---|---|---|
//! | 成带 | [`TRACK_MIN_RUN`] = **100** | 这一行算不算轨道行 | 确定态行最长段 318~320、不定态行 306~308、文字行 **22** |
//! | 宽度 | [`TRACK_MIN_WIDTH`] = **300** | 这条轨道的宽度对不对 | 宿主 320/360，砍半（160）必须判不合格 |
//!
//! ⚠️ **别按「单条轨道」心算阈值**：`bands_by_run` 是按**整行**取最长连续段（三个宿主在同一
//! y 上，取其中最长那一段），不是按单条 320 轨道取。按单条轨道算会得到 ~88px 的错值，
//! 于是把阈值定到 200，160px 的窄轨道就整行掉出判据——复审就是这么抓到第一版的漏洞的。
//!
//! 取值区间由 `tests/bands.rs` 的合成图钉住（成带阈值调大到 >160、或小到 ≤22，那里都红）。

use unlucky_design::color::Role;

use super::super::pixel::{band_width, bands_by_run, matches, near, rgb, runs, Shot};
use crate::theme::Ui;

/// 认定为「轨道行」所需的**连续**同色段长度。
///
/// `pub(crate)`：`tests/bands.rs` 的合成图要断言这两个阈值本身（`probe` 是 `#[cfg(test)]` 模块）。
pub(crate) const TRACK_MIN_RUN: usize = 100;

/// 轨道必须占满宿主带来的宽度（线性进度宿主 320/360，下限取 300）。
pub(crate) const TRACK_MIN_WIDTH: usize = 300;

/// `check` 的**取带规则**（`pub(crate)` 是为了让 `tests/bands.rs` 的合成图跑同一段逻辑）。
///
/// 抽函数而不是让用例直接调 `bands_by_run`：用例要能在「调用点被改回计数判据」时变红，
/// 那就得覆盖调用点本身。
pub(crate) fn track_bands(
    shot: &Shot,
    track: iced::Color,
    surface: iced::Color,
) -> Vec<(usize, usize)> {
    bands_by_run(shot, track, surface, TRACK_MIN_RUN)
}

/// §12.1：线性轨道的厚度（4 行）、宽度（占满宿主）、圆角（full）。
///
/// 参考值写字面量（A1）：**不能**从 `LinearGeometry` 读回期望值——那样把 4 改成 6 之后
/// 「实测 6 == 期望 6」会照样绿，等于没有断言。轨道的**角色**同理，由调用方给字面角色
/// （`SurfaceContainerHighest`），不从 `progress::progress_roles()` 读回。
pub(super) fn check(shot: &Shot, ui: &Ui) {
    let surface = ui.color(Role::Surface);
    let track = ui.color(Role::SurfaceContainerHighest);

    let bar_bands = track_bands(shot, track, surface);
    assert!(
        bar_bands.len() >= 2,
        "线性进度应有两条轨道带（确定态 + 不定态），实测 {bar_bands:?}：\
         某一条整行掉出判据 = 它被裁到 <{TRACK_MIN_RUN}px 连不成段了"
    );
    for (top, bottom) in &bar_bands {
        let height = bottom - top + 1;
        assert!(
            near(height, 4),
            "12.1：线性进度厚必须 4（A1 Height；实测 {height} @ 行 {top}）"
        );
        // 宽度**单独**判（M248 r1 P2-1）：阈值 100 会把被裁短的轨道也认成一条带，
        // 所以「窄了」不能再靠「找不到带」兜——复审反例（砍到 160px）走的就是这条路。
        let widest = band_width(shot, (*top, *bottom), track, surface);
        assert!(
            widest >= TRACK_MIN_WIDTH,
            "12.1：轨道必须占满宿主（≥{TRACK_MIN_WIDTH}px 的连续段），实测最宽段 {widest}px @ 行 {top}"
        );
    }

    // 圆角 full：条的四角必须透出底色（直角会把角填满）。
    let (top, _) = bar_bands[0];
    let (left, len) = runs(shot, top + 1, track, surface)
        .into_iter()
        .find(|(_, len)| *len >= TRACK_MIN_WIDTH)
        .expect("上面刚判过宽度 ≥300，这一行必须能找到长段");
    assert!(
        !matches(shot.at(left, top), rgb(track)),
        "12.1：圆角 full(2) 的左上角必须透出底色（直角会填满）"
    );
    assert!(
        matches(shot.at(left + len / 2, top + 1), rgb(track)),
        "条的中间必须有墨（否则是没画出来，不是圆角）"
    );

    // 覆盖面账（复审可对 `--nocapture` 输出核）：两条带**都在**厚度/宽度判据里；
    // 不定态那条 5 行是含 1 行抗锯齿（`near(·, 4)` 容 ±1）。12.4 只量主色两端点，不量厚度。
    eprintln!(
        "[feedback-probe] 轨道带 {} 条（高 {:?}）· 最宽段 {:?}",
        bar_bands.len(),
        bar_bands
            .iter()
            .map(|(top, bottom)| bottom - top + 1)
            .collect::<Vec<_>>(),
        bar_bands
            .iter()
            .map(|band| band_width(shot, *band, track, surface))
            .collect::<Vec<_>>(),
    );
}
