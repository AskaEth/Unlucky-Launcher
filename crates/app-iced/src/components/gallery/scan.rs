//! 截图像素扫描工具（`gallery::probe` 的底层，四个后续家族的 probe 也能复用）。
//!
//! 只做「按颜色找连通块 / 量外接框 / 量某行的连续长度」三件事，不含任何判据——
//! 判据在同级 `probe.rs` 里，这样每条家族线可以自带 probe 而不用重写扫描逻辑。

use std::path::Path;

use iced::Color;

/// 颜色比对的通道容差（抗锯齿/取整）。
const TOLERANCE: u8 = 10;

/// iced 颜色 → 8bit RGB（截图里的原值）。
pub(super) fn rgb(color: Color) -> [u8; 3] {
    [
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
    ]
}

/// 颜色是否落在容差内。
pub(super) fn matches(pixel: [u8; 3], expected: [u8; 3]) -> bool {
    pixel
        .iter()
        .zip(expected)
        .all(|(a, b)| a.abs_diff(b) <= TOLERANCE)
}

/// 一张图的像素访问器。
pub(super) struct Shot {
    pub(super) width: usize,
    pub(super) height: usize,
    pixels: Vec<[u8; 3]>,
}

impl Shot {
    pub(super) fn load(path: &Path) -> Result<Self, String> {
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

    /// 某种颜色的全部像素的外接框（连通块）。
    pub(super) fn bbox_of(&self, color: [u8; 3]) -> Vec<BBox> {
        let mut seen = vec![false; self.width * self.height];
        let mut boxes = Vec::new();

        for y in 0..self.height {
            for x in 0..self.width {
                if seen[y * self.width + x] || !matches(self.at(x, y), color) {
                    continue;
                }

                let mut stack = vec![(x, y)];
                seen[y * self.width + x] = true;
                let mut bbox = BBox::new(x, y);

                while let Some((cx, cy)) = stack.pop() {
                    bbox.include(cx, cy);
                    for (nx, ny) in [
                        (cx + 1, cy),
                        (cx.saturating_sub(1), cy),
                        (cx, cy + 1),
                        (cx, cy.saturating_sub(1)),
                    ] {
                        let inside = nx < self.width && ny < self.height;
                        if inside && !seen[ny * self.width + nx] && matches(self.at(nx, ny), color)
                        {
                            seen[ny * self.width + nx] = true;
                            stack.push((nx, ny));
                        }
                    }
                }

                if bbox.area() > 200 {
                    boxes.push(bbox);
                }
            }
        }

        boxes
    }

    /// 在矩形 `[x0,x1) × [y0,y1)` 内数某颜色的像素（按行/列取证用）。
    pub(super) fn count_in(
        &self,
        color: [u8; 3],
        x0: usize,
        x1: usize,
        y0: usize,
        y1: usize,
    ) -> usize {
        (y0..y1.min(self.height))
            .map(|y| {
                (x0..x1.min(self.width))
                    .filter(|x| matches(self.at(*x, y), color))
                    .count()
            })
            .sum()
    }

    /// `bbox` 内某颜色的墨迹外接框（量字形用）。
    ///
    /// 内缩 3px：不内缩会把按钮自身的抗锯齿边算成墨迹（实测把 24 盒字形量成 40）。
    pub(super) fn ink_inside(&self, bbox: &BBox, color: [u8; 3]) -> Option<BBox> {
        const MARGIN: usize = 3;
        let mut ink: Option<BBox> = None;
        for y in (bbox.top + MARGIN)..=(bbox.bottom - MARGIN) {
            for x in (bbox.left + MARGIN)..=(bbox.right - MARGIN) {
                if matches(self.at(x, y), color) {
                    match &mut ink {
                        Some(found) => found.include(x, y),
                        None => ink = Some(BBox::new(x, y)),
                    }
                }
            }
        }
        ink
    }
}

/// 一个外接框（闭区间）。
#[derive(Debug, Clone, Copy)]
#[allow(clippy::struct_field_names)]
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

    pub(super) fn area(&self) -> usize {
        self.width() * self.height()
    }
}

/// `pixel` 更接近 `fill` 还是 `surface`。
pub(super) fn nearer(pixel: [u8; 3], fill: [u8; 3], surface: [u8; 3]) -> bool {
    let dist = |a: [u8; 3], b: [u8; 3]| -> u32 {
        a.iter()
            .zip(b)
            .map(|(x, y)| (i32::from(*x) - i32::from(y)).unsigned_abs())
            .sum()
    };
    dist(pixel, fill) < dist(pixel, surface)
}
