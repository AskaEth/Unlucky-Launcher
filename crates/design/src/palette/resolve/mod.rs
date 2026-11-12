//! 三级来源编排：**纯函数**，一次调用算完「用哪一级的颜色、为什么不是别的级」。
//!
//! D38 的优先序：① 启动器背景图 → ② 桌面壁纸 → ③ 内置色卡。
//! 本模块不做任何 IO：像素是调用方解码好传进来的（D40 分层），壁纸路径探测是 M73 的事。
//!
//! ```no_run
//! use unlucky_design::palette::{resolve_source, CardKey, ImageInput, ImageSource, Level};
//! use unlucky_design::color::SchemeFlavor;
//!
//! let pixels = [0x3C, 0x8D, 0xBC, 0xFF].repeat(16); // 4×4 天蓝图（示例）
//! let bg = ImageInput::Pixels(ImageSource { pixels: &pixels, width: 4, height: 4 });
//! let (scheme, provenance) = resolve_source(Some(bg), None, Some(CardKey::Forest), SchemeFlavor::Light);
//! assert_eq!(provenance.level, Level::BackgroundImage);
//! assert!(provenance.hint_key().is_none(), "一级成功时没有提示");
//! ```
//!
//! ## 失败链：每一级都可能「有输入但不合格」
//!
//! | 情形 | 结果 | `Provenance` 里留下 |
//! |---|---|---|
//! | 没有背景图 | 跳到 ② | `Outcome::Absent` |
//! | 更高级已经成功 | 不再看这一级 | `Outcome::NotReached`（**不等于**「用户没提供」） |
//! | 背景图解码失败 | 跳到 ② | `Outcome::Failed(Skip::Undecodable)` |
//! | 背景图太小 / 像素缓冲不对 / 全透明 | 跳到 ② | `… TooSmall / BadDimensions / NoOpaquePixels` |
//! | 背景图彩度不足（D40 的 `< 8`） | 跳到 ② | `… LowChroma { chroma }` |
//! | 背景图取色成功但对比度收敛失败 | 跳到 ② | `… Contrast { failures }`（理论上不会发生，仍要留） |
//! | 三级都不成立 | 用色卡，**一定**有结果 | `Level::Preset` |
//!
//! 三条纪律：
//!
//! 1. **不许静默**。任何一次回落都在 `Provenance` 里留下原因码，[`Provenance::hint_key`]
//!    给 UI 一个稳定的提示键（文案映射见 `DevDocs/SubAgentsTalk/palette-source-contract.md`）。
//! 2. **不许静默变黑白**。彩度不足的图**不**会被「压成灰方案」，而是直接回落到用户选的那张色卡。
//! 3. **不许静默换成默认卡**。回落的落点是「用户当前选中的卡」（参数 `preset`），
//!    只有用户从没选过卡（`None`）才用 [`super::DEFAULT_CARD`]。
//!
//! ## 文件划分
//!
//! [`provenance`] 放「结果解释」的数据类型（`Level` / `Outcome` / `Skip` / `Provenance`），
//! 本文件放编排函数与测试。拆开只是为了单文件 ≤400 行的纪律，对外路径仍是
//! `palette::resolve::*`。

use super::card::{Card, CardKey, DEFAULT_CARD};
use super::seed::{analyze_rgba, Seed};
use super::wcag::{check, converge, Adjustment};
use crate::color::{Scheme, SchemeFlavor};

mod provenance;

pub use provenance::{Level, Outcome, Provenance, Skip};

/// 已解码的图片：借用像素，不拷贝（解码在 `crates/palette-extract`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSource<'a> {
    /// RGBA8 像素，`width × height × 4` 字节。
    pub pixels: &'a [u8],
    /// 宽（像素）。
    pub width: u32,
    /// 高（像素）。
    pub height: u32,
}

/// 某一级来源的输入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageInput<'a> {
    /// 解码好的像素。
    Pixels(ImageSource<'a>),
    /// 有这张图，但解码失败（文件损坏 / 格式不支持 / 读不到）。
    ///
    /// 这个状态由适配层传进来——不传就等于「没有这张图」，UI 会说错话。
    Undecodable,
}

impl<'a> ImageInput<'a> {
    /// 便捷构造：从像素缓冲包一个输入。
    pub fn pixels(pixels: &'a [u8], width: u32, height: u32) -> Self {
        ImageInput::Pixels(ImageSource {
            pixels,
            width,
            height,
        })
    }
}
/// 三级来源编排（纯函数）。
///
/// 参数：
/// - `background`：用户设的启动器背景图（`None` = 没设）。解码失败传
///   [`ImageInput::Undecodable`]，**不要**传 `None`（那样 UI 分不清「没设」和「坏了」）。
/// - `wallpaper`：桌面壁纸（`None` = 没开莫奈 / 探测不到壁纸文件）。
/// - `preset`：用户当前选中的色卡（`None` = 从没选过 → 用 [`super::DEFAULT_CARD`]）。
/// - `flavor`：深浅模式。
///
/// 返回值永远有一套可用方案：三级全失败时落到色卡，并带上原因。
pub fn resolve_source(
    background: Option<ImageInput<'_>>,
    wallpaper: Option<ImageInput<'_>>,
    preset: Option<CardKey>,
    flavor: SchemeFlavor,
) -> (Scheme, Provenance) {
    let card = preset.map(Card::from).unwrap_or(DEFAULT_CARD);
    // 先假定「没轮到」，逐级评估时再改成 Absent / Failed / Used。
    let mut outcomes = [Outcome::NotReached; 3];

    for (level, input) in [
        (Level::BackgroundImage, background),
        (Level::Wallpaper, wallpaper),
    ] {
        match try_level(input, card, flavor) {
            Attempt::Absent => outcomes[level.index()] = Outcome::Absent,
            Attempt::Failed(skip) => outcomes[level.index()] = Outcome::Failed(skip),
            Attempt::Used {
                scheme,
                seed,
                adjustments,
            } => {
                outcomes[level.index()] = Outcome::Used;
                return (
                    *scheme,
                    Provenance {
                        level,
                        card: card_key(card),
                        seed: Some(seed),
                        flavor,
                        adjustments,
                        outcomes,
                    },
                );
            }
        }
    }

    // 第三级：色卡。收敛仍然要跑（色卡也可能因为将来改常数而越线），但落点是它自己。
    let (scheme, adjustments) = converge(card.scheme(flavor));
    let report = check(&scheme);
    outcomes[Level::Preset.index()] = if report.is_clean() {
        Outcome::Used
    } else {
        Outcome::Failed(Skip::Contrast {
            failures: report.failures.len(),
        })
    };
    (
        scheme,
        Provenance {
            level: Level::Preset,
            card: card_key(card),
            seed: None,
            flavor,
            adjustments,
            outcomes,
        },
    )
}

/// 单级的处理结果。
///
/// `scheme` 装箱只是为了枚举大小（`Scheme` 是 49 个色值 ≈ 200 字节，逐级在栈上传会白白拷两次）；
/// 这条路径每次配色只走一次，堆分配的开销可忽略。
enum Attempt {
    Absent,
    Failed(Skip),
    Used {
        scheme: Box<Scheme>,
        seed: Seed,
        adjustments: Vec<Adjustment>,
    },
}

fn try_level(input: Option<ImageInput<'_>>, card: Card, flavor: SchemeFlavor) -> Attempt {
    let source = match input {
        None => return Attempt::Absent,
        Some(ImageInput::Undecodable) => return Attempt::Failed(Skip::Undecodable),
        Some(ImageInput::Pixels(source)) => source,
    };

    let seed = match analyze_rgba(source.pixels, source.width, source.height) {
        Ok(seed) => seed,
        Err(failure) => return Attempt::Failed(Skip::from(failure)),
    };

    // 取色方案的表面色**跟着图像种子染色**，只借当前色卡的中性彩度定浓淡（D43）：
    // 这是与色卡路径刻意不同的一处——色卡固定中性色相、只换强调色，取色保留整幅染色
    // （那是用户要的 Monet 体验）。别把两条路径「统一处理」：这条接线的守护测试是
    // `tests/card_neutral.rs::resolved_image_scheme_keeps_tinting_from_the_seed`（走的就是
    // 本函数所在的 `resolve_source`；只测 `Seed::scheme` 的那条覆盖不到这里，复审 r1 p1-1）。
    let (scheme, adjustments) = converge(seed.scheme(flavor, card.neutral_chroma));
    let report = check(&scheme);
    if !report.is_clean() {
        return Attempt::Failed(Skip::Contrast {
            failures: report.failures.len(),
        });
    }
    Attempt::Used {
        scheme: Box::new(scheme),
        seed,
        adjustments,
    }
}

fn card_key(card: Card) -> CardKey {
    CardKey::from_key(card.key).unwrap_or(CardKey::Sky)
}

#[cfg(test)]
mod tests;
