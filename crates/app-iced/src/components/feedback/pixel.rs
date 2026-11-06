//! 像素取证的小工具（**仅测试**）：读 PNG、按「底色 → 墨色」的覆盖率找连通块与长条。
//!
//! 为什么不复用 `gallery/scan.rs`：那个模块的可见性是
//! `pub(in crate::app::components::gallery)`，从家族目录拿不到（`selection/pixel.rs` 也是
//! 同样的理由自己写了一份）。这里只重写本家族真正用到的几个纯函数。
//!
//! **判据口径 1：覆盖率而不是精确色。** 4px 厚的进度条、28 圆角的卡片，边缘全是抗锯齿；
//! 按「精确色」连通会把它们切碎（实测进度条只剩中间 1~2 行）。
//! **判据口径 2：覆盖率 ≥50%。** 50% 是几何边界（30% 会把半格抗锯齿也算进去，4px 量成 6）。

use std::path::PathBuf;

use iced::Color;

/// 8bit RGB。
pub(super) fn rgb(color: Color) -> [u8; 3] {
    [
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
    ]
}

/// 容差比较（抗锯齿/取整）。
pub(super) fn matches(pixel: [u8; 3], expected: [u8; 3]) -> bool {
    pixel.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 10)
}

/// `ULL_GALLERY_SHOT` → 截图路径（相对路径按**工作区根**解释：`cargo test` 的 cwd 是 crate 目录）。
pub(super) fn shot_path() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("ULL_GALLERY_SHOT")?);
    if path.is_absolute() || path.exists() {
        return Some(path);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Some(root.join(path))
}

/// 一张图的像素访问器。
pub(super) struct Shot {
    pub(super) width: usize,
    pub(super) height: usize,
    pixels: Vec<[u8; 3]>,
}

impl Shot {
    pub(super) fn load(path: &std::path::Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|error| format!("读不动 {path:?}：{error}"))?;
        let image = image::load_from_memory(&bytes)
            .map_err(|error| format!("解 PNG 失败：{error}"))?
            .to_rgb8();
        let (width, height) = (image.width() as usize, image.height() as usize);
        Ok(Self {
            width,
            height,
            pixels: image.pixels().map(|p| p.0).collect(),
        })
    }

    pub(super) fn at(&self, x: usize, y: usize) -> [u8; 3] {
        self.pixels[y * self.width + x]
    }
}

/// 一个外接框（闭区间）。
#[derive(Debug, Clone, Copy)]
pub(super) struct BBox {
    pub(super) left: usize,
    pub(super) right: usize,
    pub(super) top: usize,
    pub(super) bottom: usize,
}

impl BBox {
    pub(super) fn new(x: usize, y: usize) -> Self {
        Self {
            left: x,
            right: x,
            top: y,
            bottom: y,
        }
    }

    pub(super) fn include(&mut self, x: usize, y: usize) {
        self.left = self.left.min(x);
        self.right = self.right.max(x);
        self.top = self.top.min(y);
        self.bottom = self.bottom.max(y);
    }

    pub(super) fn width(&self) -> usize {
        self.right - self.left + 1
    }

    pub(super) fn height(&self) -> usize {
        self.bottom - self.top + 1
    }
}

/// 尺寸容差 ±1（rubric §6 的默认容差：控件常落在半像素上，实测会多一行抗锯齿）。
pub(super) fn near(value: usize, expected: usize) -> bool {
    value.abs_diff(expected) <= 1
}

pub(super) fn dims(boxes: &[BBox]) -> String {
    boxes
        .iter()
        .map(|bbox| format!("{}×{}", bbox.width(), bbox.height()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `pixel` 在「`under` → `fill`」这条混色线上的**覆盖率**（0..1；≥0.5 视为「算这块墨」）。
///
/// 投影到混色向量上取比例：抗锯齿像素正好落在 0..1 之间，另一种墨（文字、别的容器色）
/// 会落在线的两侧偏远处，比例被夹到 0 或 1 之外 → 用「偏离线距离」再筛一道。
pub(super) fn coverage(pixel: [u8; 3], fill: [u8; 3], under: [u8; 3]) -> f32 {
    let axis = [0, 1, 2].map(|i| f32::from(fill[i]) - f32::from(under[i]));
    let offset = [0, 1, 2].map(|i| f32::from(pixel[i]) - f32::from(under[i]));
    let square: f32 = axis.iter().map(|a| a * a).sum();
    if square <= f32::EPSILON {
        return 0.0;
    }
    let t: f32 = (0..3).map(|i| axis[i] * offset[i]).sum::<f32>() / square;
    let distance = (0..3)
        .map(|i| (offset[i] - t * axis[i]).powi(2))
        .sum::<f32>()
        .sqrt();
    // 8.0 而不是 16.0：深色填充（如 InverseSurface）与浅底之间的混色轴很长，深色**文字**
    // （OnSurfaceVariant）投影上去残差只有 ≈9 —— 16 的阈值会把文字行算成填充，于是 48 高的
    // 提示条被切成好几段（实测 46 + 1 + 1）。8.0 只排除文字，保留抗锯齿边（残差 0~3）。
    if distance > 8.0 {
        return 0.0;
    }
    t.clamp(0.0, 1.0)
}

/// 第 `y` 行上所有「算 `fill` 那块墨」的连续段 `(起点, 长度)`。
pub(super) fn runs(shot: &Shot, y: usize, fill: Color, under: Color) -> Vec<(usize, usize)> {
    let (ink, base) = (rgb(fill), rgb(under));
    let mut found = Vec::new();
    let mut start: Option<usize> = None;
    for x in 0..shot.width {
        let hit = coverage(shot.at(x, y), ink, base) >= 0.5;
        match (hit, start) {
            (true, None) => start = Some(x),
            (false, Some(from)) => {
                found.push((from, x - from));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        found.push((from, shot.width - from));
    }
    found
}

/// 一行里有多少「算 `fill` 那块墨」的像素（按覆盖率 ≥50% 判）。
///
/// 用**计数**而不是「最长段」：一条 344 宽的提示条，里面一行中文字会把它切成几段，
/// 按最长段判会把中间那些行漏掉（实测 48 高被切成 6 + 6 两段）。
pub(super) fn row_ink(shot: &Shot, y: usize, fill: Color, under: Color) -> usize {
    let (ink, base) = (rgb(fill), rgb(under));
    (0..shot.width)
        .filter(|x| coverage(shot.at(*x, y), ink, base) >= 0.5)
        .count()
}

/// 连续行里「墨量 ≥ `min_ink`」的那些行 → 聚成**竖带** `(top, bottom)`。
///
/// 用途：一条 4px 的进度条在图上就是「4 个连续行、每行都有 ~320 个墨点」；
/// 一个 48 高的提示条就是「48 个连续行」。比按精确色找 bbox 稳得多。
pub(super) fn bands(shot: &Shot, fill: Color, under: Color, min_ink: usize) -> Vec<(usize, usize)> {
    let mut bands: Vec<(usize, usize)> = Vec::new();
    let mut current: Option<usize> = None;
    for y in 0..shot.height {
        let hit = row_ink(shot, y, fill, under) >= min_ink;
        match (hit, current) {
            (true, None) => current = Some(y),
            (false, Some(top)) => {
                bands.push((top, y - 1));
                current = None;
            }
            _ => {}
        }
    }
    if let Some(top) = current {
        bands.push((top, shot.height - 1));
    }
    bands
}

/// 竖带里最宽的一行的段长：取「每行最长连通段」的最大值。
///
/// 为什么要取最大而不是某一行：矩形控件上下有留白行（没文字），只有那几行量得到全宽；
/// 文字会把中间行切成几段。两条同色矩形并排时，`runs` 分别返回各自的长段，所以也不会串。
pub(super) fn band_width(shot: &Shot, band: (usize, usize), fill: Color, under: Color) -> usize {
    (band.0..=band.1)
        .map(|y| {
            runs(shot, y, fill, under)
                .into_iter()
                .map(|(_, len)| len)
                .max()
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0)
}

/// 某种颜色的**精确色**连通块（只用于「找着一大块实心」这种粗定位）。
pub(super) fn blobs(shot: &Shot, color: Color, min_side: usize) -> Vec<BBox> {
    let target = rgb(color);
    let mut seen = vec![false; shot.width * shot.height];
    let mut found = Vec::new();
    for y in 0..shot.height {
        for x in 0..shot.width {
            if seen[y * shot.width + x] || !matches(shot.at(x, y), target) {
                continue;
            }
            let mut stack = vec![(x, y)];
            seen[y * shot.width + x] = true;
            let mut bbox = BBox::new(x, y);
            while let Some((cx, cy)) = stack.pop() {
                bbox.include(cx, cy);
                for (nx, ny) in [
                    (cx + 1, cy),
                    (cx.saturating_sub(1), cy),
                    (cx, cy + 1),
                    (cx, cy.saturating_sub(1)),
                ] {
                    let inside = nx < shot.width && ny < shot.height;
                    if inside && !seen[ny * shot.width + nx] && matches(shot.at(nx, ny), target) {
                        seen[ny * shot.width + nx] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            if bbox.width() >= min_side && bbox.height() >= min_side {
                found.push(bbox);
            }
        }
    }
    found
}

/// 在 `bbox` 内找「最接近 `ink`（而不是 `under`）」的像素外接框（量文字/图标的内缩）。
pub(super) fn ink_inside(
    shot: &Shot,
    bbox: &BBox,
    ink: Color,
    under: Color,
    margin: usize,
) -> Option<BBox> {
    let (target, base) = (rgb(ink), rgb(under));
    let mut found: Option<BBox> = None;
    for y in (bbox.top + margin)..=(bbox.bottom.saturating_sub(margin)) {
        for x in (bbox.left + margin)..=(bbox.right.saturating_sub(margin)) {
            let pixel = shot.at(x, y);
            let to_ink = pixel
                .iter()
                .zip(target)
                .map(|(a, b)| u32::from(a.abs_diff(b)))
                .sum::<u32>();
            let to_base = pixel
                .iter()
                .zip(base)
                .map(|(a, b)| u32::from(a.abs_diff(b)))
                .sum::<u32>();
            if to_ink < to_base {
                match &mut found {
                    Some(bbox) => bbox.include(x, y),
                    None => found = Some(BBox::new(x, y)),
                }
            }
        }
    }
    found
}
