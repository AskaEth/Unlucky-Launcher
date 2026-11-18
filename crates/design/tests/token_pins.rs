//! 官方数值钉死（二）：**色彩 tone 表 49 条**、7 条 easing 曲线、16 档时长、28 个图标下标、
//! 形状阶梯，以及「同族容器不撞色」的回归（reviewer P3-2）。
//!
//! 期望值全部独立于实现重写；实现改一个数、表不改，这里就红。

use unlucky_design::color::{Role, Scheme, SchemeFlavor};
use unlucky_design::icons::IconKind;
use unlucky_design::motion::{DurationToken, Easing};
use unlucky_design::shape::ShapeScale;

fn expect_tone(role: Role, light: f64, dark: f64) {
    assert_eq!(
        Scheme::tone_of(role, SchemeFlavor::Light),
        light,
        "{} 的 light tone 被改动了",
        role.name()
    );
    assert_eq!(
        Scheme::tone_of(role, SchemeFlavor::Dark),
        dark,
        "{} 的 dark tone 被改动了",
        role.name()
    );
}

#[test]
fn all_forty_nine_tones_are_pinned() {
    expect_tone(Role::Primary, 40.0, 80.0);
    expect_tone(Role::OnPrimary, 100.0, 20.0);
    expect_tone(Role::PrimaryContainer, 90.0, 30.0);
    expect_tone(Role::OnPrimaryContainer, 30.0, 90.0);
    expect_tone(Role::Secondary, 40.0, 80.0);
    expect_tone(Role::OnSecondary, 100.0, 20.0);
    expect_tone(Role::SecondaryContainer, 90.0, 30.0);
    expect_tone(Role::OnSecondaryContainer, 30.0, 90.0);
    expect_tone(Role::Tertiary, 40.0, 80.0);
    expect_tone(Role::OnTertiary, 100.0, 20.0);
    expect_tone(Role::TertiaryContainer, 90.0, 30.0);
    expect_tone(Role::OnTertiaryContainer, 30.0, 90.0);
    expect_tone(Role::Error, 40.0, 80.0);
    expect_tone(Role::OnError, 100.0, 20.0);
    expect_tone(Role::ErrorContainer, 90.0, 30.0);
    expect_tone(Role::OnErrorContainer, 30.0, 90.0);
    expect_tone(Role::Surface, 98.0, 6.0);
    expect_tone(Role::OnSurface, 10.0, 90.0);
    expect_tone(Role::SurfaceVariant, 90.0, 30.0);
    expect_tone(Role::OnSurfaceVariant, 30.0, 80.0);
    expect_tone(Role::Background, 98.0, 6.0);
    expect_tone(Role::OnBackground, 10.0, 90.0);
    expect_tone(Role::SurfaceDim, 87.0, 6.0);
    expect_tone(Role::SurfaceBright, 98.0, 24.0);
    expect_tone(Role::SurfaceContainerLowest, 100.0, 4.0);
    expect_tone(Role::SurfaceContainerLow, 96.0, 10.0);
    expect_tone(Role::SurfaceContainer, 94.0, 12.0);
    expect_tone(Role::SurfaceContainerHigh, 92.0, 17.0);
    expect_tone(Role::SurfaceContainerHighest, 90.0, 22.0);
    expect_tone(Role::Outline, 50.0, 60.0);
    expect_tone(Role::OutlineVariant, 80.0, 30.0);
    expect_tone(Role::InverseSurface, 20.0, 90.0);
    expect_tone(Role::InverseOnSurface, 95.0, 20.0);
    expect_tone(Role::InversePrimary, 80.0, 40.0);
    expect_tone(Role::Shadow, 0.0, 0.0);
    expect_tone(Role::Scrim, 0.0, 0.0);
    expect_tone(Role::SurfaceTint, 40.0, 80.0);
    expect_tone(Role::PrimaryFixed, 90.0, 90.0);
    expect_tone(Role::PrimaryFixedDim, 80.0, 80.0);
    expect_tone(Role::OnPrimaryFixed, 10.0, 10.0);
    expect_tone(Role::OnPrimaryFixedVariant, 30.0, 30.0);
    expect_tone(Role::SecondaryFixed, 90.0, 90.0);
    expect_tone(Role::SecondaryFixedDim, 80.0, 80.0);
    expect_tone(Role::OnSecondaryFixed, 10.0, 10.0);
    expect_tone(Role::OnSecondaryFixedVariant, 30.0, 30.0);
    expect_tone(Role::TertiaryFixed, 90.0, 90.0);
    expect_tone(Role::TertiaryFixedDim, 80.0, 80.0);
    expect_tone(Role::OnTertiaryFixed, 10.0, 10.0);
    expect_tone(Role::OnTertiaryFixedVariant, 30.0, 30.0);
}

#[test]
fn easing_curves_are_pinned() {
    let cases: [(Easing, f32, f32, f32, f32); 7] = [
        (Easing::Linear, 0.0, 0.0, 1.0, 1.0),
        (Easing::Standard, 0.2, 0.0, 0.0, 1.0),
        (Easing::StandardDecelerate, 0.0, 0.0, 0.0, 1.0),
        (Easing::StandardAccelerate, 0.3, 0.0, 1.0, 1.0),
        (Easing::Emphasized, 0.2, 0.0, 0.0, 1.0),
        (Easing::EmphasizedDecelerate, 0.05, 0.7, 0.1, 1.0),
        (Easing::EmphasizedAccelerate, 0.3, 0.0, 0.8, 0.15),
    ];
    for (easing, x1, y1, x2, y2) in cases {
        let curve = easing.curve();
        assert_eq!(
            (curve.x1, curve.y1, curve.x2, curve.y2),
            (x1, y1, x2, y2),
            "{easing:?} 的曲线参数被改动了"
        );
    }
}

#[test]
fn durations_are_pinned() {
    let cases: [(DurationToken, u32); 16] = [
        (DurationToken::Short1, 50),
        (DurationToken::Short2, 100),
        (DurationToken::Short3, 150),
        (DurationToken::Short4, 200),
        (DurationToken::Medium1, 250),
        (DurationToken::Medium2, 300),
        (DurationToken::Medium3, 350),
        (DurationToken::Medium4, 400),
        (DurationToken::Long1, 450),
        (DurationToken::Long2, 500),
        (DurationToken::Long3, 550),
        (DurationToken::Long4, 600),
        (DurationToken::ExtraLong1, 700),
        (DurationToken::ExtraLong2, 800),
        (DurationToken::ExtraLong3, 900),
        (DurationToken::ExtraLong4, 1000),
    ];
    for (token, ms) in cases {
        assert_eq!(token.ms(), ms, "{} 的毫秒值被改动了", token.name());
    }
}

#[test]
fn spring_presets_are_pinned() {
    use unlucky_design::motion::{
        spring, MotionSchemeKind, Spring, SpringCategory, SpringSpeed, SPRING_EFFECTS_DEFAULT,
        SPRING_EFFECTS_FAST, SPRING_EFFECTS_SLOW, SPRING_EXPRESSIVE_SPATIAL_DEFAULT,
        SPRING_EXPRESSIVE_SPATIAL_FAST, SPRING_EXPRESSIVE_SPATIAL_SLOW,
        SPRING_STANDARD_SPATIAL_DEFAULT, SPRING_STANDARD_SPATIAL_FAST,
        SPRING_STANDARD_SPATIAL_SLOW,
    };
    // A 级：StandardMotionTokens / ExpressiveMotionTokens（damping = 阻尼比 ζ，stiffness = k）。
    let cases: [(Spring, f32, f32); 9] = [
        // standard 方案 spatial：ζ=0.9（保守）
        (SPRING_STANDARD_SPATIAL_FAST, 1400.0, 0.9),
        (SPRING_STANDARD_SPATIAL_DEFAULT, 700.0, 0.9),
        (SPRING_STANDARD_SPATIAL_SLOW, 300.0, 0.9),
        // expressive 方案 spatial：ζ=0.6/0.8（回弹明显）
        (SPRING_EXPRESSIVE_SPATIAL_FAST, 800.0, 0.6),
        (SPRING_EXPRESSIVE_SPATIAL_DEFAULT, 380.0, 0.8),
        (SPRING_EXPRESSIVE_SPATIAL_SLOW, 200.0, 0.8),
        // effects 两套同值：ζ=1.0（不过冲）
        (SPRING_EFFECTS_FAST, 3800.0, 1.0),
        (SPRING_EFFECTS_DEFAULT, 1600.0, 1.0),
        (SPRING_EFFECTS_SLOW, 800.0, 1.0),
    ];
    for (entry, stiffness, damping_ratio) in cases {
        assert_eq!(entry.stiffness, stiffness, "弹簧刚度被改动");
        assert_eq!(entry.damping_ratio, damping_ratio, "弹簧阻尼比被改动");
    }
    // 二维表查询与常量必须一致（改了表结构也别想绕过 pin）。
    assert_eq!(
        spring(
            MotionSchemeKind::Expressive,
            SpringCategory::Spatial,
            SpringSpeed::Default
        ),
        SPRING_EXPRESSIVE_SPATIAL_DEFAULT
    );
    assert_eq!(
        spring(
            MotionSchemeKind::Standard,
            SpringCategory::Spatial,
            SpringSpeed::Fast
        ),
        SPRING_STANDARD_SPATIAL_FAST
    );
}

#[test]
fn icon_indices_are_pinned() {
    let names = [
        "instances",
        "download",
        "logs",
        "settings",
        "ai",
        "play",
        "stop",
        "search",
        "plus",
        "close",
        "minimize",
        "maximize",
        "chevron-left",
        "chevron-right",
        "chevron-down",
        "expand",
        "send",
        "check",
        "warn",
        "error",
        "more",
        "sidebar",
        "restore",
        "user",
        "refresh",
        "filter",
        "folder",
        "trash",
    ];
    assert_eq!(IconKind::ALL.len(), names.len());
    for (i, kind) in IconKind::ALL.iter().enumerate() {
        assert_eq!(
            kind.name(),
            names[i],
            "第 {i} 个图标被换位/改名了（序号即持久化 id）"
        );
    }
}

#[test]
fn shape_ladder_is_pinned() {
    let cases: [(ShapeScale, f32); 9] = [
        (ShapeScale::None, 0.0),
        (ShapeScale::ExtraSmall, 4.0),
        (ShapeScale::Small, 8.0),
        (ShapeScale::Medium, 12.0),
        (ShapeScale::Large, 16.0),
        (ShapeScale::LargeIncreased, 20.0),
        (ShapeScale::ExtraLarge, 28.0),
        (ShapeScale::ExtraLargeIncreased, 32.0),
        (ShapeScale::ExtraExtraLarge, 48.0),
    ];
    for (scale, dp) in cases {
        assert_eq!(scale.dp(), dp, "{scale:?} 的半径档位被改动了");
    }
    assert_eq!(ShapeScale::Full.radius_px(64.0), 32.0);
}

#[test]
fn same_family_containers_do_not_collide() {
    // P3-2 回归：light 模式下 primary-container / secondary-container 曾经被 gamut 裁剪成同色。
    let light = Scheme::baseline(SchemeFlavor::Light);
    let primary = light.get(Role::PrimaryContainer);
    let secondary = light.get(Role::SecondaryContainer);
    assert_ne!(
        primary, secondary,
        "同族容器撞色（彩度压缩没生效）：{primary:?}"
    );
    // secondary-container 必须比 primary-container 更灰（M3 的「次级」语义）。
    let c_primary = unlucky_design::color::Hct::from_color(primary).chroma;
    let c_secondary = unlucky_design::color::Hct::from_color(secondary).chroma;
    assert!(
        c_secondary < c_primary,
        "secondary-container 彩度 {c_secondary} 应低于 primary-container {c_primary}"
    );
    // tertiary 与 primary 也该可区分（不同色相）。
    assert_ne!(light.get(Role::TertiaryContainer), primary);
}
