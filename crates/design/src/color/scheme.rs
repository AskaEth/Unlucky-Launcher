//! 一套完整的 M3 语义色方案（`Scheme`）：角色 → 调色板槽位 → tone 的三段映射。
//!
//! tone 表取自 **ColorSpec2021** 的官方映射（`MaterialDynamicColors` 的 light/dark 两列，
//! 见 `NOTICE`）。固定角色（`*-fixed*`）深浅同值，因为它们的用途就是「跨模式稳定」。
//!
//! ## 只实现了 contrast level = 0（standard）
//!
//! 官方的 `ColorSpec2021.getTone()` 还有一层机制：`toneDeltaPair`（成对角色保持 tone 差）
//! 与 `ContrastCurve`（按对比度等级修正 tone）。在 **contrastLevel = 0** 下这层机制的输入
//! 全部是常数：`ContrastCurve.get(0)` 恒等于 normal 档的 tone，而「背景 tone」也都是本表里的
//! 常数，所以它对 49 个角色的调整结果**就是这张表本身**。测试
//! `tests/official_baseline.rs` 用官方 Java 实现的逐角色输出反证了这一点（每通道误差 ≤1）：
//! 表错一格，对齐测试就会红。三档对比度（medium/high）不在本 crate 范围内，见
//! [`super`] 的 gap 登记。

use super::boards::BrandBoard;
use super::palettes::{PaletteSlot, Palettes};
use super::roles::Role;
use super::Color;

/// 深浅模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemeFlavor {
    /// 浅色（亮底暗字）。
    Light,
    /// 深色（暗底亮字）。
    Dark,
}

impl SchemeFlavor {
    /// 是否深色模式。
    pub const fn is_dark(self) -> bool {
        matches!(self, SchemeFlavor::Dark)
    }

    /// 反过来那一个。
    pub const fn toggled(self) -> Self {
        match self {
            SchemeFlavor::Light => SchemeFlavor::Dark,
            SchemeFlavor::Dark => SchemeFlavor::Light,
        }
    }

    /// 稳定字符串名（`light` / `dark`），写配置与日志用。
    pub const fn name(self) -> &'static str {
        match self {
            SchemeFlavor::Light => "light",
            SchemeFlavor::Dark => "dark",
        }
    }
}

/// 阴影色透明度（M3 阴影 = 纯黑 + 这个 alpha；绘制阴影时叠在背景上）。
pub const SHADOW_ALPHA: f32 = 0.20;
/// 遮罩色透明度（模态背景压暗层）。
pub const SCRIM_ALPHA: f32 = 0.32;

/// 一条「角色 → 槽位 + tone」映射。
struct ToneRow {
    role: Role,
    slot: PaletteSlot,
    light: f64,
    dark: f64,
}

/// 49 个角色的 tone 映射表（ColorSpec2021），**下标必须等于 `Role as usize`**（由测试钉死）。
const TONE_TABLE: [ToneRow; 49] = [
    row(Role::Primary, PaletteSlot::Primary, 40.0, 80.0),
    row(Role::OnPrimary, PaletteSlot::Primary, 100.0, 20.0),
    row(Role::PrimaryContainer, PaletteSlot::Primary, 90.0, 30.0),
    row(Role::OnPrimaryContainer, PaletteSlot::Primary, 30.0, 90.0),
    row(Role::Secondary, PaletteSlot::Secondary, 40.0, 80.0),
    row(Role::OnSecondary, PaletteSlot::Secondary, 100.0, 20.0),
    row(Role::SecondaryContainer, PaletteSlot::Secondary, 90.0, 30.0),
    row(
        Role::OnSecondaryContainer,
        PaletteSlot::Secondary,
        30.0,
        90.0,
    ),
    row(Role::Tertiary, PaletteSlot::Tertiary, 40.0, 80.0),
    row(Role::OnTertiary, PaletteSlot::Tertiary, 100.0, 20.0),
    row(Role::TertiaryContainer, PaletteSlot::Tertiary, 90.0, 30.0),
    row(Role::OnTertiaryContainer, PaletteSlot::Tertiary, 30.0, 90.0),
    row(Role::Error, PaletteSlot::Error, 40.0, 80.0),
    row(Role::OnError, PaletteSlot::Error, 100.0, 20.0),
    row(Role::ErrorContainer, PaletteSlot::Error, 90.0, 30.0),
    row(Role::OnErrorContainer, PaletteSlot::Error, 30.0, 90.0),
    row(Role::Surface, PaletteSlot::Neutral, 98.0, 6.0),
    row(Role::OnSurface, PaletteSlot::Neutral, 10.0, 90.0),
    row(
        Role::SurfaceVariant,
        PaletteSlot::NeutralVariant,
        90.0,
        30.0,
    ),
    row(
        Role::OnSurfaceVariant,
        PaletteSlot::NeutralVariant,
        30.0,
        80.0,
    ),
    row(Role::Background, PaletteSlot::Neutral, 98.0, 6.0),
    row(Role::OnBackground, PaletteSlot::Neutral, 10.0, 90.0),
    row(Role::SurfaceDim, PaletteSlot::Neutral, 87.0, 6.0),
    row(Role::SurfaceBright, PaletteSlot::Neutral, 98.0, 24.0),
    row(
        Role::SurfaceContainerLowest,
        PaletteSlot::Neutral,
        100.0,
        4.0,
    ),
    row(Role::SurfaceContainerLow, PaletteSlot::Neutral, 96.0, 10.0),
    row(Role::SurfaceContainer, PaletteSlot::Neutral, 94.0, 12.0),
    row(Role::SurfaceContainerHigh, PaletteSlot::Neutral, 92.0, 17.0),
    row(
        Role::SurfaceContainerHighest,
        PaletteSlot::Neutral,
        90.0,
        22.0,
    ),
    row(Role::Outline, PaletteSlot::NeutralVariant, 50.0, 60.0),
    row(
        Role::OutlineVariant,
        PaletteSlot::NeutralVariant,
        80.0,
        30.0,
    ),
    row(Role::InverseSurface, PaletteSlot::Neutral, 20.0, 90.0),
    row(Role::InverseOnSurface, PaletteSlot::Neutral, 95.0, 20.0),
    row(Role::InversePrimary, PaletteSlot::Primary, 80.0, 40.0),
    row(Role::Shadow, PaletteSlot::Neutral, 0.0, 0.0),
    row(Role::Scrim, PaletteSlot::Neutral, 0.0, 0.0),
    row(Role::SurfaceTint, PaletteSlot::Primary, 40.0, 80.0),
    row(Role::PrimaryFixed, PaletteSlot::Primary, 90.0, 90.0),
    row(Role::PrimaryFixedDim, PaletteSlot::Primary, 80.0, 80.0),
    row(Role::OnPrimaryFixed, PaletteSlot::Primary, 10.0, 10.0),
    row(
        Role::OnPrimaryFixedVariant,
        PaletteSlot::Primary,
        30.0,
        30.0,
    ),
    row(Role::SecondaryFixed, PaletteSlot::Secondary, 90.0, 90.0),
    row(Role::SecondaryFixedDim, PaletteSlot::Secondary, 80.0, 80.0),
    row(Role::OnSecondaryFixed, PaletteSlot::Secondary, 10.0, 10.0),
    row(
        Role::OnSecondaryFixedVariant,
        PaletteSlot::Secondary,
        30.0,
        30.0,
    ),
    row(Role::TertiaryFixed, PaletteSlot::Tertiary, 90.0, 90.0),
    row(Role::TertiaryFixedDim, PaletteSlot::Tertiary, 80.0, 80.0),
    row(Role::OnTertiaryFixed, PaletteSlot::Tertiary, 10.0, 10.0),
    row(
        Role::OnTertiaryFixedVariant,
        PaletteSlot::Tertiary,
        30.0,
        30.0,
    ),
];

/// `const` 友好的行构造器（数组初始化里不能调用非 const fn 之外的东西）。
const fn row(role: Role, slot: PaletteSlot, light: f64, dark: f64) -> ToneRow {
    ToneRow {
        role,
        slot,
        light,
        dark,
    }
}

/// 一套语义色（49 个角色，全部已解析成具体颜色）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scheme {
    flavor: SchemeFlavor,
    colors: [Color; 49],
}

impl Scheme {
    /// **D38 取色链的中性锚点**（种子 [`super::palettes::DEFAULT_SEED`] + 中性彩度
    /// [`super::palettes::DEFAULT_NEUTRAL_CHROMA`]，见 [`Palettes::baseline`]）。
    ///
    /// M228 起**产品出厂外观**走 [`Scheme::from_board`]（Ink / Silver 固化色板）；本入口保留给
    /// 取色链与 [`Scheme::from_accent`] 的表面色基准，两者并列而不是互相替换（理由见
    /// [`super::boards`] 的模块文档）。
    pub fn baseline(flavor: SchemeFlavor) -> Self {
        Self::from_palettes(&Palettes::baseline(), flavor)
    }

    /// **官方基线锚点**（种子 `#6750A4` + 官方中性彩度 6，等价官方
    /// `SchemeTonalSpot(0xFF6750A4)`）。
    ///
    /// 与 [`Scheme::baseline`] 的区别只有入参：算法、tone 表、gamut 映射完全同一条路径，
    /// 所以 `tests/official_baseline.rs` 拿它跟官方 Java 输出逐角色对账。
    pub fn official_baseline(flavor: SchemeFlavor) -> Self {
        Self::from_palettes(&Palettes::official_baseline(), flavor)
    }

    /// 指定种子色的方案（表面色与强调色同源，等价官方 `SchemeTonalSpot`）。
    pub fn from_seed(flavor: SchemeFlavor, seed: Color) -> Self {
        Self::from_palettes(&Palettes::from_seed(seed), flavor)
    }

    /// 只覆盖强调色（表面中性色仍走基线，见 [`Palettes::with_accent`]）。
    pub fn from_accent(flavor: SchemeFlavor, accent: Color) -> Self {
        Self::from_palettes(&Palettes::baseline().with_accent(accent), flavor)
    }

    /// 直接从调色板集合构建（高级入口：整仓测试与自定义 scheme 用）。
    pub fn from_palettes(palettes: &Palettes, flavor: SchemeFlavor) -> Self {
        let colors = std::array::from_fn(|i| {
            let entry = &TONE_TABLE[i];
            palettes.tone(entry.slot, Scheme::tone_of(entry.role, flavor))
        });
        Self { flavor, colors }
    }

    /// **品牌色板**（Ink 深 / Silver 浅）→ 完整方案：`unlucky_design::color::BrandBoard`。
    ///
    /// 与 [`Scheme::baseline`] 的区别不是算法而是**来源**：baseline 由种子 + tone 表推导（D38
    /// 取色链），本入口直接吃用户规格卡固化的 49 个色值（`color::boards`）。深浅模式由板自己
    /// 决定（`BrandBoard::flavor`），所以不需要再传 [`SchemeFlavor`]——穿错了会得到「深板浅色」
    /// 这种自相矛盾的方案。
    pub fn from_board(board: BrandBoard) -> Self {
        Self {
            flavor: board.flavor(),
            colors: board.colors(),
        }
    }

    /// 从 49 个已解析色值直接构造（**底层入口**，下标必须等于 `Role as usize`）。
    ///
    /// 只给两类调用方用：
    /// 1. `palette::wcag` 的对比度收敛——它按角色替换个别色值后要重新组装成一套方案；
    /// 2. 测试——要能构造「故意不达标」的方案，比较度校验器才有反证。
    ///
    /// 常规路径一律走 [`Scheme::from_palettes`]：那样色值才是 tone 表的推导结果，
    /// 而不是随手塞进来的数。
    pub fn from_colors(flavor: SchemeFlavor, colors: [Color; 49]) -> Self {
        Self { flavor, colors }
    }

    /// 该角色在指定模式下的 tone 值（M3 语义；文档表格、调试与测试用）。
    pub fn tone_of(role: Role, flavor: SchemeFlavor) -> f64 {
        let entry = &TONE_TABLE[role as usize];
        debug_assert_eq!(entry.role, role, "TONE_TABLE 下标必须等于角色序号");
        match flavor {
            SchemeFlavor::Light => entry.light,
            SchemeFlavor::Dark => entry.dark,
        }
    }

    /// 取角色色值。
    pub fn get(&self, role: Role) -> Color {
        self.colors[role as usize]
    }

    /// 当前模式。
    pub fn flavor(&self) -> SchemeFlavor {
        self.flavor
    }

    /// 按角色顺序遍历（`for (role, color) in scheme.iter()`）。
    pub fn iter(&self) -> impl Iterator<Item = (Role, Color)> + '_ {
        Role::ALL.iter().copied().map(|role| (role, self.get(role)))
    }

    /// 阴影色（纯黑 + [`SHADOW_ALPHA`]）。
    pub fn shadow(&self) -> Color {
        self.get(Role::Shadow).with_alpha(SHADOW_ALPHA)
    }

    /// 遮罩色（纯黑 + [`SCRIM_ALPHA`]）。
    pub fn scrim(&self) -> Color {
        self.get(Role::Scrim).with_alpha(SCRIM_ALPHA)
    }

    /// 两个角色的 WCAG 对比度（1..21）。
    pub fn contrast(&self, fg: Role, bg: Role) -> f32 {
        self.get(fg).contrast_ratio(self.get(bg))
    }
}

impl std::ops::Index<Role> for Scheme {
    type Output = Color;

    fn index(&self, role: Role) -> &Color {
        &self.colors[role as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_table_matches_role_order() {
        for (i, entry) in TONE_TABLE.iter().enumerate() {
            assert_eq!(
                entry.role as usize,
                i,
                "TONE_TABLE 第 {i} 行是 {}，顺序与 Role 枚举不一致",
                entry.role.name()
            );
        }
    }

    #[test]
    fn fixed_roles_are_identical_across_flavors() {
        let light = Scheme::baseline(SchemeFlavor::Light);
        let dark = Scheme::baseline(SchemeFlavor::Dark);
        for role in [
            Role::PrimaryFixed,
            Role::PrimaryFixedDim,
            Role::OnPrimaryFixed,
            Role::OnPrimaryFixedVariant,
            Role::SecondaryFixed,
            Role::TertiaryFixedDim,
        ] {
            assert_eq!(
                light.get(role),
                dark.get(role),
                "{} 应跨模式同值",
                role.name()
            );
        }
    }

    #[test]
    fn light_and_dark_differ_on_text_pairs() {
        let light = Scheme::baseline(SchemeFlavor::Light);
        let dark = Scheme::baseline(SchemeFlavor::Dark);
        assert_ne!(light.get(Role::OnSurface), dark.get(Role::OnSurface));
        assert!(light.get(Role::Surface).relative_luminance() > 0.8);
        assert!(dark.get(Role::Surface).relative_luminance() < 0.05);
    }

    #[test]
    fn shadow_and_scrim_carry_alpha() {
        let scheme = Scheme::baseline(SchemeFlavor::Light);
        assert_eq!(scheme.shadow().a, (SHADOW_ALPHA * 255.0).round() as u8);
        assert_eq!(scheme.scrim().a, (SCRIM_ALPHA * 255.0).round() as u8);
    }

    #[test]
    fn accent_override_changes_primary_only() {
        let base = Scheme::baseline(SchemeFlavor::Light);
        let teal = Scheme::from_accent(SchemeFlavor::Light, Color::rgb(0x00, 0x6A, 0x5C));
        assert_ne!(base.get(Role::Primary), teal.get(Role::Primary));
        assert_eq!(base.get(Role::Surface), teal.get(Role::Surface));
        assert_eq!(
            base.get(Role::Outline),
            teal.get(Role::Outline),
            "强调色覆盖不该动中性描边"
        );
        assert_eq!(base.get(Role::Error), teal.get(Role::Error));
    }
}
