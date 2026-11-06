//! # 色彩令牌（M3 color roles）
//!
//! 49 个 M3 语义色角色 × light/dark 两套，全部由**一个种子色**派生（[`Palettes`]），
//! 再按官方 tone 映射表解析成具体色值（[`Scheme`]）。
//!
//! ```no_run
//! use unlucky_design::color::{Color, Role, Scheme, SchemeFlavor};
//!
//! let light = Scheme::baseline(SchemeFlavor::Light);
//! let surface = light.get(Role::Surface);
//! let on_surface = light.get(Role::OnSurface);
//! assert!(on_surface.contrast_ratio(surface) > 4.5);
//!
//! // 用户自定义强调色：表面中性色不动，只换强调族。
//! let custom = Scheme::from_accent(SchemeFlavor::Light, Color::from_hex("#006A5C").unwrap());
//! assert_eq!(custom.get(Role::Surface), surface);
//! ```
//!
//! ## 口径声明：真 HCT，不是近似
//!
//! 标准 M3 的调色板建在 **HCT**（CAM16 色相/彩度 + CIELAB 的 `L*`）上。本模块**就是**那套算法：
//! CAM16 正换算（`cam16` 模块）取色相/彩度，`L*` 取 tone，色域外走官方的 `HctSolver`
//! （`hct_solver` 模块：Newton 迭代 + 色调平面上的色相二分，**只压彩度、不硬裁**）。
//!
//! | 轴 | 标准 M3 | 本实现 | 一致性 |
//! |---|---|---|---|
//! | tone | CIELAB `L*` | CIELAB `L*`（同一个 `Y` 轴） | 逐值一致 |
//! | 色相 / 彩度 | CAM16 | CAM16（D65 默认观察条件） | 逐值一致（误差 < 1e-4） |
//! | gamut 映射 | `HctSolver` | 同一套两段求解器，关键平面表原样搬运 | 逐通道一致 |
//! | 调色板彩度 | Tonal Spot 变体：36 / 16 / 24 / 6 / 8，error 25°/84 | 同（primary/secondary/tertiary/neutral-variant/error 逐值一致） | 逐值一致 |
//! | 中性彩度 | 6（几乎无色） | 官方口径 6；**产品默认用 12**（有意偏离，见下节「出厂默认色」） | 仅此处偏离 |
//! | 角色 tone 表 | ColorSpec2021 | 同（49 角色 × light/dark） | 逐值一致 |
//!
//! 对齐证据在 `tests/official_baseline.rs` 与 `tests/official_accent.rs`：期望值全部由
//! `material-color-utilities`（官方 Java 实现，Apache-2.0）跑出来，**每通道误差 ≤1** 才通过。
//! 所以本 crate 可以对外说「M3 动态取色（Tonal Spot 变体 / standard 对比度）」。
//!
//! ## 未实现的部分（gap 登记，别以为已经有）
//!
//! | 能力 | 官方叫什么 | 状态 |
//! |---|---|---|
//! | 对比度三档（default/medium/high） | `ContrastCurve` + `contrastLevel` | **未实现**：只做 `contrastLevel = 0`。在 0 档下官方那层「tone 差对 + 对比度修正」机制的输入全是常数，等价于本模块的静态 tone 表（已由对齐测试反证） |
//! | 其他配色变体 | `Vibrant` / `Expressive` / `Fidelity` / `Content` / `Monochrome` / `Neutral` / `Rainbow` / `FruitSalad` / `CMF` | **未实现**：只有 `TonalSpot`（M3 默认、也是 Material Theme Builder 默认） |
//! | 壁纸多色取色 | `QuantizerCelebi` / `Score` / 多 source 色 | **单种子部分已实现**：量化（5 bit 直方图 + CIELAB k-means）与 `Score` 挑主色在 [`crate::palette`]（M87）。**多 source 色**（一张图出 4 个种子、每个 `*_container` 换色相）仍未实现——产品口径是「一套主题一个强调色」（D38） |
//! | 换观察条件 | `Hct.inViewingConditions` | **未实现**：只有 sRGB 屏的默认条件（D65 / 200 lux / 背景 `L* 50`） |
//!
//! ## 「自定义强调色」与官方 `from_seed` 的关系
//!
//! 官方 `SchemeTonalSpot(accent)` 会把**中性表面也染上**种子的色相；本项目的产品语义是
//! 「换强调色不该让整个窗口底色变脏」，所以 [`Scheme::from_accent`] 只换 primary/secondary/
//! tertiary 三族，中性色与 error 保持基线。强调族的色值与官方 `from_seed(accent)` 逐个对得上
//! （`tests/official_accent.rs`，8 个种子 × 12 个强调族角色 × 2 模式 = 192 项）。
//!
//! ## 出厂默认色（M70：浅冷天蓝）——含**两处**有意偏离（同一处偏离的两半）
//!
//! 产品默认不走官方紫，而是「浅冷天蓝」。默认值收在两个常量里，**换默认只改这两行**
//! （用户 2026-11-01 拍板采用候选 **④ 天蓝**；③ 浅冷天蓝 `#87CEEB` 留作备选，值仍 pin 在
//! `tests/skyseed_candidates.rs`）：
//!
//! | 项 | 当前值 | 官方值 | 说明 |
//! |---|---|---|---|
//! | [`palettes::DEFAULT_SEED`] | `#3C8DBC`（CAM16 hue ≈ 240°） | `#6750A4`（[`palettes::BASELINE_SEED`]） | 种子只贡献**色相**（CAM16 240°）；种子彩度会被 Tonal Spot 丢掉 |
//! | [`palettes::DEFAULT_NEUTRAL_CHROMA`]（neutral） | `12` | `6` | 让 surface / surface-container-* 带淡蓝调 |
//! | 联动的 neutral-variant | `14` | `8`（**绝对值**；官方不是「neutral + 2」） | `outline` / `outline-variant` / `surface-variant` 一起偏蓝（light `outline` = `#6A7985` 就是 14 彩度的口径） |
//!
//! **定位（D38）**：这套默认只是「**莫奈关闭时的出厂预选色卡**」（首次启动、用户还没选过色卡时
//! 用的那一套），**不是唯一的配色来源**。用户的配色来源有三级优先：① 启动器背景图取色（Monet）
//! → ② 桌面壁纸取色 → ③ 莫奈关闭（默认）时从内置多套色卡里选。取色（量化 + 挑主色）、内置色卡库
//! 与三级回落都在 [`crate::palette`]（M87 落地，D40 分层）；**本模块只负责**「给定种子 + 中性彩度，
//! 派生出 49 个角色」，不碰像素、不碰路径、不碰回落决策。
//!
//! > ⚠️ **有意偏离登记（两处，同一处偏离的两半）**：
//! >
//! > 1. **neutral 彩度 `6 → 12`**。理由＝产品要「浅冷」的冷调表面：官方 6 的表面几乎无色，单靠换
//! >    种子做不到「浅」——primary 的 tone 被官方钉在 40，种子再浅主色也还是中蓝，所以「浅」只能
//! >    落在中性色上。
//! > 2. **neutral-variant 彩度 `8 → 14`**。这是第 1 条的连带结果：官方
//! >    `getNeutralVariantPalette` 写死的是**绝对值 8**（不是「neutral + 2」这种相对式），
//! >    本实现为了让层次不倒挂，让它跟着 neutral 联动（`nv = neutral + 2`）。若把 nv 钉回 8，
//! >    表面（12）会比中性变体（8）更「有色」，破坏官方的「neutral < neutral-variant」层次。
//! >    影响面不只表面：`outline` / `outline-variant` / `surface-variant` 三个角色一起偏蓝。
//! >
//! > 除这两处外全部走官方常数（primary 36 / secondary 16 / tertiary 24 且色相 +60° /
//! > error 25°·84），tone 表与 gamut 映射一行未动；[`Scheme::official_baseline`] 仍是官方的
//! > 6（neutral）/ 8（neutral-variant）口径。
//!
//! 官方口径的锚点仍然完整保留：[`Scheme::official_baseline`]（种子 `#6750A4` + 中性 6）
//! 是 `tests/official_baseline.rs` 的对账入口，98 项仍是每通道 ≤1；
//! `tests/skyseed_candidates.rs` 把四档候选（保守蓝 / 冷调蓝 / 浅冷蓝 / 天蓝）的色值逐个 pin 住，
//! 换默认时它会指着要改的行。
//!
//! ✅ 默认色已由用户 2026-11-01 拍板：**④ 天蓝 `#3C8DBC`**。备选 ③ 浅冷蓝 `#87CEEB`
//! （主色偏青 `#0C6780`）仍在候选表里，换回去只需改 [`palettes::DEFAULT_SEED`] 一行。
//!
//! ## 对比度纪律（WCAG AA）
//!
//! - 所有 `on-*` 与其承载色（`*-container`、`primary`、`surface`）都满足 **≥ 4.5:1**；
//!   这一条由 `tests/contrast_aa.rs` 全表验证，且因为 tone 轴固定，**换任何种子色都成立**
//!   （WCAG 对比度只取决于相对亮度，而 tone 直接钉住 `L*`；中性彩度同理，
//!   `sky_seed_candidates_meet_wcag_aa` 逐档验证了 M70 的四档候选：4 档 × 2 模式 × 26 对 = 208 项）。
//! - 例外（显式登记，非缺陷）：
//!   - [`Role::Outline`] 对 [`Role::Surface`]：M3 定义的**非文字**元素（输入框描边），
//!     目标 3:1，实测 light 4.26:1 / dark 5.89:1；
//!   - [`Role::OutlineVariant`] 对 [`Role::Surface`]：纯装饰分隔线，M3 不要求对比度下限；
//!   - [`Role::SurfaceVariant`]：它是**容器**色（不是前景），不参与对比度门禁。
//! - 禁用态按 M3 的 alpha 叠加（[`state::DISABLED_CONTENT_ALPHA`]），刻意低于 AA。

pub mod hct;
pub mod palettes;
pub mod roles;
pub mod scheme;
pub mod state;
pub mod tonal;

mod cam16;
mod critical_planes;
mod hct_solver;
// `pub(crate)`：取色量化（`crate::palette::quantize`）要在 CIELAB 里算簇距离，复用同一套换算。
pub(crate) mod space;
mod viewing_conditions;

pub use hct::Hct;
pub use palettes::{
    PaletteSlot, Palettes, BASELINE_SEED, DEFAULT_NEUTRAL_CHROMA, DEFAULT_SEED, NEUTRAL_HUE_DEG,
};
pub use roles::Role;
pub use scheme::{Scheme, SchemeFlavor, SCRIM_ALPHA, SHADOW_ALPHA};
pub use state::StateLayer;
pub use tonal::TonalPalette;

/// sRGB 颜色（8 bit/通道 + alpha），刻意不用第三方颜色类型：本 crate 零依赖。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    /// 红通道。
    pub r: u8,
    /// 绿通道。
    pub g: u8,
    /// 蓝通道。
    pub b: u8,
    /// alpha（0 = 全透明，255 = 不透明）。
    pub a: u8,
}

impl Color {
    /// 不透明颜色。
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// 带 alpha 的颜色（`a` 为 0..=255）。
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// 纯黑。
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    /// 纯白。
    pub const WHITE: Color = Color::rgb(255, 255, 255);

    /// 从 `#RRGGBB` / `#RGB` / `#AARRGGBB` 解析（不合法返回 `None`）。
    ///
    /// 只给配置读取与测试用——UI 代码里**不许出现色字面量**，一律走 [`Role`]。
    pub fn from_hex(text: &str) -> Option<Self> {
        let hex = text.trim().trim_start_matches('#');
        let parse = |s: &str| u8::from_str_radix(s, 16).ok();
        match hex.len() {
            3 => {
                let d = |i: usize| {
                    let v = parse(&hex[i..i + 1])?;
                    Some(v * 17)
                };
                Some(Self::rgb(d(0)?, d(1)?, d(2)?))
            }
            6 => Some(Self::rgb(
                parse(&hex[0..2])?,
                parse(&hex[2..4])?,
                parse(&hex[4..6])?,
            )),
            8 => Some(Self::rgba(
                parse(&hex[2..4])?,
                parse(&hex[4..6])?,
                parse(&hex[6..8])?,
                parse(&hex[0..2])?,
            )),
            _ => None,
        }
    }

    /// 不透明部分转 `#RRGGBB`（alpha < 255 时附 `#AARRGGBB` 形式）。
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.a, self.r, self.g, self.b)
        }
    }

    /// 改 alpha（`alpha` 0..1，越界夹取）。
    pub fn with_alpha(self, alpha: f32) -> Self {
        Self {
            a: (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
            ..self
        }
    }

    /// alpha 归一化到 0..1。
    pub fn alpha(self) -> f32 {
        f32::from(self.a) / 255.0
    }

    /// 是否完全不透明。
    pub fn is_opaque(self) -> bool {
        self.a == 255
    }

    /// WCAG 相对亮度（0..1，即 XYZ 的 `Y` 除以 100）。
    ///
    /// 与 WCAG 2.x 的定义同一条 `Y` 轴（Rec.709 系数 0.2126/0.7152/0.0722），
    /// 所以 [`Color::contrast_ratio`] 就是标准对比度公式。
    pub fn relative_luminance(self) -> f32 {
        (space::xyz_from_color(self)[1] / 100.0) as f32
    }

    /// CIELAB `L*`（0..100）——这就是 M3 的 **tone**。
    ///
    /// 调色板按这条轴生成，验收也按这条轴断言；想知道某个色值「是几号 tone」就用它。
    pub fn lstar(self) -> f32 {
        space::lstar_from_color(self) as f32
    }

    /// WCAG 对比度（1..21）。`self` 与 `other` 都不透明时才严格；带 alpha 时先算「不计 alpha」的值。
    pub fn contrast_ratio(self, other: Self) -> f32 {
        let a = self.relative_luminance();
        let b = other.relative_luminance();
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// alpha 合成：`self` 叠在 `bg` 之上，返回不透明结果。
    ///
    /// 这是 M3 状态层/禁用态/遮罩的唯一合成口径（非预乘 alpha）。
    pub fn over(self, bg: Self) -> Self {
        let sa = self.alpha();
        if sa >= 1.0 {
            return self;
        }
        let blend = |fg: u8, b: u8| {
            let v = f32::from(fg) * sa + f32::from(b) * (1.0 - sa);
            v.round().clamp(0.0, 255.0) as u8
        };
        let a = sa + bg.alpha() * (1.0 - sa);
        Self {
            r: blend(self.r, bg.r),
            g: blend(self.g, bg.g),
            b: blend(self.b, bg.b),
            a: (a * 255.0).round().clamp(0.0, 255.0) as u8,
        }
    }

    /// 两色线性插值（`t` 0..1），用于动画中间帧（GPU 路径的 shape morph 用得上）。
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| {
            (f32::from(a) + (f32::from(b) - f32::from(a)) * t)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Self {
            r: mix(self.r, other.r),
            g: mix(self.g, other.g),
            b: mix(self.b, other.b),
            a: mix(self.a, other.a),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = Color::from_hex("#6750A4").expect("合法 hex");
        assert_eq!(c, Color::rgb(0x67, 0x50, 0xA4));
        assert_eq!(c.to_hex(), "#6750A4");
        assert_eq!(Color::from_hex("6750a4"), Some(c));
        assert_eq!(Color::from_hex("#FFF"), Some(Color::WHITE));
        assert_eq!(Color::from_hex("#80FF0000").map(|c| c.a), Some(0x80));
        assert_eq!(Color::from_hex("#12345"), None);
        assert_eq!(Color::from_hex("zzzzzz"), None);
    }

    #[test]
    fn contrast_extremes() {
        assert!((Color::BLACK.contrast_ratio(Color::WHITE) - 21.0).abs() < 0.01);
        assert!((Color::WHITE.contrast_ratio(Color::WHITE) - 1.0).abs() < 0.001);
    }

    #[test]
    fn over_composites_correctly() {
        let black_50 = Color::BLACK.with_alpha(0.5);
        let got = black_50.over(Color::WHITE);
        // with_alpha 量化到 128/255 = 0.50196，所以结果落在 127（不是 128）。
        assert_eq!((got.r, got.a), (127, 255));
    }

    #[test]
    fn lerp_hits_endpoints() {
        let a = Color::rgb(0, 0, 0);
        let b = Color::rgb(255, 255, 255);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        assert_eq!(a.lerp(b, 0.5).r, 128);
    }

    #[test]
    fn lstar_anchors_at_the_ends() {
        assert!(Color::BLACK.lstar().abs() < 1.0e-3);
        assert!((Color::WHITE.lstar() - 100.0).abs() < 1.0e-3);
        // 中灰（sRGB 128）的 L* 是 53.6 左右，不是 50——这正是「L* 不是 sRGB 编码值」的证据。
        let mid = Color::rgb(128, 128, 128).lstar();
        assert!((mid - 53.585).abs() < 0.01, "中灰 L* = {mid}");
    }

    #[test]
    fn tone_delta_implies_contrast_floor() {
        // M3 的招牌性质：tone 差 50 ⇒ 对比度 ≥ 4.5:1（与色相/彩度无关）。
        for (low, high) in [(10.0_f64, 60.0), (30.0, 80.0), (40.0, 90.0), (20.0, 70.0)] {
            let a = space::color_from_lstar(low);
            let b = space::color_from_lstar(high);
            let ratio = a.contrast_ratio(b);
            assert!(ratio >= 4.5, "tone {low}→{high} 只有 {ratio:.2}");
        }
    }
}
