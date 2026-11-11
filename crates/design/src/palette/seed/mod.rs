//! 已解码的 RGBA 像素 → 一个**种子色**（D38 第一/二级的入口，D40 的 `crates/design` 半边）。
//!
//! 这是整个取色链在 `crates/design` 里的**唯一公开入口**：零 IO、零第三方依赖，
//! 只吃「调用方已经解码好的像素」。解码（PNG/JPEG/WebP…）在 `crates/palette-extract`，
//! 分层理由见 D40。
//!
//! ```no_run
//! use unlucky_design::color::{Scheme, SchemeFlavor};
//! use unlucky_design::palette::seed_from_rgba;
//!
//! // 一张 2×2 的天蓝图（调用方负责解码）
//! let pixels: [u8; 16] = [
//!     0x3C, 0x8D, 0xBC, 0xFF, 0x3C, 0x8D, 0xBC, 0xFF,
//!     0x3C, 0x8D, 0xBC, 0xFF, 0x3C, 0x8D, 0xBC, 0xFF,
//! ];
//! let seed = seed_from_rgba(&pixels, 2, 2).expect("纯色图有确定的种子");
//! let scheme = seed.scheme(SchemeFlavor::Light, 12.0);
//! assert!(scheme.contrast(unlucky_design::color::Role::OnPrimary, unlucky_design::color::Role::Primary) >= 4.5);
//! ```
//!
//! ## 五类输入的处置（每类都有单测）
//!
//! | 输入 | 处置 | 为什么 |
//! |---|---|---|
//! | 过小图（< 4 像素） | [`Failure::TooSmall`] | 1~3 个像素没有「主色」概念，任一个噪点都会变成整套配色 |
//! | 纯色图（有彩度） | 正常取色 | 单色图只要够鲜艳就是合格的种子（不做特殊化） |
//! | 灰度/灰白图 | [`Failure::LowChroma`]（**不是**返回青灰色） | D40 实测：零彩度种子的 CAM16 色相恒为 209.49°，硬取色会把灰白壁纸「凭空上色」成青色 `#006874`；极低彩度段色相还是噪声放大器（`#808080` → 209.5°、`#7A7C7E` → 231.2°，摆 22°） |
//! | 超大图（>65536 像素） | 先按 4^n 块做**块平均**降采样，再量化 | 取色只需要「色彩分布」，不需要分辨率；块平均能保住细纹理的平均色，避免等距抽样在条纹/网格图上走样 |
//! | 混合透明度（一半不透明一半全透明） | **先按 alpha 过滤、再块平均**（整块全透明则跳过） | 「先平均再比门槛」会把 `255/2 = 127 < 128` 判成全透明，于是 13 万个不透明像素被报成 `NoOpaquePixels`——复审实测踩到的坑 |
//! | 解码失败 | 不在本 crate：见 `crates/palette-extract` | D40 分层 |
//!
//! 阈值 [`MIN_SEED_CHROMA`] = 8 是 **D40 的定案值**（`seed chroma < ~8 视为无信息量`），
//! 与 [`super::score::CUTOFF_CHROMA`] = 5 分工不同：5 决定「能不能当候选」，8 决定
//! 「这个候选够不够格当种子」。落在这条缝里（5~8）的图会有一堆候选，最终仍然回落色卡。

use super::quantize;
use super::score;
use crate::color::{Color, Palettes, Scheme, SchemeFlavor};

/// 最小像素数：低于它的图没有「主色」可言。
pub const MIN_PIXELS: u32 = 4;
/// 参与量化的最大采样点数（块的边长按它反推）。
pub const MAX_SAMPLES: usize = 65_536;
/// D40 的种子彩度门槛：低于它视为「这张图没有可用的颜色信息」。
pub const MIN_SEED_CHROMA: f64 = 8.0;
/// 「低彩度但可用」的第二道线：落在这条线下的种子会被标成 [`Seed::is_muted`]，
/// UI 可以提示「从这张图取到的颜色偏灰」。
pub const MUTED_SEED_CHROMA: f64 = 16.0;
/// alpha 低于它的像素不参与取色（半透明叠加会让颜色变成「混了背景的假色」）。
const ALPHA_THRESHOLD: u8 = 128;

/// 取色结果：种子色 + 足以让 UI 解释「为什么是这个颜色」的观感数据。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seed {
    /// 种子色。
    ///
    /// **只对「传进 [`analyze_rgba`] 的那个缓冲」作保证**：像素数 ≤ [`MAX_SAMPLES`] 时不做块
    /// 平均，种子是缓冲里真实存在的颜色（代表色取「簇内像素最多的桶里像素原值平均」，
    /// 纯色图逐通道精确）；更大的缓冲会先块平均，此时可能是平均出来的新颜色。
    ///
    /// ⚠️ 整条链上**任何一次缩放**都会破坏「真实颜色」，不只是这一层：`crates/palette-extract`
    /// 在超过 `MAX_EDGE` 时先用 `thumbnail` 缩放。实测 400×400 红蓝棋盘（每像素交替）：
    /// `MAX_EDGE = 256`（适配层缩放）与 `MAX_EDGE = 512`（本层块平均）都取到 `#704070`，
    /// 这个色值在原图上**一个像素都不是**。降采样本来就要平均（顺带降噪），所以这是
    /// **有意的行为**；不要把它当「绝不出现平均色」的保证，也别拿它判断「这个色在壁纸里存在」。
    pub color: Color,
    /// 种子的 CAM16 色相角（度）。
    pub hue: f64,
    /// 种子的 CAM16 彩度。
    pub chroma: f64,
    /// 该色所在色相族占**有效采样点**的比例（0..1）。
    ///
    /// 分母是采样点（块平均后的点、且已剔除透明像素），不是全图像素数。所以只有
    /// 「无透明 + 像素数 ≤ [`MAX_SAMPLES`]」时它才等于「占全图比例」；大图（有降采样）
    /// 或有透明区域时只能说「占有效采样点的比例」。
    pub share: f64,
}

impl Seed {
    /// 种子彩度偏低（可用但发灰）——UI 可提示「这张图颜色较淡」。
    pub fn is_muted(self) -> bool {
        self.chroma < MUTED_SEED_CHROMA
    }

    /// 用这个种子生成一套完整调色板（中性彩度由调用方给，通常沿用用户当前选中色卡的取值）。
    pub fn palettes(self, neutral_chroma: f64) -> Palettes {
        Palettes::from_seed_with_neutral_chroma(self.color, neutral_chroma)
    }

    /// 用这个种子生成一套完整方案（49 个角色）。
    pub fn scheme(self, flavor: SchemeFlavor, neutral_chroma: f64) -> Scheme {
        Scheme::from_palettes(&self.palettes(neutral_chroma), flavor)
    }
}

/// 取色失败的原因。**每一种都要能变成一句人话**（Provenance 用它生成提示）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Failure {
    /// 像素缓冲长度与 `width × height × 4` 不符（调用方给错了数据）。
    BadDimensions {
        /// 宽。
        width: u32,
        /// 高。
        height: u32,
        /// 期望的字节数（`width × height × 4`）。
        expected: usize,
        /// 实际拿到的字节数。
        actual: usize,
    },
    /// 图太小（< [`MIN_PIXELS`] 像素）。
    TooSmall {
        /// 宽。
        width: u32,
        /// 高。
        height: u32,
    },
    /// 全是透明/半透明像素，没有可用的不透明像素。
    NoOpaquePixels,
    /// 彩度不足：`chroma < MIN_SEED_CHROMA`，硬取色会凭空上色。
    ///
    /// `color` 与 `chroma` **永远来自同一个簇**（复审 r1 p2-4.2 定的口径）：
    /// - 有候选（通过 `Score` 的彩度 ≥5 与人气 ≥1% 过滤）但最强候选仍 `< 8` →
    ///   报 `Score` **榜首**的色值与彩度（它是「本次能拿到的最好候选」）；
    /// - 一个候选都没有 → 报**彩度最高**的那一簇。
    ///   这条**不是「灰图专属」**：`Score` 除了彩度 ≥5 还要求所在色相桶的人气 ≥1%，
    ///   所以「高彩度但面积太小」也会走到这里。实测（复审探针）：40×40、1592 个米白
    ///   `#F5F1EA` + 8 个纯红 `#FF0000`（0.5% 面积）→ 报回来的彩度是 **113.36**
    ///   （`#FF0000` 的 CAM16 彩度，见 `color::hct` 的对账表）。
    ///
    /// ⚠️ 因此这个数字**不受候选门槛 5 约束**，可能远高于 8。UI 别把它写成
    /// 「彩度 X < 门槛 8」：真正的失败原因是「没有一族**面积够大**的颜色」。文案用
    /// 「颜色太淡 / 太杂」即可；要显示数字也只在 `chroma < CUTOFF_CHROMA(5)` 时显示
    /// （那时的含义才是「整张图没颜色」，即纯灰图）。
    LowChroma {
        /// 取色失败时最接近可用的候选色（提示文案里可以显示成一个小色块）。
        ///
        /// 注意它不一定「能用」：高彩度但面积很小的颜料（如 0.5% 面积的纯红）也会出现在这里，
        /// 见 [`Failure::LowChroma`] 的说明。
        color: Color,
        /// 该色的实测 CAM16 彩度（与 `color` 成对）。
        chroma: f64,
    },
}

impl Failure {
    /// 稳定原因码（写日志 / 存设置 / 做提示映射的键，别用 `Debug` 输出）。
    pub const fn code(self) -> &'static str {
        match self {
            Failure::BadDimensions { .. } => "bad-dimensions",
            Failure::TooSmall { .. } => "too-small",
            Failure::NoOpaquePixels => "no-opaque-pixels",
            Failure::LowChroma { .. } => "low-chroma",
        }
    }
}

/// 取色并给出**完整原因**（推荐入口：失败时能生成提示）。
pub fn analyze_rgba(pixels: &[u8], width: u32, height: u32) -> Result<Seed, Failure> {
    let total = (width as usize).saturating_mul(height as usize);
    if total < MIN_PIXELS as usize {
        return Err(Failure::TooSmall { width, height });
    }
    let expected = total.saturating_mul(4);
    if pixels.len() != expected {
        return Err(Failure::BadDimensions {
            width,
            height,
            expected,
            actual: pixels.len(),
        });
    }

    // 采样点已经是「只由不透明像素算出的块平均」（见 [`sample_blocks`]），这里不再按 alpha 过滤。
    let samples = sample_blocks(pixels, width as usize, height as usize);
    if samples.is_empty() {
        return Err(Failure::NoOpaquePixels);
    }

    let clusters = quantize::quantize(&samples);
    let ranked = score::rank(&clusters);
    match ranked.first() {
        Some(best) if best.chroma >= MIN_SEED_CHROMA => Ok(Seed {
            color: best.cluster.color,
            hue: best.hue,
            chroma: best.chroma,
            share: best.excited,
        }),
        // 有候选但都不够鲜艳：报 `Score` 榜首的色值与彩度（`Score` 已经按人气 × 彩度排过序，
        // 它就是「本次能拿到的最好候选」）。
        Some(best) => Err(Failure::LowChroma {
            color: best.cluster.color,
            chroma: best.chroma,
        }),
        // 一个候选都没有：报**彩度最高**的那一族，色值与彩度成对。
        // 这不等于「整张图没颜色」——1592 米白 + 8 个纯红的图也走这里（红色面积 0.5% 被人气
        // 门槛淘汰），报回来的彩度是 113.36。真实失败原因是「没有一族面积够大的颜色」。
        None => {
            let closest = score::most_chromatic(&clusters);
            Err(Failure::LowChroma {
                color: closest
                    .map(|cluster| cluster.color)
                    .unwrap_or(Color::rgb(0x80, 0x80, 0x80)),
                chroma: closest
                    .map(|cluster| crate::color::Hct::from_color(cluster.color).chroma)
                    .unwrap_or(0.0),
            })
        }
    }
}

/// D40 的入口形状：`Option<Seed>`（`None` = 这张图没有可用的颜色信息）。
///
/// 需要知道「为什么没有」就用 [`analyze_rgba`]。
pub fn seed_from_rgba(pixels: &[u8], width: u32, height: u32) -> Option<Seed> {
    analyze_rgba(pixels, width, height).ok()
}

/// 块平均降采样：块边长按「不超过 [`MAX_SAMPLES`] 个采样点」反推，小图不降采样（块边长 1）。
///
/// **先按 alpha 过滤、再求平均**：块内只累加 `a >= ALPHA_THRESHOLD` 的像素，整块没有有效像素
/// 就直接跳过（不产出采样点）。
///
/// 为什么不能「先平均、再拿平均 alpha 跟门槛比」：块内 alpha 用整数除法截断，
/// 一半不透明一半透明的块平均下来是 `255/2 = 127 < 128`，于是**整张图被判成「没有不透明像素」**
/// （复审实测：512×512 里 131072 个完全不透明像素 → `Err(NoOpaquePixels)`）。
/// 副作用是「半透明像素的颜色」也被排除在外——这是有意的：半透明像素的 RGB 常常是
/// 未预乘的原始值（有些解码器给的是 0,0,0），混进来会把种子拽向黑色。
fn sample_blocks(pixels: &[u8], width: usize, height: usize) -> Vec<Color> {
    let mut block = 1usize;
    while width.div_ceil(block) * height.div_ceil(block) > MAX_SAMPLES {
        block += 1;
    }

    let mut samples = Vec::with_capacity(width.div_ceil(block) * height.div_ceil(block));
    let mut top = 0usize;
    while top < height {
        let bottom = (top + block).min(height);
        let mut left = 0usize;
        while left < width {
            let right = (left + block).min(width);
            let mut sum = [0u64; 3];
            let mut count = 0u64;
            for y in top..bottom {
                for x in left..right {
                    let offset = (y * width + x) * 4;
                    if pixels[offset + 3] < ALPHA_THRESHOLD {
                        continue;
                    }
                    for channel in 0..3 {
                        sum[channel] += u64::from(pixels[offset + channel]);
                    }
                    count += 1;
                }
            }
            if count > 0 {
                let average = |channel: usize| (sum[channel] / count) as u8;
                samples.push(Color::rgb(average(0), average(1), average(2)));
            }
            left = right;
        }
        top = bottom;
    }
    samples
}

#[cfg(test)]
mod tests;
