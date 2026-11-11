//! 内置色卡（D38 第三级）：用户在「关闭莫奈取色」时手动选的那几张。
//!
//! ## `key` 是持久化键，命名就此定死
//!
//! 每张卡的 [`Card::key`] 会被写进用户设置（settings 里存字符串），**改了就等于把用户的选择
//! 抹成出厂默认**。所以：
//!
//! - 只用小写 ASCII + 单段单词，不带动词、不带颜色数值（`sky`，不是 `blue_3c8dbc`）；
//! - 显示名（[`Card::name`]）可以改、可以本地化，`key` 不许改；
//! - 新卡只能**追加**，旧卡永不删除（删了老用户的选择会静默失效 → 见
//!   [`CardKey::from_key`] 的回落口径）；
//! - `card.rs` 的 `card_keys_are_stable_and_unique` 把 8 个 key 逐字钉住——谁改谁红
//!   （测试就在本文件末尾的 `tests` 模块里，没有独立的 `palette/tests.rs`）。
//!
//! ## 换卡只换强调色：中性色相固定（D43）
//!
//! 八张卡共用**一个**中性色相 [`NEUTRAL_HUE_DEG`]（= 出厂默认种子 `#3C8DBC` 的 CAM16 色相
//! 239.51°，出处见该常量的文档），中性彩度都取 [`DEFAULT_NEUTRAL_CHROMA`]（12，M70 的「浅冷」口径）。
//! 于是这份契约是逐值可验的：
//!
//! - **表面与描边**（`surface` / `surface-container-*` / `background` / `inverse-surface` /
//!   `outline` / `outline-variant` / `surface-variant`）**八张卡逐值相同**——切卡不动窗口底色；
//! - **只有强调族**（`primary` / `secondary` / `tertiary` 及其 container、`on-*`、`inverse-primary`、
//!   `*-fixed*`、`surface-tint`）跟着卡走；
//! - **天蓝卡（出厂默认）与 [`crate::color::Scheme::baseline`] 逐值相同**：固定的中性色相本来
//!   就等于天蓝档的中性色相，所以这是「沿用」而非「另选一个冷调」（M70 定案的不动点）。
//!
//! 为什么不再让中性色跟着种子染色：TonalSpot 下八张卡的中性色相 = 种子色相，浅色档 `surface`
//! 实测 CAM16 彩度从 **1.29（rose）跨到 12.03（teal）**——tone 98 处暖相色域装不下彩度 12，
//! 同一个「请求值」在暖相上只兑现 1.29。9 倍的底色跳变让用户在设置页逐张试色时觉得「每张卡
//! 都像换了个应用」（M87 实测，D43 据此定案）。现在八张卡的 `surface` 是同一个色值，
//! 那 9 倍差由 `tests/card_neutral.rs` 断言为 1 倍。
//!
//! ⚠️ **只对色卡路径成立**：动态取色（启动器背景图 / 桌面壁纸）仍然让中性色跟着图像种子染色，
//! 那是用户要的 Monet 体验（D38）。见 [`super::resolve`] 的 `try_level`：两条路径刻意不同，
//! 别「统一处理」（`tests/card_neutral.rs` 有反证用例）。
//!
//! ## 卡与取色的关系（D38）
//!
//! 取色成功时，**当前选中的卡仍然参与生成**，但只贡献中性**彩度**（表面色的浓淡）；
//! 色相由图像种子决定。所以卡片不是「只在第三级才用到」的备胎，它的固定中性色相
//! **不会**出现在取色方案里。

use super::wcag;
use crate::color::{
    Color, Palettes, Scheme, SchemeFlavor, DEFAULT_NEUTRAL_CHROMA, NEUTRAL_HUE_DEG,
};

/// 一张内置色卡。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Card {
    /// 持久化键（写进用户设置，**不许改**）。
    pub key: &'static str,
    /// 显示名（可改、可本地化）。
    pub name: &'static str,
    /// 种子色（只贡献色相；Tonal Spot 会把彩度收到 36）。
    pub seed: Color,
    /// 中性调色板彩度（表面色的浓淡；色相固定为 [`NEUTRAL_HUE_DEG`]，见模块文档的 D43 契约）。
    pub neutral_chroma: f64,
}

impl Card {
    /// 用这张卡生成一套方案（与取色路径同一条生成器，只差两处入参：中性色相固定为
    /// [`NEUTRAL_HUE_DEG`]、强调色来自卡而不是图像）。
    pub fn scheme(self, flavor: SchemeFlavor) -> Scheme {
        Scheme::from_palettes(
            &Palettes::accent_on_fixed_neutral(self.seed, NEUTRAL_HUE_DEG, self.neutral_chroma),
            flavor,
        )
    }

    /// 这张卡的 `on-primary` 对 `primary` 的实测对比度。
    ///
    /// 设置页可以直接显示成「对比度 {:.2}:1」。**2026-11 实测（8 张卡）**：
    /// 浅色档 **6.43~6.49**、深色档 **7.70~7.75**——两种模式都远高于 WCAG AA 的 4.5:1。
    ///
    /// 这两个区间**被测试钉住**（不是口头声明）：`every_card_scheme_meets_wcag_aa` 对每张卡
    /// 断言「下限 [`wcag::AA_TEXT`] + 落在上述区间内」，所以将来改种子/调色板常数把比值
    /// 拖出区间会当场红——文档里的数字与测试同源。要改数值请连测试区间一起改。
    pub fn primary_contrast(self, flavor: SchemeFlavor) -> f32 {
        let scheme = self.scheme(flavor);
        scheme.contrast(crate::color::Role::OnPrimary, crate::color::Role::Primary)
    }
}

/// 全部内置色卡（下标 0 = 出厂默认）。
///
/// 色相在 CAM16 色环上大致均匀铺开（蓝 240 / 紫 299 / 绿 155 / 橙 65 / 红 10 / 青 185 /
/// 暮蓝 ~275 / 橄榄绿 ~110），让用户「随便挑一张都不会撞色」。`palette::card` 的单测
/// `cards_are_visually_distinct` 钉住「相邻色相间隔 ≥ 20°」。
pub const CARDS: [Card; 8] = [
    Card {
        key: "sky",
        name: "天蓝",
        // M70 定案 ④（`palettes::DEFAULT_SEED` 同值）。改这张卡 = 改产品默认色，见 M70 文档。
        seed: Color::rgb(0x3C, 0x8D, 0xBC),
        neutral_chroma: DEFAULT_NEUTRAL_CHROMA,
    },
    Card {
        key: "lavender",
        name: "薰衣草紫",
        // 官方 M3 baseline 种子，和 `Scheme::official_baseline` 同色，便于对照官方截图。
        seed: Color::rgb(0x67, 0x50, 0xA4),
        neutral_chroma: DEFAULT_NEUTRAL_CHROMA,
    },
    Card {
        key: "forest",
        name: "森林绿",
        seed: Color::rgb(0x2E, 0x7D, 0x4F),
        neutral_chroma: DEFAULT_NEUTRAL_CHROMA,
    },
    Card {
        key: "amber",
        name: "琥珀橙",
        seed: Color::rgb(0xB2, 0x6A, 0x00),
        neutral_chroma: DEFAULT_NEUTRAL_CHROMA,
    },
    Card {
        key: "rose",
        name: "玫瑰红",
        seed: Color::rgb(0xB2, 0x3A, 0x48),
        neutral_chroma: DEFAULT_NEUTRAL_CHROMA,
    },
    Card {
        key: "teal",
        name: "青碧",
        seed: Color::rgb(0x00, 0x79, 0x6B),
        neutral_chroma: DEFAULT_NEUTRAL_CHROMA,
    },
    Card {
        key: "dusk",
        name: "暮蓝",
        seed: Color::rgb(0x4A, 0x5A, 0x9C),
        neutral_chroma: DEFAULT_NEUTRAL_CHROMA,
    },
    Card {
        key: "olive",
        name: "橄榄绿",
        // 刻意**没有**「纯灰/石板灰」卡：D40 已实测，零彩度种子的 CAM16 色相恒为 209.49°
        // （不是「无色」，而是固定的青），极低彩度段色相还是噪声放大器。所以「想要无彩色界面」
        // 不能靠一张灰卡实现——那需要一个饱和度/对比度维度（未来若做，是另一条轴，不是色卡）。
        seed: Color::rgb(0x5F, 0x6B, 0x2E),
        neutral_chroma: DEFAULT_NEUTRAL_CHROMA,
    },
];

/// 出厂默认卡（首次启动、用户没选过卡时的那一张）。
pub const DEFAULT_CARD: Card = CARDS[0];

/// 色卡的类型化键（`Card::key` 的枚举形态）。
///
/// 加了它是因为「存字符串」这件事必须只有一处口径：设置读回来时走 [`CardKey::from_key`]，
/// 写出去时走 [`CardKey::key`]，两边不会漂。新卡追加在末尾，**不重排**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CardKey {
    /// 天蓝（出厂默认，M70 ④）。
    Sky,
    /// 薰衣草紫（官方 M3 baseline）。
    Lavender,
    /// 森林绿。
    Forest,
    /// 琥珀橙。
    Amber,
    /// 玫瑰红。
    Rose,
    /// 青碧。
    Teal,
    /// 暮蓝。
    Dusk,
    /// 橄榄绿。
    Olive,
}

impl CardKey {
    /// 全部键（顺序与 [`CARDS`] 一致）。
    pub const ALL: [CardKey; 8] = [
        CardKey::Sky,
        CardKey::Lavender,
        CardKey::Forest,
        CardKey::Amber,
        CardKey::Rose,
        CardKey::Teal,
        CardKey::Dusk,
        CardKey::Olive,
    ];

    /// 持久化键（与 [`Card::key`] 逐字相同）。
    pub const fn key(self) -> &'static str {
        CARDS[self.index()].key
    }

    /// 这张卡。
    pub const fn card(self) -> Card {
        CARDS[self.index()]
    }

    /// 显示名。
    pub const fn name(self) -> &'static str {
        CARDS[self.index()].name
    }

    const fn index(self) -> usize {
        match self {
            CardKey::Sky => 0,
            CardKey::Lavender => 1,
            CardKey::Forest => 2,
            CardKey::Amber => 3,
            CardKey::Rose => 4,
            CardKey::Teal => 5,
            CardKey::Dusk => 6,
            CardKey::Olive => 7,
        }
    }

    /// 从持久化键解析；**未知键返回 `None`**（调用方回落 [`DEFAULT_CARD`] 并可以提示
    /// 「设置里的色卡名已失效」——绝不静默当成默认卡用掉，那样用户会以为设置没保存）。
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|card| card.key() == key)
    }
}

impl From<CardKey> for Card {
    fn from(key: CardKey) -> Self {
        key.card()
    }
}

/// 卡片方案是否过 WCAG（给测试与设置页复用；正常路径由 [`super::resolve`] 兜底）。
pub fn card_scheme_is_readable(card: Card, flavor: SchemeFlavor) -> bool {
    wcag::check(&card.scheme(flavor)).is_clean()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_least_six_cards_and_default_is_sky() {
        assert!(CARDS.len() >= 6, "只有 {} 张卡", CARDS.len());
        assert_eq!(DEFAULT_CARD.key, "sky");
        assert_eq!(CARDS[0].seed, crate::color::palettes::DEFAULT_SEED);
    }

    #[test]
    fn card_keys_are_stable_and_unique() {
        // ⚠️ 这些字符串是持久化键：改一个就等于把老用户的选择抹成出厂默认。
        const EXPECTED: [&str; 8] = [
            "sky", "lavender", "forest", "amber", "rose", "teal", "dusk", "olive",
        ];
        for (card, expected) in CARDS.iter().zip(EXPECTED) {
            assert_eq!(card.key, expected);
        }
        let mut keys: Vec<&str> = CARDS.iter().map(|card| card.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), CARDS.len(), "key 有重复");
        for card in CARDS {
            assert!(
                card.key.chars().all(|c| c.is_ascii_lowercase()),
                "{} 不是小写 ASCII",
                card.key
            );
            assert!(!card.name.is_empty());
        }
    }

    #[test]
    fn card_key_roundtrips_and_rejects_unknown() {
        for key in CardKey::ALL {
            assert_eq!(CardKey::from_key(key.key()), Some(key));
            assert_eq!(key.card().key, key.key());
        }
        assert_eq!(CardKey::from_key("no-such-card"), None);
        assert_eq!(CardKey::from_key("SKY"), None, "key 大小写敏感");
    }

    #[test]
    fn every_card_scheme_meets_wcag_aa() {
        for card in CARDS {
            for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
                assert!(
                    card_scheme_is_readable(card, flavor),
                    "{} / {flavor:?} 未过门禁",
                    card.key
                );
                let ratio = card.primary_contrast(flavor);
                assert!(
                    ratio >= wcag::AA_TEXT,
                    "{} / {flavor:?} primary 对 {} 只有 {ratio:.2}",
                    card.key,
                    card.name
                );
                // 上半区间：文档（`Card::primary_contrast`）向用户承诺的就是这两个区间，
                // 只钉「≥ 4.5」等于没钉——比值掉到 5.0 也全绿，而文档会继续说 6.43~6.49。
                // 边界取实测极值 ±0.01：浅色 6.4324~6.4906、深色 7.7024~7.7504。
                let (low, high) = match flavor {
                    SchemeFlavor::Light => (6.43, 6.50),
                    SchemeFlavor::Dark => (7.70, 7.76),
                };
                assert!(
                    (low..=high).contains(&ratio),
                    "{} / {flavor:?} primary 对比度 {ratio:.4} 超出文档区间 {low}~{high}",
                    card.key
                );
            }
        }
    }

    #[test]
    fn cards_are_visually_distinct() {
        // 色相间隔 ≥ 20°：八张卡的种子在两套方案里都不该撞成同一套配色。
        let mut hues: Vec<f64> = CARDS
            .iter()
            .map(|card| crate::color::Hct::from_color(card.seed).hue)
            .collect();
        hues.sort_by(|a, b| a.partial_cmp(b).expect("有限色相"));
        for pair in hues.windows(2) {
            let gap = pair[1] - pair[0];
            assert!(gap >= 20.0, "色相挤在一起：{pair:?}");
        }
        // 色相是环：排序后「最大 − 最小」不是真实间隔，**回绕那一侧**也要检查。
        // 漏了这条的话，将来加一张 350° 的卡（与 rose 15.85° 实际只差 25.7°）会被算成
        // 334.2° 而静默通过。
        let wrap_gap = 360.0 - (hues[hues.len() - 1] - hues[0]);
        assert!(
            wrap_gap >= 20.0,
            "色相环回绕处挤在一起：{:.2}° ↔ {:.2}°（间隔 {wrap_gap:.2}°）",
            hues[hues.len() - 1],
            hues[0]
        );
        for card in CARDS {
            let light = card.scheme(SchemeFlavor::Light);
            let dark = card.scheme(SchemeFlavor::Dark);
            assert_ne!(
                light.get(crate::color::Role::Primary),
                dark.get(crate::color::Role::Primary)
            );
        }
    }
}
