//! 玻璃层令牌（GLASSSPEC）：S0~S4 五档表面 × GPU/CPU 两渲染档的**唯一取值入口**。
//!
//! ## 硬规矩：组件里只允许出现 `GlassSpec::for_tier(...)`
//!
//! 消费方契约（`DevDocs/SubAgentsTalk/m3-visual-spec.md:68-78`）：「组件里只允许出现
//! `GlassSpec::for_tier(...)`，不允许出现 `0.86`、`18.0` 这类魔法数」。所以 σ、tint 角色与 α、
//! rim 颜色与 α、noise α、shadow 全收进一张表：**组件不许自己拼 sigma/alpha，也不许在表外
//! 另存副本**。改玻璃观感 = 改这张表。
//!
//! ## 数值来源
//!
//! `DevDocs/PlanDoc/research/11b-m3-glass-params.md` §1.2（核心参数，GPU 版）是主表；
//! §1.3（tint α 的 WCAG 下限）与 §1.6（CPU 档降级）是两条派生规则。逐格照抄，不在这里另调。
//! ⚠️ 本文件已进**贴线带（≤400）**：**下次要改它，先拆文件再改**——§1.5 的 `specular_alpha` 落地会加字段与一整列（上限口径查 `.github/workflows/guards.yml` 的 `file-size` job）。
//!
//! ## 双渲染档（D27）
//!
//! - **CPU 档禁模糊**（σ = 0）且**禁一切半透明**（tint α = 1.00，半透明只在 GPU 档存在，见 D27）；
//! - CPU 档 `shadow = none`、`rim = none`、`noise = 0`，边界改 1px `outline_variant`（§1.6）；
//! - 几何、字号、圆角、内边距、动效曲线**两档必须完全一致**——降级只换材质，不换排版。

use crate::color::{Color, Role, SchemeFlavor};
use crate::elevation::{Elevation, ShadowSpec};
use crate::spacing::stroke::HAIRLINE;

/// 表面分档：§1.0 的 S0~S4，**只有这五类，禁止新增**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GlassTier {
    /// S0 WindowRoot：窗口底，永远不透明。
    S0,
    /// S1 Content：内容卡 / 列表行 / 区块。**不模糊**（§1.2：占桌面列表 90% 面积，模糊无收益）；
    /// 描边版（border variant）用 [`OutlineSpec::OUTLINE_VARIANT`]。
    S1,
    /// S2 Panel：贴边大面板（导航栏、标题栏、状态栏），σ = 16。
    S2,
    /// S3 Float：临时浮层（菜单、下拉、tooltip），σ = 18。
    S3,
    /// S4 Modal：模态上层（对话框、AI 抽屉），σ = 24。
    S4,
}

impl GlassTier {
    /// 全部五档（文档生成与测试用）。
    #[rustfmt::skip]
    pub const ALL: [GlassTier; 5] = [GlassTier::S0, GlassTier::S1, GlassTier::S2, GlassTier::S3, GlassTier::S4];
}

/// 渲染档：GPU 走玻璃，CPU 走纯色降级（D27）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderTier {
    /// 玻璃：模糊 + 半透明 tint + rim + 噪点。
    Gpu,
    /// 纯色降级：禁模糊 / 禁半透明，无 rim / 噪点 / 阴影，改 1px `outline_variant` 描边。
    Cpu,
}

/// 同一参数在「暗 / 亮」两套主题下的取值。
///
/// §1.2 的 rim 列是**两套颜色 + 两套 α**（暗用白、亮用黑），tint α 列也写成「暗/亮」两档，
/// 所以取值入口只给一对，消费方拿 `spec` 后按当前 `flavor` 取一次。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlavorValue<T> {
    /// 暗色主题下的值。
    pub dark: T,
    /// 亮色主题下的值。
    pub light: T,
}

impl<T: Copy> FlavorValue<T> {
    /// 暗 / 亮两个不同取值。
    pub const fn new(dark: T, light: T) -> Self {
        Self { dark, light }
    }

    /// 暗 / 亮同一个取值（§1.2 的 tint α 两列同值）。
    pub const fn uniform(value: T) -> Self {
        Self {
            dark: value,
            light: value,
        }
    }

    /// 按当前主题取值。
    pub const fn get(self, flavor: SchemeFlavor) -> T {
        match flavor {
            SchemeFlavor::Dark => self.dark,
            SchemeFlavor::Light => self.light,
        }
    }
}

/// rim 描边的三条边形态（§1.2 的「无 / 顶边 / 全周」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RimEdge {
    /// 不画 rim（S0/S1，以及 CPU 档全部）。
    None,
    /// 只画顶边（S2：顶部光源打在面板上沿）。
    Top,
    /// 四边全画（S3/S4：浮层四周都要有边界）。
    All,
}

/// rim 描边：三态 + 亮暗两套颜色与 α。宽度恒为 1px（[`HAIRLINE`]）。
///
/// 亮度方向固定为「顶部光源」（§1.2）：**暗色用白、亮色用黑**，四边亮度一致。
/// [`RimEdge::None`] 的档里 `color` / `alpha` 是**占位值**：先看 [`RimSpec::is_visible`]。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RimSpec {
    /// 画哪几条边。
    pub edge: RimEdge,
    /// 描边宽度（px）。
    pub width_px: f32,
    /// 描边颜色（暗=白 / 亮=黑）。
    pub color: FlavorValue<Color>,
    /// 描边 α（暗/亮两档）。
    pub alpha: FlavorValue<f32>,
}

impl RimSpec {
    /// 不画 rim（占位色 / α 全零）。
    pub const NONE: RimSpec = RimSpec {
        edge: RimEdge::None,
        width_px: HAIRLINE,
        color: FlavorValue::uniform(Color::BLACK),
        alpha: FlavorValue::uniform(0.0),
    };

    /// 该档是否真的要画 rim。
    pub const fn is_visible(self) -> bool {
        !matches!(self.edge, RimEdge::None)
    }
}

/// 1px `outline_variant` 描边：**CPU 档的边界替代品**（§1.6），也是 S1 描边版用的那一条。
///
/// 宽度与角色都由令牌给全（**一个字段表达**，不让调用方自己拼 1px 与角色）：
/// 外层 `Option` 是 `Some` 就画，没有就不画。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutlineSpec {
    /// 描边宽度（px）。
    pub width_px: f32,
    /// 描边颜色角色（§1.6 定为 `outline_variant`）。
    pub color: Role,
}

impl OutlineSpec {
    /// CPU 档 / S1 描边版的唯一那份描边：1px `outline_variant`（§1.6）。
    pub const OUTLINE_VARIANT: OutlineSpec = OutlineSpec {
        width_px: HAIRLINE,
        color: Role::OutlineVariant,
    };
}

/// 一档表面的完整玻璃参数。取值**只有** [`GlassSpec::for_tier`] 一个入口。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlassSpec {
    /// 这一档是哪个表面类。
    pub tier: GlassTier,
    /// 这一份是哪个渲染档的参数。
    pub render_tier: RenderTier,
    /// 高斯模糊 σ（px）。**S1 与 CPU 档都是 0，不是「接近 0」**。渲染侧按 σ 直接做高斯，
    /// 别再去套 radius（§1.2）；radius 只在造图/对表时用 [`sigma_from_radius`] 换算。
    pub blur_sigma_px: f32,
    /// tint 的颜色角色（叠在面板底色上）。
    pub tint: Role,
    /// tint 的 α（暗/亮两档）。§1.3：玻璃上有次要文字时 α ≥ 0.80。
    pub tint_alpha: FlavorValue<f32>,
    /// rim 描边（三态 + 亮暗两套颜色/α）。
    pub rim: RimSpec,
    /// 噪点 α（`0.02~0.03`，1:1 物理像素平铺；CPU 档为 0）。
    pub noise_alpha: f32,
    /// 单层阴影（**每个元素只画一层**）。`None` = 这一档不画：S0/S1/**S2** 常态与 CPU 档；
    /// S2 按 R4 取 None（§1.2 给了 `0 1 3 @0.14`，该格视为被取代）；S1 的 `L0→L1` 只在 hover 借 [`Elevation::Level1`]。
    pub shadow: Option<ShadowSpec>,
    /// 边界走「1px `outline_variant` 描边」而不是 rim 时的描边参数（§1.6）。
    ///
    /// **GPU 档恒为 `None`**（边界由 [`GlassSpec::rim`] 表达），CPU 档恒为
    /// `Some(`[`OutlineSpec::OUTLINE_VARIANT`]`)`——两者由 [`GlassSpec::for_tier`] 保证互斥。
    pub outline: Option<OutlineSpec>,
}

impl GlassSpec {
    /// **唯一取值入口**：查 §1.2 的玻璃核心参数表，并按 §1.6 做 CPU 档降级（见模块文档）。
    pub const fn for_tier(tier: GlassTier, render_tier: RenderTier) -> Self {
        let gpu = Self::gpu_table(tier);
        match render_tier {
            RenderTier::Gpu => gpu,
            RenderTier::Cpu => Self {
                render_tier: RenderTier::Cpu,
                blur_sigma_px: 0.0,
                tint_alpha: FlavorValue::uniform(1.0),
                rim: RimSpec::NONE,
                noise_alpha: 0.0,
                shadow: None,
                outline: Some(OutlineSpec::OUTLINE_VARIANT),
                ..gpu
            },
        }
    }

    /// §1.2「玻璃核心参数（GPU 版）」表，逐格照抄；**改这里 = 改全站玻璃观感**。
    ///
    /// 一行 = 规格表的一行，列序同规格：(σ, tint 角色, tint α, rim, noise α, shadow)。
    /// 手写密度由 `#[rustfmt::skip]` 保住——这张表是数据不是代码，道理同本 crate 的
    /// `color/critical_planes.rs`。
    #[rustfmt::skip]
    const fn gpu_table(tier: GlassTier) -> Self {
        use Role::{Surface, SurfaceContainer, SurfaceContainerHigh};
        let (blur_sigma_px, tint, tint_alpha, rim, noise_alpha, shadow) = match tier {
            GlassTier::S0 => (0.0,  Surface,               uniform(1.00), RimSpec::NONE,                 0.0,  None),
            GlassTier::S1 => (0.0,  SurfaceContainer,     uniform(1.00), RimSpec::NONE,                 0.0,  None),
            // S2 的 shadow：§1.2 表里给了 `0 1 3 @0.14`，但反例红线 R4 的「正确做法」栏限定「只有
            // S3/S4 有阴影」⇒ 本表按 R4 取 None，§1.2 的该格视为被取代（M431 r1 塔裁定）。
            GlassTier::S2 => (16.0, SurfaceContainerHigh, uniform(0.86), rim(RimEdge::Top, 0.10, 0.06), 0.02, None),
            GlassTier::S3 => (18.0, SurfaceContainer,     uniform(0.86), rim(RimEdge::All, 0.12, 0.07), 0.03, Some(Elevation::Level4.shadow())),
            GlassTier::S4 => (24.0, SurfaceContainerHigh, uniform(0.88), rim(RimEdge::All, 0.14, 0.08), 0.03, Some(Elevation::Level5.shadow())),
        };
        Self { tier, render_tier: RenderTier::Gpu, blur_sigma_px, tint, tint_alpha, rim, noise_alpha, shadow, outline: None }
    }

    /// 这一档是否真的要模糊（σ > 0）。S1 与整个 CPU 档都是 `false`。
    pub const fn is_blurred(&self) -> bool {
        self.blur_sigma_px > 0.0
    }

    /// 边界走哪条：`true` = [`GlassSpec::outline`]（CPU 档），`false` = [`GlassSpec::rim`]。
    pub const fn uses_outline(&self) -> bool {
        self.outline.is_some()
    }
}

/// §1.2 一行 rim 列：三态 + 亮暗两套 α（颜色恒为「暗白 / 亮黑」，顶部光源）。
const fn rim(edge: RimEdge, dark_alpha: f32, light_alpha: f32) -> RimSpec {
    RimSpec {
        edge,
        width_px: HAIRLINE,
        color: FlavorValue::new(Color::WHITE, Color::BLACK),
        alpha: FlavorValue::new(dark_alpha, light_alpha),
    }
}

/// §1.2 tint α 列：暗/亮同值（表里就是两列同数）。
const fn uniform(value: f32) -> FlavorValue<f32> {
    FlavorValue::uniform(value)
}

/// Skia 的 σ ↔ radius 换算系数（`σ = 0.57735·radius + 0.5`，源 `SkBlurMask.cpp:35`）。
pub const SIGMA_PER_RADIUS: f32 = 0.57735;

/// 换算里的常数项（见 [`SIGMA_PER_RADIUS`]）。
pub const SIGMA_OFFSET: f32 = 0.5;

/// 模糊半径 → 高斯 σ：`σ = 0.57735·radius + 0.5`（§1.2 注释，Skia 来源）。
///
/// 表的 blur 列写的是 **σ**，渲染侧按 σ 直接做高斯；这个函数只在「设计稿给 radius、要反查 σ」
/// 时用（27→16、30→18、41→24）。
pub const fn sigma_from_radius(radius_px: f32) -> f32 {
    SIGMA_PER_RADIUS * radius_px + SIGMA_OFFSET
}

/// 高斯 σ → 模糊半径（[`sigma_from_radius`] 的反解）。
pub const fn radius_from_sigma(sigma_px: f32) -> f32 {
    (sigma_px - SIGMA_OFFSET) / SIGMA_PER_RADIUS
}

#[cfg(test)]
mod tests {
    use super::*;

    const DARK: SchemeFlavor = SchemeFlavor::Dark;
    const LIGHT: SchemeFlavor = SchemeFlavor::Light;
    /// §1.3 的 WCAG 下限：玻璃上出现次要文字时 tint α ≥ 0.80。
    const MIN_TINT_ALPHA: f32 = 0.80;

    /// §1.2 整张表逐格等值：期望值写字面量（自引用实现会变成永远为真的同义反复）。
    /// 表是数据不是代码，`#[rustfmt::skip]` 保住「一行一档」。
    #[test]
    #[rustfmt::skip]
    fn gpu_table_matches_the_spec_table() {
        // (档, σ, tint, tint α, rim 形态, rim 暗 α, rim 亮 α, noise α, shadow (offset_y, blur, α))
        // 除 S2 的 shadow 外逐格照 §1.2；S2 按 R4 取 None（§1.2 该格被 R4 取代，见 gpu_table 的注释）。
        let rows = [
            (GlassTier::S0, 0.0,  Role::Surface,               1.00, RimEdge::None, 0.00, 0.00, 0.00, None),
            (GlassTier::S1, 0.0,  Role::SurfaceContainer,      1.00, RimEdge::None, 0.00, 0.00, 0.00, None),
            (GlassTier::S2, 16.0, Role::SurfaceContainerHigh,  0.86, RimEdge::Top,  0.10, 0.06, 0.02, None),
            (GlassTier::S3, 18.0, Role::SurfaceContainer,      0.86, RimEdge::All,  0.12, 0.07, 0.03, Some((6.0, 18.0, 0.24))),
            (GlassTier::S4, 24.0, Role::SurfaceContainerHigh,  0.88, RimEdge::All,  0.14, 0.08, 0.03, Some((8.0, 24.0, 0.28))),
        ];
        for (tier, sigma, tint, alpha, edge, dark_rim, light_rim, noise, shadow) in rows {
            let s = GlassSpec::for_tier(tier, RenderTier::Gpu);
            let got = (s.blur_sigma_px, s.tint, s.tint_alpha.dark, s.tint_alpha.light, s.noise_alpha);
            assert_eq!(got, (sigma, tint, alpha, alpha, noise), "{tier:?} 的 σ/tint/noise 列");
            let got = (s.rim.edge, s.rim.alpha.dark, s.rim.alpha.light);
            assert_eq!(got, (edge, dark_rim, light_rim), "{tier:?} 的 rim 列");
            assert_eq!(s.rim.width_px, HAIRLINE, "{tier:?} 的 rim 必须 1px");
            let got = s.shadow.map(|sh| (sh.offset_y_px, sh.blur_px, sh.alpha));
            assert_eq!(got, shadow, "{tier:?} 的 shadow 列");
            if s.rim.is_visible() {
                // §1.2：暗用白、亮用黑（顶部光源）。
                let got = (s.rim.color.dark, s.rim.color.light);
                assert_eq!(got, (Color::WHITE, Color::BLACK), "{tier:?} 的 rim 颜色");
            } else {
                assert_eq!(s.rim, RimSpec::NONE, "{tier:?} 无 rim 时必须是占位值");
            }
        }
    }

    /// ② S1 的 σ **必须恰好是 0**，不是「接近 0」——`0.5` 那种也不可能过。
    #[test]
    fn s1_is_exactly_unblurred() {
        let spec = GlassSpec::for_tier(GlassTier::S1, RenderTier::Gpu);
        assert_eq!(spec.blur_sigma_px, 0.0, "S1 必须不模糊（§1.2）");
        assert!(!spec.is_blurred());
        // 反证：S2/S3/S4 必须真的模糊，否则「全都不模糊」也能让上面那条为真。
        for tier in [GlassTier::S2, GlassTier::S3, GlassTier::S4] {
            let spec = GlassSpec::for_tier(tier, RenderTier::Gpu);
            assert!(spec.is_blurred() && spec.blur_sigma_px >= 16.0, "{tier:?}");
        }
    }

    /// ③ CPU 档（§1.6）：无阴影、无 rim、σ = 0、α = 1.00、1px 描边在。
    #[test]
    fn cpu_tier_degrades_per_section_1_6() {
        for tier in GlassTier::ALL {
            let gpu = GlassSpec::for_tier(tier, RenderTier::Gpu);
            let cpu = GlassSpec::for_tier(tier, RenderTier::Cpu);
            assert_eq!(cpu.render_tier, RenderTier::Cpu, "{tier:?}");
            assert_eq!(cpu.blur_sigma_px, 0.0, "{tier:?} CPU 档禁模糊");
            assert_eq!(cpu.shadow, None, "{tier:?} CPU 档无阴影");
            assert_eq!(cpu.rim, RimSpec::NONE, "{tier:?} CPU 档无 rim");
            assert_eq!(cpu.noise_alpha, 0.0, "{tier:?} CPU 档无噪点");
            // §1.6 + D27：CPU 档禁一切半透明。
            assert_eq!(
                (cpu.tint_alpha.dark, cpu.tint_alpha.light),
                (1.00, 1.00),
                "{tier:?}"
            );
            // 边界改由这条 1px outline_variant 描边表达，且 GPU 档不许同时带它。
            assert_eq!(cpu.outline, Some(OutlineSpec::OUTLINE_VARIANT), "{tier:?}");
            assert_eq!(
                cpu.outline.map(|o| (o.width_px, o.color)),
                Some((1.0, Role::OutlineVariant))
            );
            assert_eq!(gpu.outline, None, "{tier:?} GPU 档不该有 outline 描边");
            // 几何一致的前提：档位与 tint 角色两档相同，只有材质不同。
            assert!((cpu.tier, cpu.tint) == (gpu.tier, gpu.tint), "{tier:?}");
            assert!(cpu.uses_outline() && !gpu.uses_outline(), "{tier:?}");
        }
    }

    /// ④ tint α ≥ 0.80（§1.3 的 WCAG 下限）：五档 × 亮暗 × 两渲染档。
    #[test]
    fn tint_alpha_never_drops_below_the_wcag_floor() {
        for tier in GlassTier::ALL {
            for render in [RenderTier::Gpu, RenderTier::Cpu] {
                for flavor in [DARK, LIGHT] {
                    let alpha = GlassSpec::for_tier(tier, render).tint_alpha.get(flavor);
                    assert!(
                        (MIN_TINT_ALPHA..=1.0).contains(&alpha),
                        "{tier:?} / {render:?} / {flavor:?} 的 tint α {alpha} 越界"
                    );
                }
            }
        }
        // 反证（运行期取数，不是常量断言）：表里最低的 tint α 是 S2/S3 的 0.86，必须严格高于
        // §1.3 的 0.80 下限——否则「全部 ≥ 0.80」会被 0.72 那样的档蒙过去。
        let min = GlassTier::ALL
            .iter()
            .map(|tier| GlassSpec::for_tier(*tier, RenderTier::Gpu).tint_alpha.dark)
            .fold(1.0_f32, f32::min);
        assert!(min > MIN_TINT_ALPHA && min == 0.86, "最低 tint α 是 {min}");
    }

    /// ⑤ σ ↔ radius 换算：规格表里的三组（27→16、30→18、41→24）。
    #[test]
    fn sigma_radius_conversion_matches_the_spec_pairs() {
        for (radius, sigma) in [(27.0_f32, 16.0_f32), (30.0, 18.0), (41.0, 24.0)] {
            let got = sigma_from_radius(radius);
            assert!(
                (got - sigma).abs() < 0.2,
                "radius {radius} → σ {got}，表写 {sigma}"
            );
            // 表里的 σ 是取整值，四舍五入后必须逐字相等。
            assert_eq!(got.round(), sigma, "radius {radius} 的 σ 取整不等于表值");
            assert!(
                (radius_from_sigma(got) - radius).abs() < 0.01,
                "反解不回 {radius}"
            );
        }
        // 表里的 σ 只有 {0,16,18,24}（验收清单 §3.1 第 12 项）。
        for tier in GlassTier::ALL {
            let sigma = GlassSpec::for_tier(tier, RenderTier::Gpu).blur_sigma_px;
            assert!(
                matches!(sigma as u32, 0 | 16 | 18 | 24),
                "{tier:?} 的 σ {sigma}"
            );
        }
    }
}
