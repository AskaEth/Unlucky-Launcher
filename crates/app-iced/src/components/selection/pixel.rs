//! 像素取证的小工具（**仅测试**）：读 PNG、按「底色→墨色混色线」找连通块、量内缩墨迹。
//!
//! 为什么不去用 `gallery/scan.rs`：它的可见性是 `pub(in crate::app::components::gallery)`，
//! 跨目录拿不到；这里只重写 4 个纯函数（`rgb` / `matches` / 像素访问 / 外接框），其余判据
//! （[`on_ramp`] / [`ink_boxes`] / [`inner_ink`]）是一次性的测量逻辑，本来就只服务本家族。

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

/// `ULL_GALLERY_SHOT` → 截图路径（相对路径按工作区根解释）。
pub(super) fn shot_path() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("ULL_GALLERY_SHOT")?);
    if path.is_absolute() || path.exists() {
        return Some(path);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Some(root.join(path))
}

/// 一张图的像素访问器（只做本家族需要的事）。
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

/// 外接框（闭区间）。
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

pub(super) fn center(bbox: &BBox) -> (usize, usize) {
    ((bbox.left + bbox.right) / 2, (bbox.top + bbox.bottom) / 2)
}

/// 尺寸容差 ±1（rubric §6 的默认容差）：控件常落在半像素上，实测会多一行抗锯齿。
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

/// `pixel` 是否落在 `under → fill` 这条混色线上（覆盖率 ≥50%、偏离线 ≤14）。
///
/// 覆盖率 ≥50% 取几何边界（30% 会把半格抗锯齿也算进去，Ø10 量成 12）；
/// 偏离线 ≤14 把「另一种深色墨」排除（它不在线上，见 [`ink_boxes`] 的坑 2）。
fn on_ramp(pixel: [u8; 3], fill: [u8; 3], under: [u8; 3]) -> bool {
    let axis = [0, 1, 2].map(|i| f32::from(fill[i]) - f32::from(under[i]));
    let offset = [0, 1, 2].map(|i| f32::from(pixel[i]) - f32::from(under[i]));
    let square: f32 = axis.iter().map(|a| a * a).sum();
    if square <= f32::EPSILON {
        return false;
    }
    let t: f32 = (0..3).map(|i| axis[i] * offset[i]).sum::<f32>() / square;
    (0.5..=1.0).contains(&t)
        && (0..3)
            .map(|i| (offset[i] - t * axis[i]).powi(2))
            .sum::<f32>()
            .sqrt()
            <= 14.0
}

/// 「墨迹块」：`fill` 与「底色→fill 的混色」都算墨，按外接框尺寸筛。
///
/// 踩过的两个坑：① 按「精确色」连通会把圆形的环切成碎片（每片面积很小，量不到内点）；
/// ② 按「更接近 fill 还是底色」判会让**另一种深色墨**混进来（`OnSurfaceVariant` 比 `Primary`
/// 更接近深色，于是未选中的框被当成 20×20 的 Primary 块）。所以判据是「落在混色线上」。
pub(super) fn ink_boxes(
    shot: &Shot,
    color: Color,
    under: Color,
    min: usize,
    max: usize,
) -> Vec<BBox> {
    let (fill, base) = (rgb(color), rgb(under));
    let mut seen = vec![false; shot.width * shot.height];
    let mut boxes = Vec::new();

    for y in 0..shot.height {
        for x in 0..shot.width {
            if seen[y * shot.width + x] || !on_ramp(shot.at(x, y), fill, base) {
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
                    if inside && !seen[ny * shot.width + nx] && on_ramp(shot.at(nx, ny), fill, base)
                    {
                        seen[ny * shot.width + nx] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            if (min..=max).contains(&bbox.width()) && (min..=max).contains(&bbox.height()) {
                boxes.push(bbox);
            }
        }
    }

    boxes
}

/// `bbox` 内缩 `inset` 后、落在 **`under → color` 混色线上**的墨迹框（量内点 / 勾形用）。
///
/// 内缩 4 而不是 3（描边内缘 AA 正好压在 3 上，会把 Ø10 量成 14）；用混色线而不是精确色
/// （Ø10 的圆只有 ~8×8 是精确色，边缘 1px 全是抗锯齿）。
pub(super) fn inner_ink(
    shot: &Shot,
    bbox: &BBox,
    color: Color,
    under: Color,
    inset: usize,
) -> Option<BBox> {
    let mut ink: Option<BBox> = None;
    for y in (bbox.top + inset)..=(bbox.bottom - inset) {
        for x in (bbox.left + inset)..=(bbox.right - inset) {
            if on_ramp(shot.at(x, y), rgb(color), rgb(under)) {
                match &mut ink {
                    Some(found) => found.include(x, y),
                    None => ink = Some(BBox::new(x, y)),
                }
            }
        }
    }
    ink
}
