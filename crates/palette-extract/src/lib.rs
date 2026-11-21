//! # 配色取色的解码适配层（D40 三层里的第 2 层）
//!
//! 本 crate 只做一件事：**图片路径 / 字节 → RGBA 像素 → `unlucky_design` 的种子色**。
//!
//! ```no_run
//! use unlucky_palette_extract::{input, load};
//! use unlucky_design::palette::{resolve_source, CardKey};
//! use unlucky_design::color::SchemeFlavor;
//!
//! // ① 读用户设的启动器背景图（失败不 panic，失败状态会原样进 Provenance）
//! let background = load("D:/Pictures/bg.png");
//! let (scheme, provenance) = resolve_source(
//!     Some(input(&background)),
//!     None,                       // ② 桌面壁纸：本次不启用
//!     Some(CardKey::Sky),         // ③ 用户选中的色卡 = 回落目标
//!     SchemeFlavor::Light,
//! );
//! println!("{:?} / {:?}", provenance.level, provenance.hint_key());
//! ```
//!
//! ## 为什么解码要单独成 crate（D40）
//!
//! `crates/design` 的卖点是「零第三方依赖」（`cargo tree -p unlucky-design` 单行），而图片解码
//! 必然拉依赖。把 `image` 塞进 design 会让那句卖点变成假话，所以：
//!
//! | 层 | crate | 依赖 | 职责 |
//! |---|---|---|---|
//! | 1 纯数学 | `unlucky-design` | 无 | 像素/种子 → 49 角色方案、色卡、回落决策、对比度收敛 |
//! | 2 解码 | **本 crate** | `image`（png + jpeg） | 路径/字节 → RGBA 像素；限尺寸；错误分类 |
//! | 3 调用侧 | 后续 appearance/settings mission | UI 框架 | 拿路径、把 `Provenance` 变文案 |
//!
//! ## 为什么选 `image`（而不是 `png` + `zune-jpeg` 各接一个）
//!
//! 1. **仓库里已经有它**：`crates/app-iced` 的截图通道用 `image`（png）。同一份依赖只编译一次，
//!    版本由 workspace 的 `image = "0.25"` 统一钉住，`cargo deny` 的许可面不新增条目。
//! 2. **格式嗅探现成**：`ImageReader::with_guessed_format()` 按魔数判格式，用户把 `.jpg`
//!    改成 `.png` 也能读（壁纸文件被改名的概率不低）。
//! 3. **有 `Limits`**：可以在解码前拒绝「头里写着 40000×40000」的 PNG（压缩炸弹），
//!    单接 `png` crate 也要自己写这层判断。
//!
//! 许可：`image` 是 `MIT OR Apache-2.0`，在 `deny.toml` 白名单内（`[licenses].allow`）。
//!
//! ## 体积影响（`image` 只开 png + jpeg + bmp，关掉 default-features）
//!
//! 拉进的传递依赖就这几个：`png`（+ `fdeflate`/`miniz_oxide`/`crc32fast`/`flate2` 已在树里）、
//! `zune-jpeg` + `zune-core`、`byteorder-lite`、`num-traits`、`moxcms`/`pxfm`（色彩转换）。
//! `bmp` 是 `image` 的独立 feature 且**不带任何传递依赖**（`bmp = []`），加它只是因为
//! M73 的 Windows 壁纸探测不筛扩展名、`.bmp` 壁纸真会出现。
//! 关掉的默认特性里有 `rayon`（省一个线程池）、`avif`/`ravif`（最大的一个，编解码器整套）、
//! `webp`/`tiff`/`gif`/`exr`/`qoi`/`hdr`/`dds`/`ff`。实测增量见 M87 报告（`cargo tree` 前后对比）。
//!
//! ## 两条硬纪律
//!
//! 1. **解码后才交出去**：送进 `unlucky_design` 的像素最多 [`MAX_EDGE`] 边长（默认 256）。
//!    取色只需要色彩分布，不需要分辨率；一张 4K 壁纸缩到 256×144 后量化结果与全分辨率
//!    在肉眼层面一致，内存却从 33MB 掉到 147KB。`crates/design` 里还有第二道块平均降采样
//!    （防御式：直接调 design 的调用方也能拿到有界开销）。
//! 2. **失败不 panic、不 return 默认色**：解码失败只变成 [`ExtractError`]，
//!    [`input`] 把它翻成 `ImageInput::Undecodable` 交给 `resolve_source`，
//!    由后者回落色卡并在 `Provenance` 里留下原因。**不许**在这里「读不到就给个蓝色」。

use std::io::Cursor;
use std::path::Path;

use unlucky_design::palette::{self, ImageInput, ImageSource, Seed};

mod thumbnail;

pub use thumbnail::{thumbnail, THUMBNAIL_MAX_EDGE};

/// 交给 `unlucky_design` 前允许的最大边长（像素）。
///
/// 256 的依据：量化器自己只采 65536 个点（≈256²），再大的图在 `crates/design` 里也会被
/// 块平均降采样；提前缩小只是省内存与解码时间。可预测、可测试（见 `tests.rs`）。
pub const MAX_EDGE: u32 = 256;

/// 解码**之前**允许的图片头尺寸上限。
///
/// 4K 壁纸是 3840×2160，8K 是 7680×4320；16384 留出 2 倍余量，同时挡住「头里写着
/// 40000×40000」的解压炸弹（否则解码器会先按头分配几十 GB）。
const MAX_SOURCE_EDGE: u32 = 16_384;

/// 解码后的 RGBA8 图像（行主序，`width × height × 4` 字节）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba8 {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Rgba8 {
    /// 宽（像素）。
    pub fn width(&self) -> u32 {
        self.width
    }

    /// 高（像素）。
    pub fn height(&self) -> u32 {
        self.height
    }

    /// 原始 RGBA8 字节（`width × height × 4`）。
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// 包成 `unlucky_design` 的借用视图（零拷贝，供 `resolve_source` 用）。
    pub fn view(&self) -> ImageSource<'_> {
        ImageSource {
            pixels: &self.pixels,
            width: self.width,
            height: self.height,
        }
    }
}

/// 解码/取色失败的原因。
#[derive(Debug)]
pub enum ExtractError {
    /// 读文件失败（不存在、无权限、被占用）。
    Io(std::io::Error),
    /// 解码失败（格式不认识、文件截断、超出 [`MAX_SOURCE_EDGE`] 限制）。
    Decode(image::ImageError),
    /// 解码成功但取色失败（图太小、全透明、彩度不足……原因在 `unlucky_design`）。
    Palette(palette::Failure),
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractError::Io(error) => write!(f, "读取图片失败：{error}"),
            ExtractError::Decode(error) => write!(f, "解码图片失败：{error}"),
            ExtractError::Palette(failure) => {
                write!(f, "取色失败（{}）：{failure:?}", failure.code())
            }
        }
    }
}

impl std::error::Error for ExtractError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ExtractError::Io(error) => Some(error),
            ExtractError::Decode(error) => Some(error),
            ExtractError::Palette(_) => None,
        }
    }
}

/// 从内存字节解码（格式按魔数嗅探，不看扩展名）。
pub fn decode_bytes(bytes: &[u8]) -> Result<Rgba8, ExtractError> {
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(ExtractError::Io)?;
    finish(reader)
}

/// 从文件路径解码（格式按魔数嗅探，不看扩展名）。
pub fn load(path: impl AsRef<Path>) -> Result<Rgba8, ExtractError> {
    let reader = image::ImageReader::open(path).map_err(ExtractError::Io)?;
    let reader = reader.with_guessed_format().map_err(ExtractError::Io)?;
    finish(reader)
}

/// 路径 → 种子色（解码 + 量化一步到位；只需要 bool「能不能取」时用 [`input`]）。
pub fn extract_seed(path: impl AsRef<Path>) -> Result<Seed, ExtractError> {
    seed_from_rgba8(&load(path)?)
}

/// 字节 → 种子色（与 [`extract_seed`] 同一条路径，测试与「内存里的图」用）。
pub fn seed_from_bytes(bytes: &[u8]) -> Result<Seed, ExtractError> {
    seed_from_rgba8(&decode_bytes(bytes)?)
}

fn seed_from_rgba8(image: &Rgba8) -> Result<Seed, ExtractError> {
    palette::analyze_rgba(image.pixels(), image.width(), image.height())
        .map_err(ExtractError::Palette)
}

/// 把「读到了 / 没读到」翻成 `resolve_source` 的输入。
///
/// 这是本 crate 与编排层之间**唯一**的接线点：`Err` → `ImageInput::Undecodable`
/// （回落链会据此写 `Skip::Undecodable` 而不是 `Skip::Absent`——两者对用户的含义不同：
/// 「你没设背景图」 vs 「你的背景图坏了」）。
pub fn input<'a>(loaded: &'a Result<Rgba8, ExtractError>) -> ImageInput<'a> {
    match loaded {
        Ok(image) => ImageInput::Pixels(image.view()),
        Err(_) => ImageInput::Undecodable,
    }
}

fn finish<R: std::io::BufRead + std::io::Seek>(
    reader: image::ImageReader<R>,
) -> Result<Rgba8, ExtractError> {
    Ok(fit(decode_with_limits(reader)?, MAX_EDGE))
}

/// 解码一个 reader（带 [`MAX_SOURCE_EDGE`] 头尺寸上限），**不缩放**。
///
/// 从 [`finish`] 里拆出来只有一个理由：取色（[`load`]）与缩略图（[`thumbnail`]）要**不同的
/// 目标边长**，而「读文件 + 嗅探格式 + 卡解压炸弹」这段必须只有一份（两份就会有一份忘了卡上限）。
pub(crate) fn decode_with_limits<R: std::io::BufRead + std::io::Seek>(
    mut reader: image::ImageReader<R>,
) -> Result<image::RgbaImage, ExtractError> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_EDGE);
    limits.max_image_height = Some(MAX_SOURCE_EDGE);
    reader.limits(limits);

    let decoded = reader.decode().map_err(ExtractError::Decode)?;
    Ok(decoded.to_rgba8())
}

/// 把解码结果等比缩到**长边 ≤ `max_edge`**（不放大：本来就小的图原样交付）。
///
/// `thumbnail` 是整数盒式采样：对取色（只关心色彩分布）与缩略图（160px 的墙）都够，
/// 且比 Lanczos 快一个量级。
pub(crate) fn fit(rgba: image::RgbaImage, max_edge: u32) -> Rgba8 {
    let (width, height) = (rgba.width(), rgba.height());
    let scaled = if width.max(height) > max_edge {
        let factor = f64::from(max_edge) / f64::from(width.max(height));
        let target_width = ((f64::from(width) * factor).round() as u32).max(1);
        let target_height = ((f64::from(height) * factor).round() as u32).max(1);
        image::imageops::thumbnail(&rgba, target_width, target_height)
    } else {
        rgba
    };
    Rgba8 {
        width: scaled.width(),
        height: scaled.height(),
        pixels: scaled.into_raw(),
    }
}

#[cfg(test)]
mod tests;
