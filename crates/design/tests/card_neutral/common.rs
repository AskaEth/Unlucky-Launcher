//! `card_neutral` 测试组共用的帮手：改前路径的复原、模式列表、色相环距离。
//!
//! 拆成子模块是为了单文件 ≤400 行的纪律（`card_neutral.rs` 是 crate 根，`common` / `dump`
//! 只是它的子模块；cargo 只把 `tests/card_neutral.rs` 当一个 test target）。

use unlucky_design::color::{Palettes, Scheme, SchemeFlavor};
use unlucky_design::palette::Card;

/// 改前的路径（M87 及之前）：中性调色板跟着种子染色（官方 TonalSpot）。
///
/// 产品代码已经不用它了，但公开 API 仍在（取色路径就靠它逐色染色），所以这里能原样复现「改前」。
pub fn before_d43(card: Card, flavor: SchemeFlavor) -> Scheme {
    Scheme::from_palettes(
        &Palettes::from_seed_with_neutral_chroma(card.seed, card.neutral_chroma),
        flavor,
    )
}

pub fn flavors() -> [SchemeFlavor; 2] {
    [SchemeFlavor::Light, SchemeFlavor::Dark]
}

/// 色相环上的最短距离（度）。
pub fn hue_gap(a: f64, b: f64) -> f64 {
    let raw = (a - b).abs();
    raw.min(360.0 - raw)
}
