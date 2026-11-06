//! 官方 M3 **Expressive** 强调排版表（15 档，A 级来源）。
//!
//! 来源：`androidx.compose.material3.tokens.TypeScaleTokens` v0_103 的
//! `<Role>Emphasized{Size,LineHeight,Tracking,Weight}` 四元组。要点：
//!
//! - **字号与行高与 baseline 完全相同**（Expressive 的强调不改变排版高度）；
//! - 只动**字重**与**字距**：display/headline/title-large/body-* → Medium(500)；
//!   title-medium / title-small / label-* → Bold(700)（baseline 是 500）；
//! - display-large 的字距从 −0.2 回 0；body-large 0.5 → 0.15；title-medium 0.2 → 0.15；
//!   body-medium 0.2 → 0.25（唯一一个「强调后字距变大」的档）。

use super::{style, FontFamily, TypeRole, TypeStyle, Weight};

/// 取某档的官方强调样式。非 [`TypeRole`] 的每个变体都有值（15/15，无缺口）。
pub(super) const fn style_of(role: TypeRole) -> TypeStyle {
    match role {
        TypeRole::DisplayLarge => style(57.0, 64.0, Weight::Medium, 0.0, FontFamily::Display),
        TypeRole::DisplayMedium => style(45.0, 52.0, Weight::Medium, 0.0, FontFamily::Display),
        TypeRole::DisplaySmall => style(36.0, 44.0, Weight::Medium, 0.0, FontFamily::Display),
        TypeRole::HeadlineLarge => style(32.0, 40.0, Weight::Medium, 0.0, FontFamily::Display),
        TypeRole::HeadlineMedium => style(28.0, 36.0, Weight::Medium, 0.0, FontFamily::Display),
        TypeRole::HeadlineSmall => style(24.0, 32.0, Weight::Medium, 0.0, FontFamily::Display),
        // TitleLarge 官方用 Brand 族（与 Display 同栈）。
        TypeRole::TitleLarge => style(22.0, 28.0, Weight::Medium, 0.0, FontFamily::Display),
        TypeRole::TitleMedium => style(16.0, 24.0, Weight::Bold, 0.15, FontFamily::Text),
        TypeRole::TitleSmall => style(14.0, 20.0, Weight::Bold, 0.1, FontFamily::Text),
        TypeRole::BodyLarge => style(16.0, 24.0, Weight::Medium, 0.15, FontFamily::Text),
        TypeRole::BodyMedium => style(14.0, 20.0, Weight::Medium, 0.25, FontFamily::Text),
        TypeRole::BodySmall => style(12.0, 16.0, Weight::Medium, 0.4, FontFamily::Text),
        TypeRole::LabelLarge => style(14.0, 20.0, Weight::Bold, 0.1, FontFamily::Text),
        TypeRole::LabelMedium => style(12.0, 16.0, Weight::Bold, 0.5, FontFamily::Text),
        TypeRole::LabelSmall => style(11.0, 16.0, Weight::Bold, 0.5, FontFamily::Text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emphasized_keeps_size_and_line_height() {
        for role in TypeRole::ALL {
            let base = role.style();
            let emph = style_of(role);
            assert_eq!(
                base.size_px,
                emph.size_px,
                "{} 强调档不该改字号",
                role.name()
            );
            assert_eq!(
                base.line_height_px,
                emph.line_height_px,
                "{} 强调档不该改行高",
                role.name()
            );
        }
    }

    #[test]
    fn emphasized_weights_follow_the_official_split() {
        // 官方两档：正文/展示类 → Medium(500)；小标题与标签类 → Bold(700)。
        for role in TypeRole::ALL {
            let expect = match role {
                TypeRole::TitleMedium
                | TypeRole::TitleSmall
                | TypeRole::LabelLarge
                | TypeRole::LabelMedium
                | TypeRole::LabelSmall => Weight::Bold,
                _ => Weight::Medium,
            };
            assert_eq!(
                style_of(role).weight,
                expect,
                "{} 强调档字重不对",
                role.name()
            );
        }
    }

    #[test]
    fn display_large_tracking_returns_to_zero() {
        assert_eq!(TypeRole::DisplayLarge.style().letter_spacing_px, -0.2);
        assert_eq!(TypeRole::DisplayLarge.emphasized().letter_spacing_px, 0.0);
        assert_eq!(TypeRole::BodyLarge.style().letter_spacing_px, 0.5);
        assert_eq!(TypeRole::BodyLarge.emphasized().letter_spacing_px, 0.15);
    }
}
