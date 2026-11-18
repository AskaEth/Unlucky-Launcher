//! 两图逐像素对比（GPU/CPU 几何一致性）与 ±N px 平移扫描。
//!
//! 拆成子模块只为「src 文件 ≤400 行」的门禁；对外通过 `shot::{compare, diff_files, Diff, Image}`
//! 使用，调用方不用感知这层。

use std::path::Path;

/// 一张已解码的 RGBA 图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// RGBA8 字节（逐像素 4 字节）。
    pub rgba: Vec<u8>,
    /// 宽（px）。
    pub width: u32,
    /// 高（px）。
    pub height: u32,
}

impl Image {
    /// 读 PNG（只支持 PNG：本 crate 的 `image` 只开了 png 编码/解码）。
    ///
    /// # Errors
    /// 读文件失败或解码失败时返回人话。
    pub fn load(path: &Path) -> Result<Self, String> {
        let reader = image::ImageReader::open(path)
            .map_err(|error| format!("读图 {} 失败：{error}", path.display()))?
            .with_guessed_format()
            .map_err(|error| format!("判定 {} 的格式失败：{error}", path.display()))?;

        let decoded = reader
            .decode()
            .map_err(|error| format!("解码 {} 失败：{error}", path.display()))?
            .to_rgba8();

        Ok(Self {
            width: decoded.width(),
            height: decoded.height(),
            rgba: decoded.into_raw(),
        })
    }
}

/// 两图对比结果（GPU/CPU 一致性靠它给数字，不靠肉眼）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Diff {
    /// 宽（px）。
    pub width: u32,
    /// 高（px）。
    pub height: u32,
    /// 有差异的像素数。
    pub differing_pixels: usize,
    /// 单通道最大差（0~255）。
    pub max_channel_delta: u8,
    /// 单通道平均差（0~255）。
    pub mean_channel_delta: f64,
    /// 平移扫描的最佳对齐（dx, dy）；不扫描时为 (0, 0)。
    pub best_shift: (i32, i32),
    /// 最佳对齐下的差异像素数（不扫描时等于 `differing_pixels`）。
    pub best_differing_pixels: usize,
}

/// 逐像素对比两张尺寸相同的图。
///
/// # Errors
/// 尺寸不同直接报错——**几何不一致不许静默通过**。
pub fn compare(left: &Image, right: &Image) -> Result<Diff, String> {
    if (left.width, left.height) != (right.width, right.height) {
        return Err(format!(
            "尺寸不同：{}x{} vs {}x{}",
            left.width, left.height, right.width, right.height
        ));
    }

    let pixel_count = left.rgba.len() / 4;
    let mut differing = 0_usize;
    let mut max_delta = 0_u8;
    let mut sum_delta = 0_u64;

    for index in 0..pixel_count {
        let offset = index * 4;
        let mut pixel_differs = false;

        for channel in 0..4 {
            let delta = left.rgba[offset + channel].abs_diff(right.rgba[offset + channel]);
            if delta > 0 {
                pixel_differs = true;
                max_delta = max_delta.max(delta);
                sum_delta += u64::from(delta);
            }
        }

        if pixel_differs {
            differing += 1;
        }
    }

    Ok(Diff {
        width: left.width,
        height: left.height,
        differing_pixels: differing,
        max_channel_delta: max_delta,
        mean_channel_delta: if pixel_count == 0 {
            0.0
        } else {
            sum_delta as f64 / (pixel_count * 4) as f64
        },
        best_shift: (0, 0),
        best_differing_pixels: differing,
    })
}

/// ±`max_shift` px 的平移扫描：把 `right` 平移后再比，返回差异像素数最小的那一档。
///
/// 用途：D30 的「几何逐像素一致」除了看差异占比，还要证明**没有整体错位**——
/// 如果 (0,0) 就是最优，说明差异只来自抗锯齿而不是位移/缩放。CI 不必再依赖 numpy。
///
/// 平移后越界的像素按「无法比较」处理，不计入差异（否则边缘会造成假阳性）。
pub fn shift_scan(left: &Image, right: &Image, max_shift: i32) -> Result<Diff, String> {
    let mut best = compare(left, right)?;
    let max_shift = max_shift.unsigned_abs();

    if (left.width, left.height) != (right.width, right.height) {
        return Err("尺寸不同，无法做平移扫描".to_owned());
    }

    for dy in -(max_shift as i32)..=(max_shift as i32) {
        for dx in -(max_shift as i32)..=(max_shift as i32) {
            if (dx, dy) == (0, 0) {
                continue;
            }

            let count = shifted_differing_pixels(left, right, dx, dy);
            if count < best.best_differing_pixels {
                best.best_differing_pixels = count;
                best.best_shift = (dx, dy);
            }
        }
    }

    Ok(best)
}

/// 把 `right` 平移 `(dx, dy)` 后与 `left` 比较，返回差异像素数（越界不算）。
fn shifted_differing_pixels(left: &Image, right: &Image, dx: i32, dy: i32) -> usize {
    let width = left.width as i32;
    let height = left.height as i32;
    let mut differing = 0;

    for y in 0..height {
        let source_y = y - dy;
        if !(0..height).contains(&source_y) {
            continue;
        }

        for x in 0..width {
            let source_x = x - dx;
            if !(0..width).contains(&source_x) {
                continue;
            }

            let left_offset = ((y * width + x) * 4) as usize;
            let right_offset = ((source_y * width + source_x) * 4) as usize;

            if left.rgba[left_offset..left_offset + 4] != right.rgba[right_offset..right_offset + 4]
            {
                differing += 1;
            }
        }
    }

    differing
}

impl Diff {
    /// 完全一致（几何 + 颜色都一致）。
    pub fn is_identical(&self) -> bool {
        self.differing_pixels == 0
    }

    /// 一行日志。
    pub fn describe(&self) -> String {
        let total = (self.width as usize) * (self.height as usize);
        let percent = if total == 0 {
            0.0
        } else {
            self.differing_pixels as f64 * 100.0 / total as f64
        };

        format!(
            "[diff] {}x{} differing={}/{} ({percent:.2}%) max_delta={} mean_delta={:.4} best_shift=({},{}) best_differing={}",
            self.width,
            self.height,
            self.differing_pixels,
            total,
            self.max_channel_delta,
            self.mean_channel_delta,
            self.best_shift.0,
            self.best_shift.1,
            self.best_differing_pixels,
        )
    }
}

/// `--diff <left> <right>` 的实现：读两张 PNG、对比、打日志。
///
/// # Errors
/// 任一图读不出来，或尺寸不同。
pub fn diff_files(left: &Path, right: &Path) -> Result<Diff, String> {
    diff_files_with_scan(left, right, 0)
}

/// 同 [`diff_files`]，但额外做 ±`max_shift` px 的平移扫描（`--diff-scan`）。
///
/// # Errors
/// 任一图读不出来，或尺寸不同。
pub fn diff_files_with_scan(left: &Path, right: &Path, max_shift: i32) -> Result<Diff, String> {
    let left = Image::load(left)?;
    let right = Image::load(right)?;

    if max_shift == 0 {
        compare(&left, &right)
    } else {
        shift_scan(&left, &right, max_shift)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use iced::window;

    use super::*;
    use crate::shot::{save, write_rgba};

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join("ull-app-iced-shot-tests");
        std::fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    #[test]
    fn identical_images_have_zero_diff() {
        let image = Image {
            rgba: vec![7_u8; 4 * 4],
            width: 2,
            height: 2,
        };
        let diff = compare(&image, &image).unwrap();
        assert!(diff.is_identical());
        assert_eq!(diff.max_channel_delta, 0);
    }

    #[test]
    fn geometry_mismatch_is_an_error_not_a_silent_pass() {
        let square = Image {
            rgba: vec![0; 16],
            width: 2,
            height: 2,
        };
        let strip = Image {
            rgba: vec![0; 16],
            width: 4,
            height: 1,
        };
        assert!(compare(&square, &strip).is_err());
    }

    #[test]
    fn color_only_differences_are_counted_per_pixel() {
        let left = Image {
            rgba: vec![0, 0, 0, 255, 0, 0, 0, 255],
            width: 2,
            height: 1,
        };
        let right = Image {
            rgba: vec![10, 0, 0, 255, 0, 0, 0, 255],
            width: 2,
            height: 1,
        };
        let diff = compare(&left, &right).unwrap();
        assert_eq!(diff.differing_pixels, 1);
        assert_eq!(diff.max_channel_delta, 10);
        assert!(diff.describe().contains("differing=1/2"));
    }

    #[test]
    fn png_roundtrip_through_disk() {
        let path = temp_dir().join("roundtrip.png");
        let bytes = vec![200_u8; 4 * 3 * 2];
        write_rgba(&bytes, 3, 2, &path).unwrap();

        let loaded = Image::load(&path).unwrap();
        assert_eq!((loaded.width, loaded.height), (3, 2));
        assert_eq!(loaded.rgba, bytes);

        let diff = diff_files(&path, &path).unwrap();
        assert!(diff.is_identical());
    }

    #[test]
    fn shift_scan_finds_the_best_alignment() {
        // 5x5 图，R 通道存唯一值；构造 right[y][x] = left[y+1][x+1]（即把左图向左上挪 1px），
        // 于是「left 与 right 在 dy=1,dx=1 时逐像素相等」——扫描应该把 (1,1) 找出来。
        let side = 5_i32;
        let mut rgba = Vec::new();
        for y in 0..side {
            for x in 0..side {
                rgba.extend([(y * side + x) as u8, 0, 0, 255]);
            }
        }
        let left = Image {
            rgba,
            width: side as u32,
            height: side as u32,
        };

        let mut moved = vec![255_u8; (side * side * 4) as usize];
        for y in 0..side - 1 {
            for x in 0..side - 1 {
                let source = ((y + 1) * side + (x + 1)) as usize * 4;
                let target = (y * side + x) as usize * 4;
                moved[target..target + 4].copy_from_slice(&left.rgba[source..source + 4]);
            }
        }
        let right = Image {
            rgba: moved,
            width: side as u32,
            height: side as u32,
        };

        let diff = shift_scan(&left, &right, 2).unwrap();
        assert_eq!(diff.best_shift, (1, 1), "应扫出 1px 的位移");
        assert_eq!(diff.best_differing_pixels, 0, "对齐后应逐像素相等");
        assert!(
            diff.differing_pixels > 0,
            "不对齐时应该有差异（否则这条测试没有信息量）"
        );

        // 同一张图：最佳对齐就是 (0,0)
        let same = shift_scan(&left, &left, 2).unwrap();
        assert_eq!(same.best_shift, (0, 0));
        assert_eq!(same.best_differing_pixels, 0);
    }

    #[test]
    fn wrong_byte_count_is_reported() {
        let screenshot = window::Screenshot::new(vec![0_u8; 3], iced::Size::new(1, 1), 1.0);
        let error = save(&screenshot, &temp_dir().join("unused.png")).unwrap_err();
        assert!(error.contains("字节数对不上"), "{error}");
    }
}
