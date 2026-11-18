//! 与官方 `material-color-utilities` 的**逐角色对齐**（baseline 方案）。
//!
//! 期望值不是「我们觉得对的颜色」，而是把官方 Java 实现（Apache-2.0）编译跑出来的结果：
//! `new SchemeTonalSpot(Hct.fromInt(0xFF6750A4), isDark, 0.0, SpecVersion.SPEC_2021, Platform.PHONE)`
//! 配上 `ColorSpec2021` 的 49 个动态色。跑法见 `NOTICE` 的归属声明——期望值表就是那份输出的
//! 抄本（`seed,flavor,role,hex`）。
//!
//! 判据是**每通道误差 ≤1**：8 bit 量化本身就有半格误差，再加上 Java `double` 与 Rust `f64`
//! 的末位差异，1 是「同一个算法」的合理上限；2 以上就是实现真的不一样了。

use unlucky_design::color::{Color, Hct, Role, Scheme, SchemeFlavor, BASELINE_SEED};

/// 每通道允许的最大误差。
const CHANNEL_TOLERANCE: i16 = 1;

/// 官方 baseline 输出（seed `#6750A4`，Tonal Spot，ColorSpec2021，contrastLevel 0）。
const BASELINE: [(Role, &str, &str); 49] = [
    (Role::Primary, "#65558F", "#CFBDFE"),
    (Role::OnPrimary, "#FFFFFF", "#36275D"),
    (Role::PrimaryContainer, "#E9DDFF", "#4D3D75"),
    (Role::OnPrimaryContainer, "#4D3D75", "#E9DDFF"),
    (Role::Secondary, "#625B71", "#CBC2DB"),
    (Role::OnSecondary, "#FFFFFF", "#332D41"),
    (Role::SecondaryContainer, "#E8DEF8", "#4A4458"),
    (Role::OnSecondaryContainer, "#4A4458", "#E8DEF8"),
    (Role::Tertiary, "#7E5260", "#EFB8C8"),
    (Role::OnTertiary, "#FFFFFF", "#4A2532"),
    (Role::TertiaryContainer, "#FFD9E3", "#633B48"),
    (Role::OnTertiaryContainer, "#633B48", "#FFD9E3"),
    (Role::Error, "#BA1A1A", "#FFB4AB"),
    (Role::OnError, "#FFFFFF", "#690005"),
    (Role::ErrorContainer, "#FFDAD6", "#93000A"),
    (Role::OnErrorContainer, "#93000A", "#FFDAD6"),
    (Role::Surface, "#FDF7FF", "#141218"),
    (Role::OnSurface, "#1D1B20", "#E6E0E9"),
    (Role::SurfaceVariant, "#E7E0EB", "#49454E"),
    (Role::OnSurfaceVariant, "#49454E", "#CAC4CF"),
    (Role::Background, "#FDF7FF", "#141218"),
    (Role::OnBackground, "#1D1B20", "#E6E0E9"),
    (Role::SurfaceDim, "#DED8E0", "#141218"),
    (Role::SurfaceBright, "#FDF7FF", "#3B383E"),
    (Role::SurfaceContainerLowest, "#FFFFFF", "#0F0D13"),
    (Role::SurfaceContainerLow, "#F8F2FA", "#1D1B20"),
    (Role::SurfaceContainer, "#F2ECF4", "#211F24"),
    (Role::SurfaceContainerHigh, "#ECE6EE", "#2B292F"),
    (Role::SurfaceContainerHighest, "#E6E0E9", "#36343A"),
    (Role::Outline, "#7A757F", "#948F99"),
    (Role::OutlineVariant, "#CAC4CF", "#49454E"),
    (Role::InverseSurface, "#322F35", "#E6E0E9"),
    (Role::InverseOnSurface, "#F5EFF7", "#322F35"),
    (Role::InversePrimary, "#CFBDFE", "#65558F"),
    (Role::Shadow, "#000000", "#000000"),
    (Role::Scrim, "#000000", "#000000"),
    (Role::SurfaceTint, "#65558F", "#CFBDFE"),
    (Role::PrimaryFixed, "#E9DDFF", "#E9DDFF"),
    (Role::PrimaryFixedDim, "#CFBDFE", "#CFBDFE"),
    (Role::OnPrimaryFixed, "#201047", "#201047"),
    (Role::OnPrimaryFixedVariant, "#4D3D75", "#4D3D75"),
    (Role::SecondaryFixed, "#E8DEF8", "#E8DEF8"),
    (Role::SecondaryFixedDim, "#CBC2DB", "#CBC2DB"),
    (Role::OnSecondaryFixed, "#1E192B", "#1E192B"),
    (Role::OnSecondaryFixedVariant, "#4A4458", "#4A4458"),
    (Role::TertiaryFixed, "#FFD9E3", "#FFD9E3"),
    (Role::TertiaryFixedDim, "#EFB8C8", "#EFB8C8"),
    (Role::OnTertiaryFixed, "#31101D", "#31101D"),
    (Role::OnTertiaryFixedVariant, "#633B48", "#633B48"),
];

/// 官方 `Hct.from(hue, chroma, tone)` 的色值（求解器与 gamut 映射的直接证据）。
const HCT_PROBES: [(f64, f64, f64, &str); 12] = [
    (95.0, 36.0, 40.0, "#705D0D"),
    (95.0, 36.0, 90.0, "#FCE186"),
    (95.0, 16.0, 90.0, "#F0E2BB"),
    (96.0, 24.0, 80.0, "#D7C68D"),
    (25.0, 84.0, 40.0, "#BA1A1A"),
    (0.0, 0.0, 50.0, "#777777"),
    (200.0, 120.0, 70.0, "#00BEC3"),
    (30.0, 0.5, 98.0, "#FDF8F7"),
    (270.0, 200.0, 10.0, "#001847"),
    (120.0, 60.0, 99.0, "#FCFFDD"),
    (60.0, 6.0, 6.0, "#19120C"),
    (95.0, 36.0, 30.0, "#554500"),
];

/// 三通道里最大的绝对误差。
fn worst_channel(got: Color, want: Color) -> i16 {
    let pairs = [(got.r, want.r), (got.g, want.g), (got.b, want.b)];
    pairs
        .iter()
        .map(|(a, b)| (i16::from(*a) - i16::from(*b)).abs())
        .max()
        .unwrap_or(0)
}

#[test]
fn baseline_matches_official_on_all_forty_nine_roles() {
    // 注意：对齐的是**官方锚点**（种子 #6750A4 + 中性彩度 6），不是产品默认（M70 起是浅冷蓝）。
    let light = Scheme::official_baseline(SchemeFlavor::Light);
    let dark = Scheme::official_baseline(SchemeFlavor::Dark);
    let mut worst = 0;
    let mut off_by_one = 0;
    let mut failures = Vec::new();
    for (role, want_light, want_dark) in BASELINE {
        for (flavor, scheme, want_hex) in [
            (SchemeFlavor::Light, light, want_light),
            (SchemeFlavor::Dark, dark, want_dark),
        ] {
            let got = scheme.get(role);
            let want = Color::from_hex(want_hex).expect("官方 hex 必须合法");
            let delta = worst_channel(got, want);
            worst = worst.max(delta);
            if delta > 0 {
                off_by_one += 1;
            }
            if delta > CHANNEL_TOLERANCE {
                failures.push(format!(
                    "{}/{} 是 {}，官方 {want_hex}（差 {delta}）",
                    flavor.name(),
                    role.name(),
                    got.to_hex()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} 个角色与官方不一致（容差 {CHANNEL_TOLERANCE}）：\n{}",
        failures.len(),
        failures.join("\n")
    );
    // 把误差统计打出来（`cargo test -- --nocapture` 可见），报告里要引用。
    println!("baseline 对齐：98 项，最大通道误差 {worst}，非零误差 {off_by_one} 项");
}

#[test]
fn hct_solver_matches_official_color_spec_2021() {
    for (hue, chroma, tone, want_hex) in HCT_PROBES {
        let got = Hct::from_components(hue, chroma, tone).to_color();
        let want = Color::from_hex(want_hex).expect("官方 hex 必须合法");
        assert!(
            worst_channel(got, want) <= CHANNEL_TOLERANCE,
            "HCT({hue}, {chroma}, {tone}) 是 {}，官方 {want_hex}",
            got.to_hex()
        );
    }
}

#[test]
fn baseline_primary_is_not_the_seed_and_that_is_official() {
    // 常见误解：官方基线的 primary 应该等于种子 #6750A4。官方不是——Tonal Spot 变体把
    // primary 的彩度固定成 36，而 #6750A4 的 HCT 彩度是 47.86，所以 primary 被收到 #65558F。
    // 这也是 Flutter `ColorScheme.fromSeed(0xFF6750A4)` 的 primary 值。
    let seed = Hct::from_color(BASELINE_SEED);
    assert!(
        (seed.chroma - 47.856_526).abs() < 1.0e-4,
        "种子彩度 {}",
        seed.chroma
    );
    let primary = Scheme::official_baseline(SchemeFlavor::Light).get(Role::Primary);
    assert_eq!(primary.to_hex(), "#65558F");
    assert_ne!(primary, BASELINE_SEED);
}

#[test]
fn baseline_tone_axis_is_still_exact_lstar() {
    // gamut 映射只动彩度，不动 tone：每个角色的实测 L* 与它声明的 tone 相差 ≤1。
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let scheme = Scheme::official_baseline(flavor);
        for (role, color) in scheme.iter() {
            let tone = Scheme::tone_of(role, flavor);
            let got = f64::from(color.lstar());
            assert!(
                (got - tone).abs() <= 1.0,
                "{}/{} 的 tone 声明 {tone}、实测 {got}",
                flavor.name(),
                role.name()
            );
        }
    }
}

#[test]
fn same_family_containers_are_separated_by_hct_chroma() {
    // 旧缺陷①：light 下 primary-container 与 secondary-container 撞色。
    // 现在两者彩度不同（36 vs 16），在 tone 90 上都不会被色域裁到同一个上限。
    let light = Scheme::official_baseline(SchemeFlavor::Light);
    let primary = light.get(Role::PrimaryContainer);
    let secondary = light.get(Role::SecondaryContainer);
    assert_ne!(primary, secondary);
    let c_primary = Hct::from_color(primary).chroma;
    let c_secondary = Hct::from_color(secondary).chroma;
    assert!(
        c_secondary < c_primary,
        "secondary-container 彩度 {c_secondary} 应低于 primary-container {c_primary}"
    );
    // 深层原因：tone 90 上彩度 36 装得下，所以容器色是「真的有彩度的浅色」而不是被裁到灰。
    assert!(c_primary > 12.0, "primary-container 彩度只剩 {c_primary}");
}
