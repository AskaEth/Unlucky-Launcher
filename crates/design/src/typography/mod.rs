//! M3 type scale（15 档，baseline + Expressive 强调两套）+ 三平台字体族回退栈。
//!
//! **数值来源 A 级**：`androidx.compose.material3.tokens.TypeScaleTokens`（v0_103）。
//! baseline 四元组取 `<Role>{Size,LineHeight,Tracking,Weight}`，Expressive 强调档取
//! `<Role>Emphasized{...}`（见 [`emphasized`] 模块）。注意 Compose 这一代 token 的 tracking 与
//! m3.material.io 网页表格有 0.05px 级别的差异（display-large −0.2 vs −0.25、body-medium 0.2 vs
//! 0.25、title-medium 0.2 vs 0.15），本项目跟 **Compose（实际渲染值）**，差异登记在
//! crate 文档「baseline vs Expressive 差异清单」里。
//!
//! **只写字体族名，不内嵌任何字体文件**（Segoe UI / SF Pro / Roboto 都受各自许可约束，
//! 内嵌再分发是违规的；引用系统已安装的族名是合规且体积为零的做法——这是 D14 与 D28 的共同要求）。
//! 中文由系统字体匹配：栈里显式列出 CJK 族名，纯英文环境不会去加载任何中文字体（D28）。

mod emphasized;
mod fonts;

pub use fonts::{FontChoice, FontFace, FontFamily, Platform, WeightCoverage, WeightMatch};

/// 字重（M3 只用这四档；可变字体按连续值渲染时取 [`Weight::value`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weight {
    /// 400：正文。
    Regular,
    /// 500：label / title-medium / Expressive 强调的正文与展示类。
    Medium,
    /// 600：本项目的「区块标题」档（介于 Medium 与 Bold 之间）。
    SemiBold,
    /// 700：页面标题 / 空态主文案 / Expressive 强调的 label 与 title-small-medium。
    Bold,
}

impl Weight {
    /// 数值字重（CSS/OpenType 口径）。
    pub const fn value(self) -> u16 {
        match self {
            Weight::Regular => 400,
            Weight::Medium => 500,
            Weight::SemiBold => 600,
            Weight::Bold => 700,
        }
    }

    /// 再重一档（最重封顶 Bold）。
    pub const fn heavier(self) -> Self {
        match self {
            Weight::Regular => Weight::Medium,
            Weight::Medium => Weight::SemiBold,
            Weight::SemiBold | Weight::Bold => Weight::Bold,
        }
    }
}

/// 一条排版样式（已解析成渲染要用的四个数）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeStyle {
    /// 字号（px；1.0 缩放时与 dp 同值）。
    pub size_px: f32,
    /// 行高（px）。
    pub line_height_px: f32,
    /// 字重。
    pub weight: Weight,
    /// 字间距（px；M3 的 type scale 用 px 表达，中文场景下正字距更易读）。
    pub letter_spacing_px: f32,
    /// 用哪条字体栈。
    pub family: FontFamily,
}

impl TypeStyle {
    /// 按用户字号缩放（`factor` 1.0 = 标准；行高与字距同比例，夹取到 0.5..2.0）。
    pub fn scaled(self, factor: f32) -> Self {
        let f = factor.clamp(0.5, 2.0);
        Self {
            size_px: self.size_px * f,
            line_height_px: self.line_height_px * f,
            letter_spacing_px: self.letter_spacing_px * f,
            ..self
        }
    }

    /// 该样式的字体族回退栈（栈项带字重覆盖，见 [`FontFace`]）。
    pub fn font_stack(self, platform: Platform) -> &'static [FontFace] {
        platform.font_stack(self.family)
    }

    /// 该样式在这台平台上「字重最终落到哪个族」——中文缺档时看它判断是否走降级。
    pub fn resolve(self, platform: Platform) -> FontChoice {
        platform.resolve_weight(self.family, self.weight)
    }
}

/// M3 type scale 的 15 个角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeRole {
    /// 57/64：落地页大标题（本启动器基本不用）。
    DisplayLarge,
    /// 45/52。
    DisplayMedium,
    /// 36/44。
    DisplaySmall,
    /// 32/40：空态主标题的大档。
    HeadlineLarge,
    /// 28/36。
    HeadlineMedium,
    /// 24/32。
    HeadlineSmall,
    /// 22/28：页面标题（导航栏下的主标题）。
    TitleLarge,
    /// 16/24：卡片标题。
    TitleMedium,
    /// 14/20：行标题。
    TitleSmall,
    /// 16/24：正文大档（说明文字）。
    BodyLarge,
    /// 14/20：默认正文。
    BodyMedium,
    /// 12/16：辅助说明。
    BodySmall,
    /// 14/20：按钮文字。
    LabelLarge,
    /// 12/16：标签、徽标。
    LabelMedium,
    /// 11/16：最小标签（谨慎使用，中文 11px 可读性差）。
    LabelSmall,
}

impl TypeRole {
    /// 全部 15 档，按 M3 的族序排列。
    pub const ALL: [TypeRole; 15] = [
        TypeRole::DisplayLarge,
        TypeRole::DisplayMedium,
        TypeRole::DisplaySmall,
        TypeRole::HeadlineLarge,
        TypeRole::HeadlineMedium,
        TypeRole::HeadlineSmall,
        TypeRole::TitleLarge,
        TypeRole::TitleMedium,
        TypeRole::TitleSmall,
        TypeRole::BodyLarge,
        TypeRole::BodyMedium,
        TypeRole::BodySmall,
        TypeRole::LabelLarge,
        TypeRole::LabelMedium,
        TypeRole::LabelSmall,
    ];

    /// 稳定字符串名（`display-large` … `label-small`）。
    pub const fn name(self) -> &'static str {
        match self {
            TypeRole::DisplayLarge => "display-large",
            TypeRole::DisplayMedium => "display-medium",
            TypeRole::DisplaySmall => "display-small",
            TypeRole::HeadlineLarge => "headline-large",
            TypeRole::HeadlineMedium => "headline-medium",
            TypeRole::HeadlineSmall => "headline-small",
            TypeRole::TitleLarge => "title-large",
            TypeRole::TitleMedium => "title-medium",
            TypeRole::TitleSmall => "title-small",
            TypeRole::BodyLarge => "body-large",
            TypeRole::BodyMedium => "body-medium",
            TypeRole::BodySmall => "body-small",
            TypeRole::LabelLarge => "label-large",
            TypeRole::LabelMedium => "label-medium",
            TypeRole::LabelSmall => "label-small",
        }
    }

    /// **baseline MD3** 样式（官方 type scale 值：字号 / 行高 / 字重 / 字距 + 字体族分类）。
    pub const fn style(self) -> TypeStyle {
        match self {
            TypeRole::DisplayLarge => style(57.0, 64.0, Weight::Regular, -0.2, FontFamily::Display),
            TypeRole::DisplayMedium => style(45.0, 52.0, Weight::Regular, 0.0, FontFamily::Display),
            TypeRole::DisplaySmall => style(36.0, 44.0, Weight::Regular, 0.0, FontFamily::Display),
            TypeRole::HeadlineLarge => style(32.0, 40.0, Weight::Regular, 0.0, FontFamily::Display),
            TypeRole::HeadlineMedium => {
                style(28.0, 36.0, Weight::Regular, 0.0, FontFamily::Display)
            }
            TypeRole::HeadlineSmall => style(24.0, 32.0, Weight::Regular, 0.0, FontFamily::Display),
            // 官方 TitleLarge 用 Brand 族（与 Display 同栈），其余 title 用 Plain（Text）。
            TypeRole::TitleLarge => style(22.0, 28.0, Weight::Regular, 0.0, FontFamily::Display),
            TypeRole::TitleMedium => style(16.0, 24.0, Weight::Medium, 0.2, FontFamily::Text),
            TypeRole::TitleSmall => style(14.0, 20.0, Weight::Medium, 0.1, FontFamily::Text),
            TypeRole::BodyLarge => style(16.0, 24.0, Weight::Regular, 0.5, FontFamily::Text),
            TypeRole::BodyMedium => style(14.0, 20.0, Weight::Regular, 0.2, FontFamily::Text),
            TypeRole::BodySmall => style(12.0, 16.0, Weight::Regular, 0.4, FontFamily::Text),
            TypeRole::LabelLarge => style(14.0, 20.0, Weight::Medium, 0.1, FontFamily::Text),
            TypeRole::LabelMedium => style(12.0, 16.0, Weight::Medium, 0.5, FontFamily::Text),
            TypeRole::LabelSmall => style(11.0, 16.0, Weight::Medium, 0.5, FontFamily::Text),
        }
    }

    /// **M3 Expressive** 官方强调档（15/15 全有值，A 级来源）。
    ///
    /// 规则（官方）：字号与行高不变；字重升到 500（展示/正文类）或 700（label 与小标题）；
    /// 字距按官方表微调（display-large −0.2→0、body-large 0.5→0.15、title-medium 0.2→0.15、
    /// body-medium 0.2→0.25）。这是**官方值，不是我们的近似**。
    pub const fn emphasized(self) -> TypeStyle {
        emphasized::style_of(self)
    }

    /// **简化强调规则（非官方）**：只把字重升一级，字号/行高/字距全不动。
    ///
    /// 用途：需要在「同一 role 内加强语气但不想引入任何字距变化」的场合（例如同屏多行标题
    /// 需要严格对齐基线网格时）。**优先用 [`TypeRole::emphasized`]**——那才是 Expressive 的定义。
    pub const fn emphasized_simplified(self) -> TypeStyle {
        let base = self.style();
        TypeStyle {
            weight: base.weight.heavier(),
            ..base
        }
    }
}

/// `const fn` 用的构造器。
const fn style(
    size: f32,
    line_height: f32,
    weight: Weight,
    tracking_px: f32,
    family: FontFamily,
) -> TypeStyle {
    TypeStyle {
        size_px: size,
        line_height_px: line_height,
        weight,
        letter_spacing_px: tracking_px,
        family,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_has_sane_metrics() {
        for role in TypeRole::ALL {
            let s = role.style();
            assert!(s.size_px >= 11.0, "{} 字号过小", role.name());
            assert!(
                s.line_height_px >= s.size_px,
                "{} 行高小于字号（{} < {}）",
                role.name(),
                s.line_height_px,
                s.size_px
            );
            assert!(
                (1.1..=1.8).contains(&(s.line_height_px / s.size_px)),
                "{} 行高倍数 {} 超出 1.1..1.8（M3 的行高是 1.12~1.5 量级）",
                role.name(),
                s.line_height_px / s.size_px
            );
            assert!(s.letter_spacing_px.abs() < 1.0, "{} 字距过大", role.name());
        }
    }

    #[test]
    fn each_family_is_monotone_by_size() {
        // M3 的 type scale 是「五族 × 三档」，跨族不保证单调（title-small 14 → body-large 16），
        // 但族内必须由大到小。
        for family in [
            [
                TypeRole::DisplayLarge,
                TypeRole::DisplayMedium,
                TypeRole::DisplaySmall,
            ],
            [
                TypeRole::HeadlineLarge,
                TypeRole::HeadlineMedium,
                TypeRole::HeadlineSmall,
            ],
            [
                TypeRole::TitleLarge,
                TypeRole::TitleMedium,
                TypeRole::TitleSmall,
            ],
            [
                TypeRole::BodyLarge,
                TypeRole::BodyMedium,
                TypeRole::BodySmall,
            ],
            [
                TypeRole::LabelLarge,
                TypeRole::LabelMedium,
                TypeRole::LabelSmall,
            ],
        ] {
            for pair in family.windows(2) {
                assert!(
                    pair[0].style().size_px > pair[1].style().size_px,
                    "{} 应大于 {}",
                    pair[0].name(),
                    pair[1].name()
                );
            }
        }
    }

    #[test]
    fn names_are_unique_and_kebab() {
        let mut seen = std::collections::BTreeSet::new();
        for role in TypeRole::ALL {
            let name = role.name();
            assert!(seen.insert(name), "排版角色名重复：{name}");
            assert!(name.chars().all(|c| c.is_ascii_lowercase() || c == '-'));
        }
    }

    #[test]
    fn simplified_emphasis_only_bumps_weight() {
        for role in TypeRole::ALL {
            let base = role.style();
            let emph = role.emphasized_simplified();
            assert_eq!(base.size_px, emph.size_px);
            assert_eq!(base.line_height_px, emph.line_height_px);
            assert_eq!(base.letter_spacing_px, emph.letter_spacing_px);
            assert!(emph.weight.value() > base.weight.value() || base.weight == Weight::Bold);
        }
    }

    #[test]
    fn official_emphasis_is_at_least_as_heavy_as_baseline() {
        for role in TypeRole::ALL {
            assert!(
                role.emphasized().weight.value() >= role.style().weight.value(),
                "{} 的官方强调档不该比 baseline 更轻",
                role.name()
            );
            assert_ne!(
                role.emphasized(),
                role.style(),
                "{} 强调档与 baseline 相同",
                role.name()
            );
        }
    }

    #[test]
    fn scaling_is_proportional_and_bounded() {
        let base = TypeRole::BodyMedium.style();
        let big = base.scaled(1.25);
        assert!((big.size_px - 17.5).abs() < 1.0e-4);
        assert!((big.line_height_px - 25.0).abs() < 1.0e-4);
        assert_eq!(base.scaled(10.0).size_px, base.size_px * 2.0);
        assert_eq!(base.scaled(0.1).size_px, base.size_px * 0.5);
    }
}
