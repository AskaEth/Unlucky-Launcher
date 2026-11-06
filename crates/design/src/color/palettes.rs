//! 六条 tone 调色板的集合（M3 scheme 的原料）。
//!
//! 角色 → 调色板槽位 → tone 的三段映射在 [`super::scheme`]；这里只负责「种子 → 六条调色板」，
//! 以及**官方 Tonal Spot 变体**的彩度常数。
//!
//! 彩度值来自 `material-color-utilities` 的 `ColorSpec2021.get*Palette(Variant.TONAL_SPOT)`
//! （Apache-2.0，见 `NOTICE`）：primary 恒定 **36**、secondary **16**、tertiary **24**
//! （色相 +60°）、neutral **6**、neutral-variant **8**、error 固定色相 **25** 彩度 **84**。
//! 注意 primary 的 36 与种子自身的彩度无关——这是官方口径：Tonal Spot 是「镇静」变体，
//! 再鲜艳的种子也会被收到 36，再灰的种子也会被提到 36。
//!
//! ## 两个入口：官方口径 vs 产品默认（M70）
//!
//! | 入口 | 种子 | 中性彩度 | 用途 |
//! |---|---|---|---|
//! | [`Palettes::official_baseline`] / [`super::Scheme::official_baseline`] | [`BASELINE_SEED`]（`#6750A4`） | 6（官方） | 与官方 Java 实现逐角色对齐的**锚点**，`tests/official_baseline.rs` 用它 |
//! | [`Palettes::baseline`] / [`super::Scheme::baseline`] | [`DEFAULT_SEED`] | [`DEFAULT_NEUTRAL_CHROMA`] | **产品出厂默认**（消费方拿到的就是它） |
//!
//! 换默认色只改 [`DEFAULT_SEED`] 与 [`DEFAULT_NEUTRAL_CHROMA`] 两行常量（中性彩度不动就只改一行），
//! 然后重跑 `tests/skyseed_candidates.rs` 的 pin（它会打印新值并指出哪几行要同步）。
//! 官方锚点不受影响——**算法没动，动的只是入参**。
//!
//! ⚠️ 中性彩度 6 → 12 是**有意偏离**官方 Tonal Spot（理由：产品要「浅冷」的冷调表面），
//! 偏离登记见 [`super`] 模块文档的「出厂默认色」小节。

use super::tonal::TonalPalette;
use super::Color;

/// M3 baseline 种子色（与 material-color-utilities 的默认基线同值）。
///
/// **不是**产品默认色——产品默认见 [`DEFAULT_SEED`]。
pub const BASELINE_SEED: Color = Color::rgb(0x67, 0x50, 0xA4);

/// 产品出厂默认种子色 —— M70 候选④「天蓝」，**用户 2026-11-01 拍板采用**。
///
/// `#3C8DBC`（CAM16 hue ≈ 239.5°，主色 `#236488`）。注意种子的彩度（42.6）会被 Tonal Spot
/// 丢掉——主色板的彩度恒为 36，种子只贡献**色相**。
///
/// ## 定位（D38）：这是「莫奈关闭时的出厂预选色卡」，不是唯一的配色来源
///
/// 用户定了三级优先的配色来源：① 启动器背景图取色（Monet）→ ② 桌面壁纸取色 → ③ 莫奈关闭
/// （默认）时用户从内置多套色卡里选。本常量 + [`DEFAULT_NEUTRAL_CHROMA`] 只定义**第三级里
/// 出厂预选的那一套**（首次启动、用户没选过色卡时的那一套；`palette::DEFAULT_CARD` 的种子即本值）。
///
/// 取色（量化 + 挑主色）、内置色卡库与三级回落都在 [`crate::palette`]（M87，分层见 D40）；
/// 本模块只负责「给定种子 + 中性彩度，派生出 49 个角色」。
pub const DEFAULT_SEED: Color = Color::rgb(0x3C, 0x8D, 0xBC);

/// 产品出厂默认的中性调色板彩度（官方 Tonal Spot 是 6，这里 12 = **有意偏离**）。
///
/// 官方 6 让表面几乎无色；12 让 surface / surface-container-* 带一层淡蓝调，
/// 这是「浅冷天蓝」唯一能落在表面上的手段（primary 的 tone 被官方钉在 40，
/// 种子再浅也变不成浅蓝）。
///
/// **数值安全区间 `6..=13`，产品取 12**：neutral-variant 跟 neutral 联动（`nv = neutral + 2`），
/// 13 时 nv = 15 仍低于 secondary 的 16；**14 才会撞上**（nv = 16 = secondary），届时要一并改
/// `NEUTRAL_VARIANT_OFFSET`（私有常量）的规则。nv 的那一半偏离见 `color` 模块文档的偏离登记。
///
/// 同样只服务于「出厂预选色卡」（D38 第三级），不约束将来的取色结果。
/// 内置多套色卡落地时每张卡各带自己的中性彩度（`palette::Card::neutral_chroma`），八张卡
/// 当前都取本值；再加上中性**色相**固定为 [`NEUTRAL_HUE_DEG`]（D43），八张卡的表面色因此
/// 逐值相同——切卡只换强调色。
pub const DEFAULT_NEUTRAL_CHROMA: f64 = 12.0;

/// 中性调色板的**固定色相**（度）——内置色卡共用它，动态取色不用它（D43）。
///
/// ## 出处：这就是「天蓝」档当前用的那个中性色相
///
/// 值等于 [`DEFAULT_SEED`]（`#3C8DBC`）的 CAM16 色相 **239.51420577630955°**。天蓝档的中性
/// 调色板本来就是这个色相（`from_seed_with_neutral_chroma` 让中性色跟着种子走），所以它是
/// **沿用**，不是「另选一个冷调」：`Card::scheme` 换用它之后天蓝卡的 49 个角色必须逐值不变
/// （`tests/card_neutral.rs` 钉着）。
///
/// ## 为什么要有这个常量（D43）
///
/// 八张卡原先走 TonalSpot 的「中性色也跟种子染色」，切卡连窗口底色一起换调子：浅色档
/// `surface` 的实测 CAM16 彩度从 **1.29（rose）跨到 12.03（teal）**——tone 98 处暖相色域装
/// 不下彩度 12，「请求值 12」在暖相上只能兑现 1.29，9 倍跳变让用户觉得「每张卡都像换了个
/// 应用」。D43 的处置：**色卡固定中性色相、只换强调色**；动态取色（背景图 / 壁纸）保留逐色
/// 染色，那才是用户要的 Monet 体验。所以本常量只服务色卡路径，**不要**传给 `Seed::scheme`。
///
/// ⚠️ 换 [`DEFAULT_SEED`] 时本常量会「过期」：`neutral_hue_is_the_default_seed_hue` 用
/// 1e-9° 容差钉住两者的绑定（**不用 `to_bits` 位相等**——色相来自跨平台有末位差的 `atan2`），
/// 改了种子它就红——那时要想清楚是「色卡跟着新默认色走」（照抄新色相）还是「色卡保持这条
/// 冷调」（改成显式例外），别静默漂移。
pub const NEUTRAL_HUE_DEG: f64 = 239.514_205_776_309_55;

/// 调色板槽位（M3 的六条 tonal palette）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteSlot {
    /// 主调色板（种子色相，彩度 36）。
    Primary,
    /// 次调色板（同色相，彩度 16）。
    Secondary,
    /// 第三调色板（种子色相 +60°，彩度 24）。
    Tertiary,
    /// 中性调色板（色相跟种子；**色卡路径**例外——固定为 [`NEUTRAL_HUE_DEG`]，D43。
    /// 官方彩度 6，产品默认 12——见 [`DEFAULT_NEUTRAL_CHROMA`]）。
    Neutral,
    /// 中性变体调色板（色相同 neutral，彩度 = neutral + 2；官方 8，产品默认 14）。
    NeutralVariant,
    /// 错误调色板（固定红相 25°，彩度 84）。
    Error,
}

/// Tonal Spot 的 primary 彩度（官方常数，与种子彩度无关）。
const PRIMARY_CHROMA: f64 = 36.0;
/// secondary 彩度。
const SECONDARY_CHROMA: f64 = 16.0;
/// tertiary 彩度。
const TERTIARY_CHROMA: f64 = 24.0;
/// 官方 Tonal Spot 的 neutral 彩度（几乎是灰）。产品默认用 [`DEFAULT_NEUTRAL_CHROMA`]。
const TONAL_SPOT_NEUTRAL_CHROMA: f64 = 6.0;
/// neutral-variant 相对 neutral 的彩度偏移。
///
/// ⚠️ 官方 `getNeutralVariantPalette` 写死的是**绝对值 8**，**不是**「neutral + 2」这种相对式；
/// 本实现选择让 neutral-variant 随 neutral 联动，于是产品默认（neutral 12）取到 **14**。
/// 这是**有意偏离**的另一半（见 `color` 模块文档的偏离登记），不是官方口径——
/// 官方的 6 / 8 由 [`Palettes::official_baseline`] 原样保留。
const NEUTRAL_VARIANT_OFFSET: f64 = 2.0;
/// error 的色相（HCT 红相）。
const ERROR_HUE_DEG: f64 = 25.0;
/// error 的彩度。
const ERROR_CHROMA: f64 = 84.0;
/// tertiary 相对种子的色相偏移。
const TERTIARY_HUE_SHIFT_DEG: f64 = 60.0;

/// 一套完整的六条调色板。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palettes {
    /// 主要调色板。
    pub primary: TonalPalette,
    /// 次要调色板。
    pub secondary: TonalPalette,
    /// 第三调色板。
    pub tertiary: TonalPalette,
    /// 中性调色板。
    pub neutral: TonalPalette,
    /// 中性变体调色板。
    pub neutral_variant: TonalPalette,
    /// 错误调色板。
    pub error: TonalPalette,
}

impl Palettes {
    /// **产品出厂默认**：种子 [`DEFAULT_SEED`] + 中性彩度 [`DEFAULT_NEUTRAL_CHROMA`]。
    ///
    /// 消费方（`DesignTokens::baseline`、iced 适配层）拿到的就是这套，所以产品默认色
    /// 只需要改那两个常量。
    pub fn baseline() -> Self {
        Self::from_seed_with_neutral_chroma(DEFAULT_SEED, DEFAULT_NEUTRAL_CHROMA)
    }

    /// **官方基线的锚点**：种子 [`BASELINE_SEED`] + 官方中性彩度 6（等价官方
    /// `SchemeTonalSpot(0xFF6750A4)`）。只给对齐测试与文档表格用。
    pub fn official_baseline() -> Self {
        Self::from_seed(BASELINE_SEED)
    }

    /// 从一个种子色派生全套调色板（官方 Tonal Spot 变体，中性彩度取官方 6）。
    pub fn from_seed(seed: Color) -> Self {
        Self::from_seed_with_neutral_chroma(seed, TONAL_SPOT_NEUTRAL_CHROMA)
    }

    /// 从一个种子色派生，并显式指定中性调色板彩度（`6.0` 即官方 Tonal Spot）。
    ///
    /// ⚠️ **调用方负责传 `6.0..=13.0`（产品取 12）**。本函数**不做范围校验**，越界的表现是
    /// 静默退化而不是 panic：
    /// - `neutral_chroma < 0` 或 `NaN` → 彩度被夹到 0（`TonalPalette::new` 里 `.max(0.0)`），
    ///   得到一套纯灰的表面（surface ≈ `#F9F9F9`），**不报错**；
    /// - `neutral_chroma >= 14` → neutral-variant（= neutral + 2）够到甚至超过 secondary 的 16，
    ///   「中性 < 中性变体 < 次色」的层次被破坏（描边会明显发蓝）。
    ///
    /// 除 neutral / neutral-variant 外**全部走官方常数**：primary 恒定 36、secondary 16、
    /// tertiary 24（色相 +60°）、error 固定 25°/84。所以这条入口不绕算法，只是把「官方写死在
    /// `getNeutralPalette` / `getNeutralVariantPalette` 里的那两个数」变成可调入参
    /// （neutral-variant 这里取相对式 `neutral + 2`，属**有意偏离**，见 `NEUTRAL_VARIANT_OFFSET`）。
    pub fn from_seed_with_neutral_chroma(seed: Color, neutral_chroma: f64) -> Self {
        let hue = super::Hct::from_color(seed).hue;
        Self::tonal_spot(hue, neutral_chroma)
    }

    /// 只换强调色：primary/secondary/tertiary 跟着新强调色的色相走，
    /// **表面中性色与错误色保持不变**。
    ///
    /// 这是本项目的产品语义（换强调色不该让整个窗口底色变脏，见 `DesignTokens::with_accent`）：
    /// 强调族的色值与官方 `from_seed(accent)` 逐个对得上，中性表面则仍走基线——
    /// 官方 `from_seed` 会把中性色也染上种子的色相，那是「整窗染色」，不是我们要的。
    pub fn with_accent(self, accent: Color) -> Self {
        let hue = super::Hct::from_color(accent).hue;
        Self {
            primary: TonalPalette::new(hue, PRIMARY_CHROMA),
            secondary: TonalPalette::new(hue, SECONDARY_CHROMA),
            tertiary: TonalPalette::new(hue + TERTIARY_HUE_SHIFT_DEG, TERTIARY_CHROMA),
            ..self
        }
    }

    /// **内置色卡的派生入口（D43）**：强调三族跟 `accent` 的色相走，中性族（含
    /// neutral-variant）**固定在 `neutral_hue_deg`**。
    ///
    /// 与 [`Palettes::from_seed_with_neutral_chroma`]（TonalSpot，中性色跟着种子染色）的唯一
    /// 差别是中性族色相；与 [`Palettes::with_accent`] 的差别是中性族可以整体搬到一个不属于
    /// 基线的色相上（`with_accent` 只能沿用 `self` 的中性族）。
    ///
    /// 产品用法：色卡路径传 [`NEUTRAL_HUE_DEG`]（= 出厂默认种子的中性色相），于是八张卡的
    /// 表面色完全相同，只有强调色在换。**取色路径不要用本函数**——D43 明确保留「按图像种子
    /// 逐色染色」的 Monet 体验（见 [`crate::palette::resolve`]）。
    pub fn accent_on_fixed_neutral(
        accent: Color,
        neutral_hue_deg: f64,
        neutral_chroma: f64,
    ) -> Self {
        let hue = super::Hct::from_color(accent).hue;
        Self::tonal_spot_with_neutral(hue, neutral_hue_deg, neutral_chroma)
    }

    /// 官方 Tonal Spot 的六条调色板（色相来自种子，彩度取官方常数，neutral 可调）。
    fn tonal_spot(hue: f64, neutral_chroma: f64) -> Self {
        Self::tonal_spot_with_neutral(hue, hue, neutral_chroma)
    }

    /// 同一套 Tonal Spot，但**中性族（neutral / neutral-variant）的色相可以与种子不同**。
    ///
    /// 这是 D43「内置色卡固定中性色相」的实现核心：强调三族跟 `accent_hue`，表面族跟
    /// `neutral_hue`。其余彩度常数、error 族、tone 表一行未动。
    fn tonal_spot_with_neutral(accent_hue: f64, neutral_hue: f64, neutral_chroma: f64) -> Self {
        Self {
            primary: TonalPalette::new(accent_hue, PRIMARY_CHROMA),
            secondary: TonalPalette::new(accent_hue, SECONDARY_CHROMA),
            tertiary: TonalPalette::new(accent_hue + TERTIARY_HUE_SHIFT_DEG, TERTIARY_CHROMA),
            neutral: TonalPalette::new(neutral_hue, neutral_chroma),
            neutral_variant: TonalPalette::new(
                neutral_hue,
                neutral_chroma + NEUTRAL_VARIANT_OFFSET,
            ),
            error: TonalPalette::new(ERROR_HUE_DEG, ERROR_CHROMA),
        }
    }

    /// 按槽位取调色板。
    pub fn slot(&self, slot: PaletteSlot) -> TonalPalette {
        match slot {
            PaletteSlot::Primary => self.primary,
            PaletteSlot::Secondary => self.secondary,
            PaletteSlot::Tertiary => self.tertiary,
            PaletteSlot::Neutral => self.neutral,
            PaletteSlot::NeutralVariant => self.neutral_variant,
            PaletteSlot::Error => self.error,
        }
    }

    /// 方案级取色：槽位 + tone → 色值。
    pub fn tone(&self, slot: PaletteSlot, tone: f64) -> Color {
        self.slot(slot).tone(tone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_baseline_uses_the_official_tonal_spot_chromas() {
        let palettes = Palettes::official_baseline();
        assert_eq!(palettes.primary.chroma(), 36.0);
        assert_eq!(palettes.secondary.chroma(), 16.0);
        assert_eq!(palettes.tertiary.chroma(), 24.0);
        assert_eq!(palettes.neutral.chroma(), 6.0);
        assert_eq!(palettes.neutral_variant.chroma(), 8.0);
        assert_eq!(palettes.error.hue_deg(), 25.0);
        assert_eq!(palettes.error.chroma(), 84.0);
    }

    #[test]
    fn product_default_only_moves_the_named_knobs() {
        // 产品默认与官方锚点的差别必须**只**在种子色相与中性彩度两处：
        // primary/secondary/tertiary/error 的取法一模一样（算法没动）。
        let official = Palettes::official_baseline();
        let product = Palettes::baseline();
        assert_eq!(product.primary.chroma(), official.primary.chroma());
        assert_eq!(product.secondary.chroma(), official.secondary.chroma());
        assert_eq!(product.tertiary.chroma(), official.tertiary.chroma());
        assert_eq!(product.error, official.error);
        assert_eq!(product.neutral.chroma(), DEFAULT_NEUTRAL_CHROMA);
        assert_eq!(
            product.neutral_variant.chroma(),
            DEFAULT_NEUTRAL_CHROMA + NEUTRAL_VARIANT_OFFSET
        );
        let seed = super::super::Hct::from_color(DEFAULT_SEED);
        assert!((product.primary.hue_deg() - seed.hue).abs() < 1.0e-9);
    }

    #[test]
    fn neutral_chroma_knob_keeps_the_official_ordering() {
        // 官方关系 neutral(6) < neutral-variant(8) < secondary(16) 在安全区间里必须一直成立，
        // 否则「表面比次色还艳」或两族撞色。
        for neutral in [6.0, 8.0, 10.0, 12.0] {
            let p = Palettes::from_seed_with_neutral_chroma(DEFAULT_SEED, neutral);
            assert!(p.neutral.chroma() < p.neutral_variant.chroma());
            assert!(p.neutral_variant.chroma() < p.secondary.chroma());
            // 中性彩度不影响 tone 轴：tone 98 的 surface 仍是最亮的那个。
            let surface = p.tone(PaletteSlot::Neutral, 98.0);
            assert!(surface.relative_luminance() > 0.9);
        }
    }

    #[test]
    fn product_default_neutral_chroma_is_a_registered_deviation() {
        // 官方 neutral / neutral-variant 是 6 / 8；产品默认是 12 / 14。
        // 这**不是官方口径**，是「有意偏离」（见 `color` 模块文档的偏离登记）。
        // 想把 nv 拉回官方绝对值 8 的人，请先改登记再改这里：直接钉成 8 会让表面（12）
        // 比中性变体（8）更「有色」，破坏官方的 neutral < neutral-variant 层次。
        let product = Palettes::baseline();
        assert_eq!(product.neutral.chroma(), 12.0, "产品默认 neutral 彩度");
        assert_eq!(
            product.neutral_variant.chroma(),
            14.0,
            "产品默认的 neutral-variant 彩度 14 是**有意偏离**（官方绝对值 8），改前先更新偏离登记"
        );
        let official = Palettes::official_baseline();
        assert_eq!(official.neutral.chroma(), 6.0, "官方锚点仍是 6");
        assert_eq!(
            official.neutral_variant.chroma(),
            8.0,
            "官方锚点仍是绝对值 8（不是 neutral + 2 的相对式）"
        );
    }

    #[test]
    fn baseline_hue_comes_from_the_seed() {
        let palettes = Palettes::official_baseline();
        let seed = super::super::Hct::from_color(BASELINE_SEED);
        assert!((palettes.primary.hue_deg() - seed.hue).abs() < 1.0e-9);
        assert!((palettes.neutral.hue_deg() - seed.hue).abs() < 1.0e-9);
        assert!((palettes.tertiary.hue_deg() - (seed.hue + 60.0)).abs() < 1.0e-9);
    }

    #[test]
    fn seed_chroma_never_leaks_into_the_primary_ramp() {
        // 官方口径：Tonal Spot 的彩度是常数，纯灰种子也会得到彩度 36 的主色板。
        let gray = Palettes::from_seed(Color::rgb(0x6E, 0x6E, 0x6E));
        assert_eq!(gray.primary.chroma(), 36.0);
        let hot = Palettes::from_seed(Color::rgb(0xFF, 0x00, 0x00));
        assert_eq!(hot.primary.chroma(), 36.0);
    }

    #[test]
    fn accent_override_keeps_surfaces_and_error() {
        let base = Palettes::baseline();
        let swapped = base.with_accent(Color::rgb(0x00, 0x6A, 0x5C));
        assert_eq!(swapped.neutral, base.neutral);
        assert_eq!(swapped.neutral_variant, base.neutral_variant);
        assert_eq!(swapped.error, base.error);
        assert_ne!(swapped.primary.hue_deg(), base.primary.hue_deg());
        assert_eq!(swapped.primary.chroma(), 36.0);
    }

    #[test]
    fn neutral_hue_is_the_default_seed_hue() {
        // D43 的绑定器。用 1e-9° 容差而非 `to_bits()` 位相等：色相来自 `atan2()`，glibc 与
        // UCRT 理论可差 1 ulp（~1e-14°），位相等会在另一个 OS 的 CI 上红；这条要抓的是「换了
        // `DEFAULT_SEED` 却没复核常量」（偏几十度）。「逐值不变」的位相等钉子在同平台的
        // `card_neutral.rs::sky_card_is_bit_identical_to_the_product_baseline`（两个算出的值
        // 互比，不涉及跨平台字面量）。
        let seed_hue = super::super::Hct::from_color(DEFAULT_SEED).hue;
        assert!(
            (NEUTRAL_HUE_DEG - seed_hue).abs() < 1.0e-9,
            "NEUTRAL_HUE_DEG = {NEUTRAL_HUE_DEG:?} 与 DEFAULT_SEED 的色相 {seed_hue:?} \
             相差超过 1e-9°（换默认种子时：要么照抄新色相，要么把这里改成显式例外）"
        );
    }

    #[test]
    fn accent_on_fixed_neutral_only_moves_the_neutral_hue() {
        let official = Palettes::official_baseline();
        // 中性色相 = 基线色相时，与官方的 from_seed 完全等价（不放宽算法）。
        let same = Palettes::accent_on_fixed_neutral(DEFAULT_SEED, NEUTRAL_HUE_DEG, 12.0);
        let per_seed = Palettes::from_seed_with_neutral_chroma(DEFAULT_SEED, 12.0);
        assert_eq!(
            same, per_seed,
            "中性色相取种子色相时必须与 TonalSpot 逐项相同"
        );

        // 换一个中性色相：只有中性两族的色相动，彩度与强调族一律不动。
        let moved = Palettes::accent_on_fixed_neutral(DEFAULT_SEED, 120.0, 12.0);
        assert_eq!(moved.primary, per_seed.primary);
        assert_eq!(moved.secondary, per_seed.secondary);
        assert_eq!(moved.tertiary, per_seed.tertiary);
        assert_eq!(moved.error, per_seed.error);
        assert_eq!(moved.neutral.chroma(), per_seed.neutral.chroma());
        assert_eq!(
            moved.neutral_variant.chroma(),
            per_seed.neutral_variant.chroma()
        );
        assert_eq!(moved.neutral.hue_deg(), 120.0);
        assert_eq!(moved.neutral_variant.hue_deg(), 120.0);
        assert_eq!(moved.primary.chroma(), official.primary.chroma());
    }
}
