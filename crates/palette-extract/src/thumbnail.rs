//! 缩略图：路径 → 限尺寸的 [`Rgba8`]（D54 第 1 条，M251 · SAVESHOTS）。
//!
//! ## 为什么落在本 crate
//!
//! D54 定的分层没有变：图片解码**集中在 `palette-extract`**（`crates/design` 零第三方依赖的
//! 代价就是把解码摊到这里；`app-iced` 另有一份 `image` 只用于截图编码与背景图，见它自己的
//! `Cargo.toml` 注释）。截图是 PNG，本 crate 的 `image` 已经开了 `png`/`jpeg`/`bmp` 解码 feature，
//! 所以加这条路**零新依赖、零新 crate、零新 feature**——只是把「解码 + 限尺寸」这段已有的能力
//! 换个目标边长（[`THUMBNAIL_MAX_EDGE`]）暴露一次。
//!
//! ## 不落盘
//!
//! 这里**只返回像素**，不写任何缓存文件（D54 第 2 条）：写 PNG 需要编码器，而编码器只为一份
//! 磁盘缓存引入不划算；内存缓存归调用方（presenter 的 `host::saves::thumbs`）。
//!
//! ## 失败不 panic
//!
//! 读不到 / 不是图片 / 头里写着 40000×40000，全都变成 [`ExtractError`]（与取色那条路同一族
//! 错误）。调用方据此给「逐条可见」的失败原因——**不许**在这里「读不到就画个灰块」。
//!
//! 与 [`crate::load`] 的区别只有一条：目标边长。`load` 钉 `MAX_EDGE = 256`（取色只需要色彩分布），
//! 这里由调用方给（缩略图墙用 160）。
//!
//! ```no_run
//! use unlucky_palette_extract::{thumbnail, THUMBNAIL_MAX_EDGE};
//!
//! match thumbnail("screenshots/2024-05-01_10.00.00.png", THUMBNAIL_MAX_EDGE) {
//!     Ok(thumb) => println!("{}×{}", thumb.width(), thumb.height()),
//!     Err(error) => eprintln!("这张图没有缩略图：{error}"),
//! }
//! ```

use std::path::Path;

use crate::{decode_with_limits, fit, ExtractError, Rgba8};

/// 缩略图的最大边（像素）。D54 定 160。
///
/// 依据：一屏缩略图墙按 4 列排、详情列宽约 700px ⇒ 每格 ≈ 160px。再大只是多占内存
/// （160² × 4B ≈ 100KB/张，60 张 ≈ 6MB），肉眼在墙上看不出差别。
pub const THUMBNAIL_MAX_EDGE: u32 = 160;

/// 从文件路径解码并缩到**长边 ≤ `max_edge`**（等比、不放大），返回 RGBA8 像素。
///
/// 格式按**魔数**嗅探，不看扩展名（与 [`crate::load`] 同一条路：用户把 `.jpg` 改名成 `.png`
/// 也照样读得出来）。`max_edge = 0` 按 1 处理（防调用方传 0 得到 0×0 的图）。
///
/// # Errors
///
/// - [`ExtractError::Io`]：文件不存在 / 无权限 / 被占用 / 打不开；
/// - [`ExtractError::Decode`]：不是认得的图片、文件截断、或头尺寸超过 16384（解压炸弹）。
pub fn thumbnail(path: impl AsRef<Path>, max_edge: u32) -> Result<Rgba8, ExtractError> {
    let max_edge = max_edge.max(1);
    let reader = image::ImageReader::open(path).map_err(ExtractError::Io)?;
    let reader = reader.with_guessed_format().map_err(ExtractError::Io)?;
    Ok(fit(decode_with_limits(reader)?, max_edge))
}
