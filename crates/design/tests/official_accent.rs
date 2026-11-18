//! 自定义强调色 × 官方算法的**逐角色对齐**（`Scheme::from_accent`）。
//!
//! 期望值来自官方 Java 实现：`SchemeTonalSpot(seed, isDark, 0, SPEC_2021)` 的强调族角色。
//! 判据：每通道误差 ≤1（见 `official_baseline.rs` 的说明）。
//!
//! 为什么只比强调族（primary/secondary/tertiary 共 12 个角色）：本项目的产品语义是
//! 「换强调色不动底色」——中性表面与 error 刻意留在 baseline（见 `Palettes::with_accent`），
//! 而官方 `from_seed` 会把中性色也染上种子色相。中性派生的正确性由本文件末尾的
//! `from_seed_*` 用例单独验证（那一处必须与官方完全一致）。

use unlucky_design::color::{Color, Role, Scheme, SchemeFlavor};

/// 每通道允许的最大误差。
const CHANNEL_TOLERANCE: i16 = 1;

/// 一条官方期望值：角色 + light 色值 + dark 色值。
type RoleRow = (Role, &'static str, &'static str);

/// 一个种子 + 它 12 个强调族角色的官方色值。
type SeedRows = (&'static str, [RoleRow; 12]);

/// 八个种子：官方基线紫、纯红/绿/蓝、低彩度灰蓝、高彩度紫，外加纯黑与纯白两个边缘用例
/// （无彩色种子的色相是病态的，官方的处理方式见 `Hct`：黑 → 色相 0、白 → 色相 209.5）。
/// 每个种子带 12 个强调族角色的官方色值（light, dark）。
const ACCENT_SEEDS: [SeedRows; 8] = [
    (
        "#6750A4",
        [
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
        ],
    ),
    (
        "#FF0000",
        [
            (Role::Primary, "#904B40", "#FFB4A8"),
            (Role::OnPrimary, "#FFFFFF", "#561E16"),
            (Role::PrimaryContainer, "#FFDAD4", "#73342A"),
            (Role::OnPrimaryContainer, "#73342A", "#FFDAD4"),
            (Role::Secondary, "#775651", "#E7BDB6"),
            (Role::OnSecondary, "#FFFFFF", "#442925"),
            (Role::SecondaryContainer, "#FFDAD4", "#5D3F3B"),
            (Role::OnSecondaryContainer, "#5D3F3B", "#FFDAD4"),
            (Role::Tertiary, "#705C2E", "#DEC48C"),
            (Role::OnTertiary, "#FFFFFF", "#3E2E04"),
            (Role::TertiaryContainer, "#FBDFA6", "#564419"),
            (Role::OnTertiaryContainer, "#564419", "#FBDFA6"),
        ],
    ),
    (
        "#00FF00",
        [
            (Role::Primary, "#406836", "#A5D395"),
            (Role::OnPrimary, "#FFFFFF", "#11380B"),
            (Role::PrimaryContainer, "#C0EFB0", "#285020"),
            (Role::OnPrimaryContainer, "#285020", "#C0EFB0"),
            (Role::Secondary, "#54634D", "#BBCBB2"),
            (Role::OnSecondary, "#FFFFFF", "#263422"),
            (Role::SecondaryContainer, "#D7E8CD", "#3C4B37"),
            (Role::OnSecondaryContainer, "#3C4B37", "#D7E8CD"),
            (Role::Tertiary, "#386568", "#A0CFD2"),
            (Role::OnTertiary, "#FFFFFF", "#003739"),
            (Role::TertiaryContainer, "#BCEBEE", "#1E4D50"),
            (Role::OnTertiaryContainer, "#1E4D50", "#BCEBEE"),
        ],
    ),
    (
        "#0000FF",
        [
            (Role::Primary, "#555992", "#BEC2FF"),
            (Role::OnPrimary, "#FFFFFF", "#272B60"),
            (Role::PrimaryContainer, "#E0E0FF", "#3E4278"),
            (Role::OnPrimaryContainer, "#3E4278", "#E0E0FF"),
            (Role::Secondary, "#5C5D72", "#C5C4DD"),
            (Role::OnSecondary, "#FFFFFF", "#2E2F42"),
            (Role::SecondaryContainer, "#E1E0F9", "#444559"),
            (Role::OnSecondaryContainer, "#444559", "#E1E0F9"),
            (Role::Tertiary, "#78536B", "#E8B9D5"),
            (Role::OnTertiary, "#FFFFFF", "#46263B"),
            (Role::TertiaryContainer, "#FFD8EE", "#5E3C52"),
            (Role::OnTertiaryContainer, "#5E3C52", "#FFD8EE"),
        ],
    ),
    (
        "#6E7C8A",
        [
            (Role::Primary, "#2B638B", "#98CCF9"),
            (Role::OnPrimary, "#FFFFFF", "#003350"),
            (Role::PrimaryContainer, "#CCE5FF", "#044B71"),
            (Role::OnPrimaryContainer, "#044B71", "#CCE5FF"),
            (Role::Secondary, "#50606F", "#B8C8D9"),
            (Role::OnSecondary, "#FFFFFF", "#23323F"),
            (Role::SecondaryContainer, "#D4E4F6", "#394856"),
            (Role::OnSecondaryContainer, "#394856", "#D4E4F6"),
            (Role::Tertiary, "#66587B", "#D1BFE7"),
            (Role::OnTertiary, "#FFFFFF", "#372A4A"),
            (Role::TertiaryContainer, "#ECDCFF", "#4E4162"),
            (Role::OnTertiaryContainer, "#4E4162", "#ECDCFF"),
        ],
    ),
    (
        "#D000FF",
        [
            (Role::Primary, "#794F81", "#E9B5EF"),
            (Role::OnPrimary, "#FFFFFF", "#472150"),
            (Role::PrimaryContainer, "#FDD6FF", "#603768"),
            (Role::OnPrimaryContainer, "#603768", "#FDD6FF"),
            (Role::Secondary, "#6A596C", "#D6C0D6"),
            (Role::OnSecondary, "#FFFFFF", "#3A2B3C"),
            (Role::SecondaryContainer, "#F3DBF2", "#524153"),
            (Role::OnSecondaryContainer, "#524153", "#F3DBF2"),
            (Role::Tertiary, "#82524C", "#F5B7B0"),
            (Role::OnTertiary, "#FFFFFF", "#4C2521"),
            (Role::TertiaryContainer, "#FFDAD6", "#673B36"),
            (Role::OnTertiaryContainer, "#673B36", "#FFDAD6"),
        ],
    ),
    (
        "#000000",
        [
            (Role::Primary, "#8C4A60", "#FFB1C8"),
            (Role::OnPrimary, "#FFFFFF", "#541D32"),
            (Role::PrimaryContainer, "#FFD9E2", "#703348"),
            (Role::OnPrimaryContainer, "#703348", "#FFD9E2"),
            (Role::Secondary, "#74565F", "#E3BDC6"),
            (Role::OnSecondary, "#FFFFFF", "#422931"),
            (Role::SecondaryContainer, "#FFD9E2", "#5A3F47"),
            (Role::OnSecondaryContainer, "#5A3F47", "#FFD9E2"),
            (Role::Tertiary, "#7C5635", "#EFBD94"),
            (Role::OnTertiary, "#FFFFFF", "#48290B"),
            (Role::TertiaryContainer, "#FFDCC1", "#623F20"),
            (Role::OnTertiaryContainer, "#623F20", "#FFDCC1"),
        ],
    ),
    (
        "#FFFFFF",
        [
            (Role::Primary, "#006874", "#82D3E0"),
            (Role::OnPrimary, "#FFFFFF", "#00363D"),
            (Role::PrimaryContainer, "#9EEFFD", "#004F58"),
            (Role::OnPrimaryContainer, "#004F58", "#9EEFFD"),
            (Role::Secondary, "#4A6267", "#B1CBD0"),
            (Role::OnSecondary, "#FFFFFF", "#1C3438"),
            (Role::SecondaryContainer, "#CDE7EC", "#334B4F"),
            (Role::OnSecondaryContainer, "#334B4F", "#CDE7EC"),
            (Role::Tertiary, "#525E7D", "#BAC6EA"),
            (Role::OnTertiary, "#FFFFFF", "#24304D"),
            (Role::TertiaryContainer, "#DAE2FF", "#3B4664"),
            (Role::OnTertiaryContainer, "#3B4664", "#DAE2FF"),
        ],
    ),
];

/// 官方 `SchemeTonalSpot(seed)` 的中性表面（surface 与 surface-container-highest，light/dark）。
const SEED_NEUTRALS: [(&str, &str, &str, &str, &str); 8] = [
    ("#6750A4", "#FDF7FF", "#141218", "#E6E0E9", "#36343A"),
    ("#FF0000", "#FFF8F6", "#1A1110", "#F1DFDC", "#3D3230"),
    ("#00FF00", "#F8FBF1", "#11140F", "#E1E4DA", "#32362F"),
    ("#0000FF", "#FBF8FF", "#131318", "#E4E1E9", "#34343A"),
    ("#6E7C8A", "#F7F9FF", "#101418", "#E0E2E8", "#313539"),
    ("#D000FF", "#FFF7FA", "#171217", "#EAE0E7", "#393338"),
    ("#000000", "#FFF8F8", "#191113", "#EFDFE1", "#3C3235"),
    ("#FFFFFF", "#F5FAFB", "#0E1415", "#DEE3E5", "#303637"),
];

fn worst_channel(got: Color, want: Color) -> i16 {
    [(got.r, want.r), (got.g, want.g), (got.b, want.b)]
        .iter()
        .map(|(a, b)| (i16::from(*a) - i16::from(*b)).abs())
        .max()
        .unwrap_or(0)
}

#[test]
fn accent_schemes_match_official_on_the_accent_families() {
    let mut worst = 0;
    let mut off_by_one = 0;
    let mut checked = 0;
    let mut failures = Vec::new();
    for (seed_hex, roles) in ACCENT_SEEDS {
        let accent = Color::from_hex(seed_hex).expect("合法 hex");
        for (flavor, is_dark) in [(SchemeFlavor::Light, false), (SchemeFlavor::Dark, true)] {
            let scheme = Scheme::from_accent(flavor, accent);
            for (role, want_light, want_dark) in roles {
                let want_hex = if is_dark { want_dark } else { want_light };
                let got = scheme.get(role);
                let want = Color::from_hex(want_hex).expect("官方 hex 必须合法");
                let delta = worst_channel(got, want);
                worst = worst.max(delta);
                checked += 1;
                if delta > 0 {
                    off_by_one += 1;
                }
                if delta > CHANNEL_TOLERANCE {
                    failures.push(format!(
                        "{seed_hex}/{}/{} 是 {}，官方 {want_hex}（差 {delta}）",
                        flavor.name(),
                        role.name(),
                        got.to_hex()
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} 项强调色与官方不一致（容差 {CHANNEL_TOLERANCE}）：\n{}",
        failures.len(),
        failures.join("\n")
    );
    println!("强调色对齐：{checked} 项，最大通道误差 {worst}，非零误差 {off_by_one} 项");
}

#[test]
fn accent_scheme_keeps_baseline_surfaces() {
    // 产品语义：换强调色不许让底色变脏（这是 `from_accent` 与官方 `from_seed` 的唯一差别）。
    let base = Scheme::baseline(SchemeFlavor::Light);
    for (seed_hex, _) in ACCENT_SEEDS {
        let accent = Color::from_hex(seed_hex).expect("合法 hex");
        let custom = Scheme::from_accent(SchemeFlavor::Light, accent);
        assert_eq!(custom.get(Role::Surface), base.get(Role::Surface));
        assert_eq!(custom.get(Role::Outline), base.get(Role::Outline));
        assert_eq!(custom.get(Role::Error), base.get(Role::Error));
    }
}

#[test]
fn from_seed_matches_official_neutrals() {
    // 全量 `from_seed` 的中性派生必须与官方一致（否则「整窗染色」的色相就是错的）。
    for (seed_hex, light_surface, dark_surface, light_highest, dark_highest) in SEED_NEUTRALS {
        let seed = Color::from_hex(seed_hex).expect("合法 hex");
        for (flavor, want_surface, want_highest) in [
            (SchemeFlavor::Light, light_surface, light_highest),
            (SchemeFlavor::Dark, dark_surface, dark_highest),
        ] {
            let scheme = Scheme::from_seed(flavor, seed);
            for (role, want_hex) in [
                (Role::Surface, want_surface),
                (Role::SurfaceContainerHighest, want_highest),
            ] {
                let got = scheme.get(role);
                let want = Color::from_hex(want_hex).expect("官方 hex 必须合法");
                assert!(
                    worst_channel(got, want) <= CHANNEL_TOLERANCE,
                    "{seed_hex}/{}/{} 是 {}，官方 {want_hex}",
                    flavor.name(),
                    role.name(),
                    got.to_hex()
                );
            }
        }
    }
}

#[test]
fn from_seed_and_from_accent_agree_on_the_accent_families() {
    // 两个入口的强调族应当逐像素相同：换种子时中性色不同，强调色只取决于种子色相。
    for (seed_hex, _) in ACCENT_SEEDS {
        let seed = Color::from_hex(seed_hex).expect("合法 hex");
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            let by_seed = Scheme::from_seed(flavor, seed);
            let by_accent = Scheme::from_accent(flavor, seed);
            for role in [
                Role::Primary,
                Role::OnPrimary,
                Role::PrimaryContainer,
                Role::OnPrimaryContainer,
                Role::Secondary,
                Role::SecondaryContainer,
                Role::Tertiary,
                Role::TertiaryContainer,
            ] {
                assert_eq!(
                    by_seed.get(role),
                    by_accent.get(role),
                    "{seed_hex}/{} 的 {} 两个入口不一致",
                    flavor.name(),
                    role.name()
                );
            }
        }
    }
}

#[test]
fn accent_hue_is_not_clipped_away() {
    // 旧缺陷②：强调色色相被 gamut 硬裁导致偏移。现在色相由 HCT 保住（只压彩度）。
    use unlucky_design::color::Hct;
    for (seed_hex, _) in ACCENT_SEEDS {
        let seed = Color::from_hex(seed_hex).expect("合法 hex");
        let seed_hue = Hct::from_color(seed).hue;
        let primary = Scheme::from_accent(SchemeFlavor::Light, seed).get(Role::Primary);
        let got = Hct::from_color(primary).hue;
        let delta = (got - seed_hue).abs().min(360.0 - (got - seed_hue).abs());
        assert!(delta < 1.0, "{seed_hex} 的 primary 色相偏了 {delta}°");
    }
}
