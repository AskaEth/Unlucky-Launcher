//! 选项区 × 三栏的**像素**验收（M123 ①）：真 PNG 上的几何断言。
//!
//! ## 为什么还要一条像素测试
//!
//! [`super::options_screen`] 断的是「喂给树的模型 + 裁决出来的数字」，它抓不住**画出来**：
//! 两栏是不是真同屏、栏宽有没有按裁决落地、版本行有没有被压成一条缝、确认键是不是还在屏内。
//! 那些只有图能回答——所以这里直接量 `pages_demo` 出的 PNG，按**产品自己的裁决函数**
//! （[`VersionRails::of_metrics`] + [`frame_metrics`]）推出每一栏该在的 x 区间，再要求像素同意。
//!
//! ## 开关与跑法（`ULL_DL_*_SHOT` 喂图，四条出图命令见 `feed.rs` 的模块文档）
//!
//! 四条断言都靠环境变量拿到一张真 PNG。M127 之前的写法是「没有图就 SKIP」，于是在 CI 里
//! **全是空跑**（`.github/` 里 grep 这些变量零命中）。现在按环境分档：
//!
//! - **只在 Windows runner 上跑**：断言量的行高/墨迹条数/药丸位置是按本仓字体栈
//!   （`Segoe UI`）标定的，Linux/macOS 的回退字体会让这些数字变样 ⇒ 假红。这条限制同时写在
//!   `.github/workflows/ci.yml` 的作业条件（`if: runner.os == 'Windows'`，出图与断言两步都带）
//!   和 `feed.rs` 的模块文档里，不靠「碰巧只配了一处」。
//! - **CI 里没喂图 = 失败**，提示「守门未喂图」：将来谁把 CI 里出图那步重构掉，构建会红，
//!   而不是静静退回手动档。本机（非 CI）才允许跳过，并且会留一句提示。
//!
//! 颜色**不**写死：`run_at` 只要求「同一段颜色连续」，栏与栏的边界按「颜色变了」判，
//! 于是浅色/深色都能跑同一条断言（实测浅色下 Card=(235,245,255)、Canvas=(246,250,255)）。

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;

use crate::pages::layout::rails::RAIL_GAP;
use crate::pages::layout::{available_width, frame_metrics, PaneKit, VersionRails};
use crate::theme::{Families, Ui};

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 1180 宽那张（`01-options-fabric-en-light.png`）。
const WIDE_SHOT: &str = "ULL_DL_OPTIONS_SHOT";
/// 700 宽那张（`02-options-fabric-en-light-chips700.png`）——chips 档回归守门用。
const CHIPS_SHOT: &str = "ULL_DL_CHIPS_SHOT";
/// 736 宽那张（`04-options-fabric-en-light-compact736.png`）——Compact 档主版本栏用。
const COMPACT_SHOT: &str = "ULL_DL_COMPACT_SHOT";
/// 700 宽、**不设 `ULL_DL_DEMO`**（= `ready`，还没选版本）那张——r1 复审 P2-1 的守门用。
const READY_SHOT: &str = "ULL_DL_READY_SHOT";

fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

/// 一张 RGB 图（只留本文件要用的那两件事：宽高 + 取点）。
struct Shot {
    width: usize,
    height: usize,
    pixels: Vec<[u8; 3]>,
}

impl Shot {
    fn load(path: &std::path::Path) -> Self {
        let bytes = std::fs::read(path).expect("读截图");
        let image = image::load_from_memory(&bytes).expect("解 PNG").to_rgb8();
        let (width, height) = (image.width() as usize, image.height() as usize);
        Self {
            width,
            height,
            pixels: image.pixels().map(|pixel| pixel.0).collect(),
        }
    }

    fn at(&self, x: usize, y: usize) -> [u8; 3] {
        self.pixels[y * self.width + x]
    }

    /// `(起点, 长度)`：`(x, y)` 所在的那一段**同色**横条。
    fn run_at(&self, x: usize, y: usize) -> (usize, usize) {
        let color = self.at(x, y);
        let mut start = x;
        while start > 0 && self.at(start - 1, y) == color {
            start -= 1;
        }
        let mut end = x;
        while end + 1 < self.width && self.at(end + 1, y) == color {
            end += 1;
        }
        (start, end - start + 1)
    }

    /// 在 `x` 这一竖列上，`[from, to)` 里**第一段**与 `background` 不同的连续区：`(top, height)`。
    ///
    /// 用它量「第一行版本行有多高」：选中行有底色，行外是页面底色，两段一夹就是行高。
    fn first_filled_band(
        &self,
        x: usize,
        from: usize,
        to: usize,
        background: [u8; 3],
    ) -> Option<(usize, usize)> {
        let filled = |y: usize| self.at(x, y) != background;
        let top = (from..to.min(self.height)).find(|y| filled(*y))?;
        let mut bottom = top;
        while bottom + 1 < to.min(self.height) && filled(bottom + 1) {
            bottom += 1;
        }
        Some((top, bottom - top + 1))
    }

    /// `[from, to)` 里最长的一段**同色**横条：`(y, 长度)`（颜色必须与底色不同）。
    ///
    /// 用它找面板里那条实心「安装」按钮：它是列表列里唯一一条 ≥ 80px 的实色带。
    fn longest_run(
        &self,
        left: usize,
        right: usize,
        from: usize,
        to: usize,
        background: [u8; 3],
    ) -> (usize, usize) {
        let mut best = (0, 0);
        for y in from..to.min(self.height) {
            let mut run = 0;
            let mut color = background;
            for x in left..right {
                let pixel = self.at(x, y);
                if pixel == background {
                    color = background;
                    run = 0;
                } else if pixel == color {
                    run += 1;
                } else {
                    color = pixel;
                    run = 1;
                }
                if run > best.1 {
                    best = (y, run);
                }
            }
        }
        best
    }

    /// 列表列里最后一行**有内容**的 y（`None` = 整列都是底色）。
    fn last_ink_row(&self, left: usize, right: usize, background: [u8; 3]) -> Option<usize> {
        (0..self.height).rev().find(|y| {
            (left..right).any(|x| {
                let pixel = self.at(x, *y);
                pixel
                    .iter()
                    .zip(background)
                    .map(|(a, b)| u32::from(a.abs_diff(b)))
                    .sum::<u32>()
                    > 12
            })
        })
    }

    /// `y` 这一行上、含 `x` 的那段**连续非底色**区：`(起点, 长度)`。
    ///
    /// 与 [`Shot::run_at`] 的区别：颜色可以变（药丸底色 + 里面的图标墨迹算同一块）。
    fn fill_run(
        &self,
        y: usize,
        left: usize,
        right: usize,
        x: usize,
        background: [u8; 3],
    ) -> (usize, usize) {
        let filled = |x: usize| self.at(x, y) != background;
        let mut start = x;
        while start > left && filled(start - 1) {
            start -= 1;
        }
        let mut end = x;
        while end + 1 < right && filled(end + 1) {
            end += 1;
        }
        (start, end - start + 1)
    }

    /// `[from, to)` 里在 `[left, right)` 有墨迹的**条数**（两条之间空 ≥ 8 行才算两条）。
    ///
    /// 用它数「读得出几个版本号」：一行版本号的文本墨迹约 11px 高、行距 44，中间空 30+ ⇒
    /// 一条对应一行。被压扁时整块只剩一条。
    fn ink_bands(
        &self,
        left: usize,
        right: usize,
        from: usize,
        to: usize,
        background: [u8; 3],
    ) -> usize {
        let mut bands = 0;
        let mut gap = usize::MAX;
        for y in from..to.min(self.height) {
            let inked = (left..right).any(|x| self.at(x, y) != background);
            if inked {
                if gap >= 8 {
                    bands += 1;
                }
                gap = 0;
            } else {
                gap = gap.saturating_add(1);
            }
        }
        bands
    }

    /// 列表列里、`from` 起的**第一条薄通栏横线**的 y（`divider::full` 画的那条 1px 线）。
    ///
    /// 用它量「版本列表到哪儿为止」：尾部面板上沿有一条通栏分隔线，它上面的高度就是列表拿到的。
    /// 只认**薄**线（上下两行都与它不同色）——选中行的底色带也是通栏的，那是 44px 的块，不是线。
    fn first_rule_y(
        &self,
        left: usize,
        right: usize,
        from: usize,
        background: [u8; 3],
    ) -> Option<usize> {
        let thin = |y: usize| {
            let color = self.at(left + 2, y);
            color != background
                && (left + 2..right.saturating_sub(2)).all(|x| self.at(x, y) == color)
                && (y == 0 || self.at(left + 2, y - 1) != color)
                && (y + 1 >= self.height || self.at(left + 2, y + 1) != color)
        };
        (from..self.height).find(|y| thin(*y))
    }
}

/// 三栏的 x 区间，全部从**产品自己的裁决函数**推出来（不是手抄的像素坐标）。
struct Bands {
    type_rail: (usize, f32),
    major_rail: (usize, f32),
    list: (usize, f32),
}

fn bands(width: f32) -> Bands {
    let ui = ui();
    let rails = VersionRails::of_metrics(frame_metrics(PaneKit::Single, width));
    // 可用宽 = 窗口 − 导航轨 − 两侧页边距 ⇒ 页边距反推出来（只有左侧那一份进左缘）
    let avail = available_width(width);
    let gutter = (width - avail - ui.metrics().nav_rail_width) / 2.0;
    assert!(
        (gutter - 24.0).abs() < 0.01,
        "页边距应当是一格 24：{gutter}"
    );
    let left = (width - avail - gutter).round() as usize;
    assert_eq!(
        left,
        (ui.metrics().nav_rail_width + gutter).round() as usize,
        "页左缘 = 导航轨 + 一格页边距"
    );
    assert_eq!(left, 112, "harness 的导航轨 88 + 页边距 24");
    let type_rail = (left, rails.type_rail);
    let major_rail = (
        left + rails.type_rail as usize + RAIL_GAP as usize,
        rails.major_rail,
    );
    // chips 档没有两条侧栏（宽度都是 0）：列表就是整条可用宽，左缘 = 页左缘。
    // 不分支的话会算成 `112 + 0 + 24 + 0 + 24 = 160`，右缘跟着冲出画面 22px
    // （M125 缺陷 1 的守门就是在这里踩到「扫描越界读下一行像素」的）。
    let list = if rails.tier.is_three_column() {
        (
            major_rail.0 + rails.major_rail as usize + RAIL_GAP as usize,
            rails.list,
        )
    } else {
        (left, rails.list)
    };
    Bands {
        type_rail,
        major_rail,
        list,
    }
}

/// `01-options-fabric-en-light.png`（1180×760）上的四条断言。
///
/// 抓得住：①两条侧栏**同时**在同一条扫描线上画出来（不是只有一条、也不是其中一条塌成 0 宽）；
/// ②两条栏各自的像素宽度 = `VersionRails` 裁决出来的 140/140，栏间 24 的缝真的空着（栏没有重叠）；
/// ③版本行至少有一行**完整**（44 = `row_height`），即列表没有被面板压成「缝」；
/// ④选项区的确认键与「实例名」都在屏内（列表列最下面一行墨迹离窗口底边还有余量）。
///
/// 抓不住：文本有没有被裁（那要看字形宽度，见报告「没验的部分」）、hover/tooltip 层（静态图拍不到）、
/// 深色主题下的对比度（本条只跑浅色图；深色另用同一 argv 换 `--screenshot-theme` 出图人看）。
#[test]
fn an_options_shot_shows_both_rails_and_the_whole_panel() {
    let Some(path) = feed(WIDE_SHOT) else {
        return;
    };
    let shot = Shot::load(&path);
    assert_eq!(
        (shot.width, shot.height),
        (1180, 760),
        "验收图必须 1180×760"
    );

    let bands = bands(1180.0);
    let ui = ui();
    let y = 400;

    // ① 两条栏同时画在同一条扫描线上，且宽度 = 裁决值（像素边界一条不差）
    for (name, (start, width)) in [("类型栏", bands.type_rail), ("主版本栏", bands.major_rail)]
    {
        let (run_start, run_len) = shot.run_at(start + 8, y);
        assert_eq!(
            (run_start, run_len as f32),
            (start, width),
            "{name}：像素区间与 `VersionRails` 裁决不一致（y={y}）"
        );
        assert_ne!(
            shot.at(start, y),
            shot.at(start.saturating_sub(1), y),
            "{name}：左缘没有边界"
        );
    }
    // 栏间 24 的缝真的空着（两条栏没有叠在一起），且列表列不是面板底色
    let gap_x = bands.type_rail.0 + bands.type_rail.1 as usize + RAIL_GAP as usize / 2;
    assert_eq!(
        shot.at(gap_x, y),
        shot.at(gap_x - 1, y),
        "两栏之间的缝被填掉了（栏重叠或间距丢失）"
    );
    assert_ne!(
        shot.at(gap_x, y),
        shot.at(bands.type_rail.0, y),
        "缝与栏同色 ⇒ 读不出「两条栏」"
    );
    assert_ne!(
        shot.at(bands.list.0 + 8, y),
        shot.at(bands.major_rail.0, y),
        "列表列被侧栏底色盖住了"
    );

    // ③ 完整的一行版本行：第一行（选中行）的底色带必须正好一行高（`row_height` = 44）
    let canvas = shot.at(bands.list.0 + 2, 12);
    let (_, row_height) = shot
        .first_filled_band(bands.list.0 + 60, 235, 520, canvas)
        .expect("列表列里应当有版本行");
    assert_eq!(
        row_height as f32,
        ui.metrics().row_height,
        "版本行被裁/被压：实测可视行高 {row_height}，应为 {}",
        ui.metrics().row_height
    );

    // ④ 选项区整块在屏内：列表列最下面的墨迹离底边还有余量（被切的话它会贴到 759）
    let last = shot
        .last_ink_row(
            bands.list.0,
            bands.list.0 + bands.list.1 as usize - 2,
            canvas,
        )
        .expect("列表列里应当有内容");
    assert!(
        last + 8 < shot.height,
        "选项区被窗口底边切掉：最后一行墨迹在 y={last}（图高 {}）",
        shot.height
    );
    assert!(
        last > 600,
        "选项区不在屏上：最后一行墨迹在 y={last}，解析应当有「实例名」那一格"
    );
}

// ---- chips 档（缺陷 1）与 Compact 档（缺陷 2）的像素守门（拆出来守 400 行红线）

mod chips;

mod compact;

mod feed;

mod ready;

use feed::feed;
