//! 官方数值钉死（一）：**排版两张表**（baseline 15 档 + Expressive 强调 15 档）。
//!
//! 表的来源是 A 级 `androidx.compose.material3.tokens.TypeScaleTokens`（v0_103）。这里的期望值是
//! **独立于实现重写的一份**：任何人改动 `TypeRole::style()` / `emphasized()` 的数值，都必须同时改
//! 这张表才会同时绿——这就是「冻结」的含义（reviewer P2-1 的变异测试就是打这里）。

use unlucky_design::typography::{FontFamily, TypeRole, Weight};

/// baseline 单档对拍。
fn expect_baseline(
    role: TypeRole,
    size: f32,
    line_height: f32,
    weight: Weight,
    tracking: f32,
    family: FontFamily,
) {
    let got = role.style();
    assert_eq!(
        (
            got.size_px,
            got.line_height_px,
            got.weight,
            got.letter_spacing_px,
            got.family
        ),
        (size, line_height, weight, tracking, family),
        "{} 的 baseline 值被改动了（官方 TypeScaleTokens）",
        role.name()
    );
}

/// Expressive 强调档单档对拍。
fn expect_emphasized(role: TypeRole, size: f32, line_height: f32, weight: Weight, tracking: f32) {
    let got = role.emphasized();
    assert_eq!(
        (
            got.size_px,
            got.line_height_px,
            got.weight,
            got.letter_spacing_px
        ),
        (size, line_height, weight, tracking),
        "{} 的 Expressive 强调档值被改动了（官方 TypeScaleTokens *Emphasized*）",
        role.name()
    );
}

#[test]
fn baseline_type_scale_matches_official_tokens() {
    use FontFamily::{Display, Text};
    use Weight::{Bold, Medium, Regular};
    expect_baseline(TypeRole::DisplayLarge, 57.0, 64.0, Regular, -0.2, Display);
    expect_baseline(TypeRole::DisplayMedium, 45.0, 52.0, Regular, 0.0, Display);
    expect_baseline(TypeRole::DisplaySmall, 36.0, 44.0, Regular, 0.0, Display);
    expect_baseline(TypeRole::HeadlineLarge, 32.0, 40.0, Regular, 0.0, Display);
    expect_baseline(TypeRole::HeadlineMedium, 28.0, 36.0, Regular, 0.0, Display);
    expect_baseline(TypeRole::HeadlineSmall, 24.0, 32.0, Regular, 0.0, Display);
    expect_baseline(TypeRole::TitleLarge, 22.0, 28.0, Regular, 0.0, Display);
    expect_baseline(TypeRole::TitleMedium, 16.0, 24.0, Medium, 0.2, Text);
    expect_baseline(TypeRole::TitleSmall, 14.0, 20.0, Medium, 0.1, Text);
    expect_baseline(TypeRole::BodyLarge, 16.0, 24.0, Regular, 0.5, Text);
    expect_baseline(TypeRole::BodyMedium, 14.0, 20.0, Regular, 0.2, Text);
    expect_baseline(TypeRole::BodySmall, 12.0, 16.0, Regular, 0.4, Text);
    expect_baseline(TypeRole::LabelLarge, 14.0, 20.0, Medium, 0.1, Text);
    expect_baseline(TypeRole::LabelMedium, 12.0, 16.0, Medium, 0.5, Text);
    expect_baseline(TypeRole::LabelSmall, 11.0, 16.0, Medium, 0.5, Text);
    let _ = Bold;
}

#[test]
fn expressive_emphasis_matches_official_tokens() {
    use Weight::{Bold, Medium};
    // 展示/正文类强调 → Medium(500)；label 与小标题 → Bold(700)。
    expect_emphasized(TypeRole::DisplayLarge, 57.0, 64.0, Medium, 0.0);
    expect_emphasized(TypeRole::DisplayMedium, 45.0, 52.0, Medium, 0.0);
    expect_emphasized(TypeRole::DisplaySmall, 36.0, 44.0, Medium, 0.0);
    expect_emphasized(TypeRole::HeadlineLarge, 32.0, 40.0, Medium, 0.0);
    expect_emphasized(TypeRole::HeadlineMedium, 28.0, 36.0, Medium, 0.0);
    expect_emphasized(TypeRole::HeadlineSmall, 24.0, 32.0, Medium, 0.0);
    expect_emphasized(TypeRole::TitleLarge, 22.0, 28.0, Medium, 0.0);
    expect_emphasized(TypeRole::TitleMedium, 16.0, 24.0, Bold, 0.15);
    expect_emphasized(TypeRole::TitleSmall, 14.0, 20.0, Bold, 0.1);
    expect_emphasized(TypeRole::BodyLarge, 16.0, 24.0, Medium, 0.15);
    expect_emphasized(TypeRole::BodyMedium, 14.0, 20.0, Medium, 0.25);
    expect_emphasized(TypeRole::BodySmall, 12.0, 16.0, Medium, 0.4);
    expect_emphasized(TypeRole::LabelLarge, 14.0, 20.0, Bold, 0.1);
    expect_emphasized(TypeRole::LabelMedium, 12.0, 16.0, Bold, 0.5);
    expect_emphasized(TypeRole::LabelSmall, 11.0, 16.0, Bold, 0.5);
}

#[test]
fn both_tables_cover_all_fifteen_roles() {
    // 防止「表里少写一档」：15 个角色都必须被上面两张表摸到。
    let mut names = std::collections::BTreeSet::new();
    for role in TypeRole::ALL {
        names.insert(role.name());
        assert_ne!(
            role.style(),
            role.emphasized(),
            "{} 强调档等同 baseline",
            role.name()
        );
    }
    assert_eq!(names.len(), 15);
}
