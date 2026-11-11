//! `probe` 的读图器（`probe.rs` 拆出来的：单文件 ≤400 行，AGENTS.md §3-1）。
//!
//! 只做三件事：读 PNG、找同色的连通块（`BBox`）、量一条横带里某颜色的墨迹范围
//! 与「一条细线」。判据在 `probe.rs` / `probe/divider.rs` / `probe/long.rs`。
//!
//! ## 已知边界（分隔线守卫，写在这里供以后改的人查）
//!
//! 1. **≤25% 覆盖的淡线找不到** → 表现是 **loud red**（`find_thin_line` 返回 `None`，
//!    `divider::check` 直接 panic「锚定失败」），**不是假绿**。原因：`find_thin_line` 的
//!    `TOL=6` 是单通道可辨差下限，40% 覆盖行实测 `(9,6,4)` 刚好在线上，25% 覆盖行只有
//!    `(6,4,2)`，会落到判据外——但相邻那行是满覆盖（`(25,17,10)`），所以真实渲染的画法下
//!    行锚定不会丢；只有「整条线都画在半像素上」才会既找不到又看起来像没画。
//! 2. **身份校验绑定了 `SurfaceVariant`**：`divider::check` 用
//!    `Surface → SurfaceVariant` 的插值做身份校验。若哪天 Secondary 分隔线换了角色色
//!    （比如改回 `OutlineVariant`），这里会 loud red——届时改 `divider.rs` 里那一行 `Role`。
//! 3. **只认「一条横线」**：竖向分隔线（本项目不用）与圆角/虚线画法不在判据内。
//! 4. **深浅两档**：浅色图必跑；深色图存在就跑（`tabs-dark.png`），深色 divider 对比更低
//!    （实测线色 `(46,60,70)`，与深色 Surface 的差比浅色档小），更值得测。

use std::path::Path;

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;

use crate::theme::{Families, Ui};

/// 字体族（与 `gallery::probe` 同一套）。
pub(super) const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 颜色比对容差。
///
/// **3 而不是 10**：本家族要区分的几个容器色本身很接近（light 下
/// `SurfaceContainerLow (235,245,255)` vs `SurfaceContainer (227,239,251)` 只差 8/6/4），
/// 容差 10 会把「取景板」和「滚动后的 AppBar」连成一块，量出来的高度就不对了。
/// 实色区域是精确值，3 足够吸收抗锯齿的取整误差。
pub(super) const TOLERANCE: u8 = 3;

pub(super) type Rgb = [u8; 3];

pub(super) fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

pub(super) fn rgb(color: iced::Color) -> Rgb {
    [
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
    ]
}

pub(super) fn matches(pixel: Rgb, expected: Rgb) -> bool {
    // **每个通道**都在容差内；`differs` 是它的否定（任一通道超差即「不同」）
    !differs(pixel, expected, TOLERANCE)
}

/// 两个颜色在**每个通道**上都不超过 `tol`。
pub(super) fn differs(pixel: Rgb, expected: Rgb, tol: u8) -> bool {
    pixel.iter().zip(expected).any(|(a, b)| a.abs_diff(b) > tol)
}

/// 一个外接框（闭区间）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BBox {
    pub(super) left: usize,
    pub(super) right: usize,
    pub(super) top: usize,
    pub(super) bottom: usize,
}

impl BBox {
    pub(super) fn width(&self) -> usize {
        self.right - self.left + 1
    }

    pub(super) fn height(&self) -> usize {
        self.bottom - self.top + 1
    }
}

/// 一张图的像素访问器。
pub(super) struct Shot {
    pub(super) width: usize,
    pub(super) height: usize,
    pixels: Vec<Rgb>,
}

impl Shot {
    pub(super) fn load(path: &Path) -> Self {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("读不动 {path:?}：{e}"));
        let image = image::load_from_memory(&bytes)
            .unwrap_or_else(|e| panic!("解 PNG 失败 {path:?}：{e}"))
            .to_rgb8();
        assert_eq!(
            (image.width() as usize, image.height() as usize),
            (1180, 760),
            "验收图必须是 D30 的 1180×760 @100% DPI"
        );
        Self {
            width: image.width() as usize,
            height: image.height() as usize,
            pixels: image.pixels().map(|p| p.0).collect(),
        }
    }

    pub(super) fn at(&self, x: usize, y: usize) -> Rgb {
        self.pixels[y * self.width + x]
    }

    /// 某种颜色的**全部**连通块（面积 ≥ 24；文字笔画太碎的块直接丢掉）。
    pub(super) fn blocks(&self, color: Rgb) -> Vec<BBox> {
        let mut seen = vec![false; self.width * self.height];
        let mut found = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if seen[y * self.width + x] || !matches(self.at(x, y), color) {
                    continue;
                }
                let mut stack = vec![(x, y)];
                seen[y * self.width + x] = true;
                let mut bbox = BBox {
                    left: x,
                    right: x,
                    top: y,
                    bottom: y,
                };
                while let Some((cx, cy)) = stack.pop() {
                    bbox.left = bbox.left.min(cx);
                    bbox.right = bbox.right.max(cx);
                    bbox.top = bbox.top.min(cy);
                    bbox.bottom = bbox.bottom.max(cy);
                    for (nx, ny) in [
                        (cx + 1, cy),
                        (cx.saturating_sub(1), cy),
                        (cx, cy + 1),
                        (cx, cy.saturating_sub(1)),
                    ] {
                        if nx < self.width
                            && ny < self.height
                            && !seen[ny * self.width + nx]
                            && matches(self.at(nx, ny), color)
                        {
                            seen[ny * self.width + nx] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
                if bbox.width() * bbox.height() >= 24 {
                    found.push(bbox);
                }
            }
        }
        found
    }

    /// 矩形内**最深**的像素（禁用的核心笔画就是精确的混合色，用它判角色最稳）。
    pub(super) fn darkest(&self, bbox: &BBox) -> Option<Rgb> {
        let mut best: Option<(u32, Rgb)> = None;
        for y in bbox.top..=bbox.bottom.min(self.height - 1) {
            for x in bbox.left..=bbox.right.min(self.width - 1) {
                let pixel = self.at(x, y);
                let sum: u32 = pixel.iter().map(|c| u32::from(*c)).sum();
                if best.is_none_or(|(current, _)| sum < current) {
                    best = Some((sum, pixel));
                }
            }
        }
        best.map(|(_, pixel)| pixel)
    }

    /// 矩形里某颜色墨迹的左右范围（量图标/文字位置用）。
    pub(super) fn band_extent(
        &self,
        color: Rgb,
        y0: usize,
        y1: usize,
        x0: usize,
        x1: usize,
    ) -> Option<(usize, usize)> {
        let mut span: Option<(usize, usize)> = None;
        for y in y0..=y1.min(self.height - 1) {
            for x in x0..=x1.min(self.width - 1) {
                if matches(self.at(x, y), color) {
                    span = Some(match span {
                        Some((l, r)) => (l.min(x), r.max(x)),
                        None => (x, x),
                    });
                }
            }
        }
        span
    }

    /// 找「一条**细、低对比、近乎实心**的长横线」，返回 `(y, x0, x1)`：
    /// `y` 是线最实的那一行，`x0..x1` 是**该行整段墨迹的跨度**（不是某一次连续 run 的窗口）。
    ///
    /// 四条判据，每条都对应一次真实的假通过（视觉 r1 P2-2 复核时被点出来的）：
    ///
    /// 1. **跨度 ≥ `min_len`**（200）：`Secondary` 分隔线整行 242px；标题/标签行凑不满，能跳过它们。
    /// 2. **近乎实心**（墨迹数 ≥ 跨度 × 4/5）：逐 tab 分段时中间是真缺口，但只有 1~2px，仍算实心
    ///    ——**必须先能找到这一行**，再由 [`Shot::line_segments`] 把缺口数成多段。
    ///    （早先「凑满 150px 就 return，然后在窗口里数段」的写法会滑过缺口，永远数到 1。）
    /// 3. **薄**（跨中点纵向墨迹 ≤ 3 行）：五态行里的 hover 底色是 40px 高的带，被这条排除
    ///    （早先的写法会把那条 224px 的带选中，量到完全不同的元素）。
    /// 4. **低对比**（线色与底色的单通道最大差 ≤ `max_contrast`）：`Primary` 指示条是高饱和色，被排除。
    ///
    /// `TOL` 的口径：**单通道**最大可辨差，取 6。40% 覆盖的抗锯齿行实测单通道差 `(9,6,4)`，
    /// 6 能看见它、又挡住 ±3 的渲染噪声；覆盖再低的行会落到判据外，但**相邻那行是满覆盖**
    /// （实测线色差 25/17/10），所以行的锚定不会丢。
    pub(super) fn find_thin_line(
        &self,
        background: Rgb,
        expected: Rgb,
        min_len: usize,
        max_contrast: u8,
    ) -> Option<(usize, usize, usize, Rgb)> {
        const TOL: u8 = 6;
        let is_ink = |pixel: Rgb| differs(pixel, background, TOL);
        let contrast = |pixel: Rgb| {
            pixel
                .iter()
                .zip(background)
                .map(|(p, q)| p.abs_diff(q))
                .max()
                .unwrap_or(0)
        };

        for y in 0..self.height {
            let ink: Vec<usize> = (0..self.width).filter(|x| is_ink(self.at(*x, y))).collect();
            let (Some(&first), Some(&last)) = (ink.first(), ink.last()) else {
                continue;
            };
            let len = last - first + 1;
            if len < min_len || ink.len() * 5 < len * 4 {
                continue;
            }
            // 薄：跨中点纵向墨迹 ≤ 3 行
            let mid = (first + last) / 2;
            let mut top = y;
            while top > 0 && is_ink(self.at(mid, top - 1)) {
                top -= 1;
            }
            let mut bottom = y;
            while bottom + 1 < self.height && is_ink(self.at(mid, bottom + 1)) {
                bottom += 1;
            }
            if bottom - top + 1 > 3 {
                continue;
            }
            // 低对比：取这条细带里最实的一行（满覆盖），跨段与线色都按那一行算
            let best = (top..=bottom)
                .max_by_key(|candidate| {
                    (first..=last)
                        .filter(|x| is_ink(self.at(*x, *candidate)))
                        .count()
                })
                .unwrap_or(y);
            let line = self.modal_color(best, first, last);
            let c = contrast(line);
            if c > max_contrast || c <= TOL {
                continue;
            }
            // **身份校验**：线色必须落在 `底色 → 预期角色色` 的插值线上（覆盖率 30%~105%）。
            // 只靠「跨度/薄/低对比」挡不住「更靠上的另一条同类细线」（复审 r3 的
            // `decoy_outline_gap` / `decoy_hover_gap` 两个反例都因此锚错行）。
            let Some(coverage) = Self::blend_coverage(line, background, expected) else {
                eprintln!(
                    "[probe] divider skip y={best} color={line:?}（不是预期角色色的插值，拒绝）"
                );
                continue;
            };
            if !(0.30..=1.05).contains(&coverage) {
                eprintln!(
                    "[probe] divider skip y={best} color={line:?}（覆盖率 {coverage:.2} 出界，拒绝）"
                );
                continue;
            }
            return Some((best, first, last, line));
        }
        None
    }

    /// `y` 行 `[x0, x1]` 上「按该行**墨迹众数色**」数的连通段数（分隔线断没断）。
    ///
    /// 参考色为什么不能取「跨度中点那一个像素」：缺口恰落在中点时，参考色变成页面底色，
    /// 数出来的是「底色有几段」→ 恒为 1 → **假绿**（复审 r3 的 `gap_at_mid` 反例）。
    /// 众数色在整段跨度里天然是线自己的颜色（242px 的线 vs ≤4px 的缺口）。
    pub(super) fn line_segments(&self, y: usize, x0: usize, x1: usize) -> usize {
        let line = self.modal_color(y, x0, x1);
        let mut segments = 0;
        let mut in_run = false;
        for x in x0..=x1 {
            let on = matches(self.at(x, y), line);
            if on && !in_run {
                segments += 1;
                in_run = true;
            } else if !on {
                in_run = false;
            }
        }
        segments
    }

    /// `y` 行 `[x0, x1]` 里出现次数最多的颜色（分隔线的线色）。
    pub(super) fn modal_color(&self, y: usize, x0: usize, x1: usize) -> Rgb {
        let mut counts: std::collections::HashMap<Rgb, usize> = std::collections::HashMap::new();
        for x in x0..=x1.min(self.width - 1) {
            *counts.entry(self.at(x, y)).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .map(|(c, _)| c)
            .unwrap_or(self.at(x0.min(self.width - 1), y))
    }

    /// `pixel` 是否落在 `from → to` 的插值线上（覆盖率 t），返回三个通道一致时的平均 t。
    ///
    /// 用来给「这条线是不是某个角色色」做**身份校验**：比「单通道差 ≤N」稳得多——
    /// 实测 `OutlineVariant (200,212,224)` → t=(1.84,2.24,3.1) 出界；
    /// hover 8% `(228,233,238)` → t=(0.72,1.0,1.7) 三通道散开 0.98；两者都被拒。
    /// 而真分隔线满覆盖 `(221,233,245)` → t=(1,1,1)；40% 覆盖的抗锯齿行 → t≈(0.36,0.35,0.40) 被接受。
    ///
    /// 只统计 `|to - from| > 4` 的通道（差太小的通道不携带信息，噪声会把 t 带偏）。
    pub(super) fn blend_coverage(pixel: Rgb, from: Rgb, to: Rgb) -> Option<f32> {
        let mut ts = Vec::new();
        for ((p, f), t) in pixel.iter().zip(from).zip(to) {
            let delta = f32::from(t) - f32::from(f);
            if delta.abs() <= 4.0 {
                continue;
            }
            ts.push((f32::from(*p) - f32::from(f)) / delta);
        }
        if ts.is_empty() {
            return None;
        }
        let max = ts.iter().cloned().fold(f32::MIN, f32::max);
        let min = ts.iter().cloned().fold(f32::MAX, f32::min);
        if max - min > 0.25 {
            return None;
        }
        Some(ts.iter().sum::<f32>() / ts.len() as f32)
    }

    /// 矩形内「与底色不同」的像素的左右范围（量文本墨迹用）。
    ///
    /// 为什么不按精确文字色找：小字号抗锯齿后几乎没有像素落在**精确**色上（容差 3 时实测
    /// 「Logs」只剩 12px）。改成「与底色不同即墨迹」，既与字色无关，也不会被状态层带偏。
    pub(super) fn band_ink(
        &self,
        background: Rgb,
        x0: usize,
        x1: usize,
        y0: usize,
        y1: usize,
    ) -> Option<(usize, usize)> {
        let mut span: Option<(usize, usize)> = None;
        for y in y0..=y1.min(self.height - 1) {
            for x in x0..=x1.min(self.width - 1) {
                let pixel = self.at(x, y);
                let differs = pixel
                    .iter()
                    .zip(background)
                    .any(|(a, b)| a.abs_diff(b) > 12);
                if differs {
                    span = Some(match span {
                        Some((l, r)) => (l.min(x), r.max(x)),
                        None => (x, x),
                    });
                }
            }
        }
        span
    }
}
