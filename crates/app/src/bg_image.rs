//! 背景图与玻璃拟态模糊层的准备（09 §101/§103 的「预生成」手法，纯 Rust、无新依赖）。
//!
//! 为什么要自己算：Slint 没有 `backdrop-filter`，也无法在渲染管线里做高斯模糊。可用的只有
//! `Image::load_from_path` + 像素缓冲（`to_rgba8` / `from_rgba8`）。于是：
//!
//! - **模糊层** = 把原图按面积平均降到极小的尺寸（默认宽 48px），渲染时被平滑放大 → 天然模糊
//!   （面积平均本身就是低通滤波，比在满分辨率上跑 box blur 快几个数量级，且一次只在设置变更时算）；
//! - **tile 模式** = Slint 的 `image-fit` 没有 `repeat`，所以在 Rust 侧按窗口尺寸平铺成一张画布，
//!   交给 `image-fit: cover`（画布与窗口同尺寸，等价于 1:1 铺设）。
//!
//! 解码格式：Slint 默认只编译 PNG 解码器（`image-default-formats` 特性未开），因此这里只保证 PNG；
//! 其它格式 `load_from_path` 直接失败，UI 侧给出提示而不是静默无效果。

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

use unlucky_presenter::prefs::BgFill;

/// 模糊层目标宽度（像素）。越小越糊；96 在 1180px 宽窗口上 ≈ 24px 半径高斯的效果。
const BLUR_WIDTH: u32 = 96;
/// 小图上再跑几轮 box blur（3×3）：把降采样的块状边缘抹平。
const BLUR_PASSES: usize = 3;
/// tile 画布上限（防超大窗口吃爆内存：4096×2560×4 ≈ 40MB）。
const TILE_MAX_WIDTH: u32 = 4096;
const TILE_MAX_HEIGHT: u32 = 2560;

/// 一次准备出来的两张图：原图（按填充模式处理）+ 模糊层。
#[derive(Debug, Clone, Default)]
pub struct BackgroundLayers {
    /// 铺在窗口底的图（tile 模式下已平铺成整窗画布）
    pub image: Image,
    /// 玻璃拟态用的模糊层
    pub blur: Image,
}

/// 载入并加工背景图。
///
/// `viewport` 是窗口的**物理像素**尺寸（tile 模式平铺用；拿不到时传 `(0, 0)` 退回只载入）。
/// 返回 `None` 表示路径无效 / 解码失败 / 读了像素但为空——调用方据此提示用户。
#[must_use]
pub fn prepare(path: &str, fill: BgFill, viewport: (u32, u32)) -> Option<BackgroundLayers> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let loaded = Image::load_from_path(std::path::Path::new(path)).ok()?;
    let source = loaded.to_rgba8()?;
    if source.width() == 0 || source.height() == 0 {
        return None;
    }

    let image = match fill {
        BgFill::Tile => {
            let (width, height) = tile_canvas_size(source.width(), source.height(), viewport);
            Image::from_rgba8(tile(&source, width, height))
        }
        // cover / contain 交给 Slint 的 image-fit，这里保留原图
        BgFill::Cover | BgFill::Contain => loaded,
    };

    let blurred = box_blur(&downscale(&source, BLUR_WIDTH), BLUR_PASSES);
    // 放大回窗口物理尺寸再做 1:1 贴图：软件渲染器对图片缩放不做平滑插值（最近邻），
    // 直接把 96px 的小图丢给它会出现明显方块；这里自己双线性放大，观感才是「磨砂」。
    let blur = match viewport {
        (0, 0) => Image::from_rgba8(blurred),
        (width, height) => Image::from_rgba8(upscale(&blurred, width, height)),
    };

    Some(BackgroundLayers { image, blur })
}

/// 3×3 均值模糊，`passes` 轮（小图上跑，代价可忽略）。
fn box_blur(
    source: &SharedPixelBuffer<Rgba8Pixel>,
    passes: usize,
) -> SharedPixelBuffer<Rgba8Pixel> {
    let width = source.width() as usize;
    let height = source.height() as usize;
    if width < 3 || height < 3 || passes == 0 {
        return source.clone();
    }
    let mut current: Vec<Rgba8Pixel> = source.as_slice().to_vec();
    let mut next = current.clone();
    for _ in 0..passes {
        for y in 0..height {
            for x in 0..width {
                let mut sum = [0u32; 4];
                let mut count = 0u32;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let sy = (y as i32 + dy).clamp(0, height as i32 - 1) as usize;
                        let sx = (x as i32 + dx).clamp(0, width as i32 - 1) as usize;
                        let pixel = current[sy * width + sx];
                        sum[0] += u32::from(pixel.r);
                        sum[1] += u32::from(pixel.g);
                        sum[2] += u32::from(pixel.b);
                        sum[3] += u32::from(pixel.a);
                        count += 1;
                    }
                }
                next[y * width + x] = Rgba8Pixel {
                    r: (sum[0] / count) as u8,
                    g: (sum[1] / count) as u8,
                    b: (sum[2] / count) as u8,
                    a: (sum[3] / count) as u8,
                };
            }
        }
        std::mem::swap(&mut current, &mut next);
    }
    let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(source.width(), source.height());
    out.make_mut_slice().copy_from_slice(&current);
    out
}

/// 双线性放大到目标尺寸。
fn upscale(
    source: &SharedPixelBuffer<Rgba8Pixel>,
    width: u32,
    height: u32,
) -> SharedPixelBuffer<Rgba8Pixel> {
    let src_w = source.width();
    let src_h = source.height();
    if src_w == 0 || src_h == 0 || width == 0 || height == 0 {
        return SharedPixelBuffer::new(width.max(1), height.max(1));
    }
    let pixels = source.as_slice();
    let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    let dst = out.make_mut_slice();
    let sample = |fx: f32, fy: f32| -> Rgba8Pixel {
        let x = (fx * (src_w - 1) as f32).clamp(0.0, (src_w - 1) as f32);
        let y = (fy * (src_h - 1) as f32).clamp(0.0, (src_h - 1) as f32);
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(src_w as usize - 1);
        let y1 = (y0 + 1).min(src_h as usize - 1);
        let tx = x - x0 as f32;
        let ty = y - y0 as f32;
        let at = |px: usize, py: usize| pixels[py * src_w as usize + px];
        let blend = |a: u8, b: u8, c: u8, d: u8| -> u8 {
            let top = f32::from(a) * (1.0 - tx) + f32::from(b) * tx;
            let bottom = f32::from(c) * (1.0 - tx) + f32::from(d) * tx;
            (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8
        };
        let (p00, p10, p01, p11) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
        Rgba8Pixel {
            r: blend(p00.r, p10.r, p01.r, p11.r),
            g: blend(p00.g, p10.g, p01.g, p11.g),
            b: blend(p00.b, p10.b, p01.b, p11.b),
            a: blend(p00.a, p10.a, p01.a, p11.a),
        }
    };
    for y in 0..height as usize {
        let fy = y as f32 / height as f32;
        for x in 0..width as usize {
            dst[y * width as usize + x] = sample(x as f32 / width as f32, fy);
        }
    }
    out
}

/// 平铺画布尺寸：覆盖整个视口、且是原图尺寸的整数倍（超上限就截断）。
fn tile_canvas_size(source_width: u32, source_height: u32, viewport: (u32, u32)) -> (u32, u32) {
    if viewport.0 == 0 || viewport.1 == 0 {
        return (source_width, source_height);
    }
    let repeat_x = viewport.0.div_ceil(source_width).max(1);
    let repeat_y = viewport.1.div_ceil(source_height).max(1);
    let width = (repeat_x * source_width).min(TILE_MAX_WIDTH);
    let height = (repeat_y * source_height).min(TILE_MAX_HEIGHT);
    (width.max(source_width), height.max(source_height))
}

/// 按整数格平铺（源图超出画布的部分裁掉）。
fn tile(
    source: &SharedPixelBuffer<Rgba8Pixel>,
    width: u32,
    height: u32,
) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut canvas = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    let src_w = source.width() as usize;
    let src_h = source.height() as usize;
    let src = source.as_slice();
    let dst = canvas.make_mut_slice();
    for y in 0..height as usize {
        let src_row = (y % src_h) * src_w;
        let dst_row = y * width as usize;
        for x in 0..width as usize {
            dst[dst_row + x] = src[src_row + (x % src_w)];
        }
    }
    canvas
}

/// 面积平均降采样（保留宽高比，最窄 1px）。
fn downscale(
    source: &SharedPixelBuffer<Rgba8Pixel>,
    target_width: u32,
) -> SharedPixelBuffer<Rgba8Pixel> {
    let src_w = source.width();
    let src_h = source.height();
    let target_width = target_width.clamp(1, src_w);
    let target_height = ((src_h as u64 * target_width as u64) / src_w as u64).max(1) as u32;

    let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(target_width, target_height);
    let src = source.as_slice();
    let dst = out.make_mut_slice();

    for ty in 0..target_height as usize {
        // 每个目标像素覆盖的源矩形（按比例分块，块内取平均）
        let y0 = ty * src_h as usize / target_height as usize;
        let y1 = ((ty + 1) * src_h as usize / target_height as usize).max(y0 + 1);
        for tx in 0..target_width as usize {
            let x0 = tx * src_w as usize / target_width as usize;
            let x1 = ((tx + 1) * src_w as usize / target_width as usize).max(x0 + 1);
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            for y in y0..y1.min(src_h as usize) {
                for x in x0..x1.min(src_w as usize) {
                    let pixel = src[y * src_w as usize + x];
                    sum[0] += u32::from(pixel.r);
                    sum[1] += u32::from(pixel.g);
                    sum[2] += u32::from(pixel.b);
                    sum[3] += u32::from(pixel.a);
                    count += 1;
                }
            }
            let count = count.max(1);
            dst[ty * target_width as usize + tx] = Rgba8Pixel {
                r: (sum[0] / count) as u8,
                g: (sum[1] / count) as u8,
                b: (sum[2] / count) as u8,
                a: (sum[3] / count) as u8,
            };
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, pixel: Rgba8Pixel) -> SharedPixelBuffer<Rgba8Pixel> {
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
        buffer.make_mut_slice().fill(pixel);
        buffer
    }

    #[test]
    fn downscale_keeps_aspect_and_averages_area() {
        let mut source = SharedPixelBuffer::<Rgba8Pixel>::new(96, 48);
        // 左半黑、右半白：降采样后左右两侧应各自保持纯色
        for y in 0..48 {
            for x in 0..96 {
                let value = if x < 48 { 0 } else { 255 };
                source.make_mut_slice()[y * 96 + x] = Rgba8Pixel {
                    r: value,
                    g: value,
                    b: value,
                    a: 255,
                };
            }
        }
        let small = downscale(&source, 24);
        assert_eq!(small.width(), 24);
        assert_eq!(small.height(), 12, "宽高比保持 2:1");
        let pixels = small.as_slice();
        assert_eq!(pixels[0].r, 0, "左侧仍是黑");
        assert_eq!(pixels[23].r, 255, "右侧仍是白");
    }

    #[test]
    fn downscale_never_produces_zero_height() {
        let source = solid(
            400,
            3,
            Rgba8Pixel {
                r: 1,
                g: 2,
                b: 3,
                a: 255,
            },
        );
        let small = downscale(&source, 2);
        assert_eq!(small.width(), 2);
        assert!(small.height() >= 1);
    }

    #[test]
    fn tile_canvas_covers_viewport_in_whole_repeats() {
        assert_eq!(tile_canvas_size(120, 100, (1180, 760)), (1200, 800));
        // 视口未知（取不到窗口尺寸）时退回单块
        assert_eq!(tile_canvas_size(120, 100, (0, 0)), (120, 100));
        // 超大视口被上限截断
        let (width, height) = tile_canvas_size(64, 64, (9000, 9000));
        assert!(width <= TILE_MAX_WIDTH && height <= TILE_MAX_HEIGHT);
    }

    #[test]
    fn tile_repeats_source_pixels() {
        let source = solid(
            2,
            2,
            Rgba8Pixel {
                r: 9,
                g: 9,
                b: 9,
                a: 255,
            },
        );
        let canvas = tile(&source, 5, 3);
        assert_eq!(canvas.width(), 5);
        assert_eq!(canvas.height(), 3);
        assert!(canvas.as_slice().iter().all(|pixel| pixel.r == 9));
    }

    #[test]
    fn prepare_rejects_bad_input() {
        assert!(prepare("", BgFill::Cover, (100, 100)).is_none());
        assert!(prepare("D:/definitely/not/here.png", BgFill::Cover, (100, 100)).is_none());
    }

    #[test]
    fn box_blur_smooths_a_hard_edge() {
        // 左黑右白：模糊后交界处应出现中间灰（硬边被抹平）
        let mut source = SharedPixelBuffer::<Rgba8Pixel>::new(9, 3);
        for y in 0..3 {
            for x in 0..9 {
                let value = if x < 4 { 0 } else { 255 };
                source.make_mut_slice()[y * 9 + x] = Rgba8Pixel {
                    r: value,
                    g: value,
                    b: value,
                    a: 255,
                };
            }
        }
        let blurred = box_blur(&source, 2);
        let middle = blurred.as_slice()[9 + 4]; // 第 1 行（共 9 列）的第 4 个像素
        assert!(
            middle.r > 20 && middle.r < 235,
            "交界处应被抹成中间灰，实际 {}",
            middle.r
        );
    }

    #[test]
    fn upscale_matches_requested_size_and_interpolates() {
        let mut source = SharedPixelBuffer::<Rgba8Pixel>::new(2, 1);
        let pixels = source.make_mut_slice();
        pixels[0] = Rgba8Pixel {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };
        pixels[1] = Rgba8Pixel {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        let big = upscale(&source, 8, 2);
        assert_eq!(big.width(), 8);
        assert_eq!(big.height(), 2);
        let row = big.as_slice();
        // 双线性：中间像素应是灰阶过渡而不是 0/255 二值
        assert!(row[3].r > 60 && row[3].r < 200, "实际 {}", row[3].r);
    }
}
