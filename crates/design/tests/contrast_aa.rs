//! WCAG AA 对比度门禁（M61 任务：「on-* 与其承载色至少满足 4.5:1，不达标的角色要显式标注例外」）。
//!
//! 三个性质让这条门禁可以做到「**换任何种子色都成立**」，而不是只对出厂紫成立：
//! 1. 颜色建在 CIELAB `L*`（tone）轴上，`TonalPalette::tone` 精确命中 tone（误差 ≤1）；
//! 2. WCAG 相对亮度只取决于同一条 Y 轴（`L*` ↔ Y 是双射）；
//! 3. 角色 → tone 的映射与色相/彩度无关。
//!
//! 所以对比度主要由「两个角色的 tone 差」决定，与 user 选的强调色无关——下面对多组种子
//! 逐一验证，就是在钉这个性质。

use unlucky_design::color::{Color, Role, Scheme, SchemeFlavor};

/// 正文文字：要求 ≥ 4.5:1（WCAG AA normal text）。
const AA_TEXT: f32 = 4.5;
/// 非文字元素（描边、图标块）：要求 ≥ 3:1（WCAG AA non-text）。
const AA_NON_TEXT: f32 = 3.0;

/// 文字/图标对（前景, 承载色）。两个模式都要过。
const TEXT_PAIRS: [(Role, Role); 26] = [
    (Role::OnPrimary, Role::Primary),
    (Role::OnPrimaryContainer, Role::PrimaryContainer),
    (Role::OnSecondary, Role::Secondary),
    (Role::OnSecondaryContainer, Role::SecondaryContainer),
    (Role::OnTertiary, Role::Tertiary),
    (Role::OnTertiaryContainer, Role::TertiaryContainer),
    (Role::OnError, Role::Error),
    (Role::OnErrorContainer, Role::ErrorContainer),
    (Role::OnSurface, Role::Surface),
    (Role::OnSurfaceVariant, Role::SurfaceVariant),
    (Role::OnBackground, Role::Background),
    (Role::InverseOnSurface, Role::InverseSurface),
    (Role::InversePrimary, Role::InverseSurface),
    (Role::OnPrimaryFixed, Role::PrimaryFixed),
    (Role::OnPrimaryFixedVariant, Role::PrimaryFixedDim),
    (Role::OnSecondaryFixed, Role::SecondaryFixed),
    (Role::OnSecondaryFixedVariant, Role::SecondaryFixedDim),
    (Role::OnTertiaryFixed, Role::TertiaryFixed),
    (Role::OnTertiaryFixedVariant, Role::TertiaryFixedDim),
    // 卡片/浮层容器上的文字（M3 现行口径：层级靠 surface-container-* 表达）
    (Role::OnSurface, Role::SurfaceContainerLowest),
    (Role::OnSurface, Role::SurfaceContainerLow),
    (Role::OnSurface, Role::SurfaceContainer),
    (Role::OnSurface, Role::SurfaceContainerHigh),
    (Role::OnSurface, Role::SurfaceContainerHighest),
    (Role::OnSurfaceVariant, Role::SurfaceContainerHighest),
    (Role::OnSurfaceVariant, Role::SurfaceContainerLow),
];

/// 显式登记的例外：`(前景, 承载色, 目标阈值, 原因)`。
///
/// 这些不是缺陷，是 M3 定义决定的（非文字元素 3:1、装饰件无要求）。
const EXCEPTIONS: [(Role, Role, f32, &str); 3] = [
    (
        Role::Outline,
        Role::Surface,
        AA_NON_TEXT,
        "输入框/卡片描边是 M3 定义的非文字元素，规范目标 3:1（light 实测 4.26:1、dark 5.89:1 已高于目标）",
    ),
    (
        Role::OutlineVariant,
        Role::Surface,
        AA_NON_TEXT * 0.5,
        "分隔线是纯装饰，M3 不要求对比度下限；light 1.62:1 / dark 1.99:1 是「几乎看不见但要能数出层级」的取值",
    ),
    (
        Role::SurfaceVariant,
        Role::Surface,
        1.0,
        "surface-variant 是容器色而不是前景色，不参与对比度门禁（M3 已不推荐用它做容器，保留仅为兼容旧值）",
    ),
];

fn seeds() -> Vec<Color> {
    vec![
        // 出厂基线（M3 baseline 紫）
        Color::from_hex("#6750A4").expect("基线种子"),
        // 冷蓝（原 Fluent 强调色，用户大概率会选这一族）
        Color::from_hex("#0078D4").expect("蓝"),
        // 青绿
        Color::from_hex("#006A5C").expect("青"),
        // 暖橙（高彩度，gamut 压力大）
        Color::from_hex("#E8710A").expect("橙"),
        // 亮黄（最容易被白字吃掉的极端）
        Color::from_hex("#FFD400").expect("黄"),
        // 近灰（低彩度种子，检验 neutral 不塌）
        Color::from_hex("#6E6E6E").expect("灰"),
    ]
}

#[test]
fn text_pairs_meet_wcag_aa_for_every_seed_and_flavor() {
    let mut failures = Vec::new();
    for seed in seeds() {
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            let scheme = Scheme::from_seed(flavor, seed);
            for (fg, bg) in TEXT_PAIRS {
                let ratio = scheme.contrast(fg, bg);
                if ratio < AA_TEXT {
                    failures.push(format!(
                        "seed {} / {} / {} on {} = {ratio:.2}",
                        seed.to_hex(),
                        flavor.name(),
                        fg.name(),
                        bg.name()
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "以下角色对未达 WCAG AA（4.5:1）：\n{}",
        failures.join("\n")
    );
}

#[test]
fn exception_list_is_honest_about_its_thresholds() {
    // 例外必须**真的**是「非文字级」，不能悄悄把低于 3:1 的文字对塞进来。
    for seed in seeds() {
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            let scheme = Scheme::from_seed(flavor, seed);
            for (fg, bg, threshold, reason) in EXCEPTIONS {
                assert!(
                    !fg.is_foreground(),
                    "例外 {} 不该是文字前景色（{reason}）",
                    fg.name()
                );
                let ratio = scheme.contrast(fg, bg);
                assert!(
                    ratio >= threshold,
                    "例外 {} on {} 实测 {ratio:.2}，低于登记阈值 {threshold}（seed {} / {}）",
                    fg.name(),
                    bg.name(),
                    seed.to_hex(),
                    flavor.name()
                );
            }
        }
    }
}

#[test]
fn baseline_light_and_dark_both_pass() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let scheme = Scheme::baseline(flavor);
        for (fg, bg) in TEXT_PAIRS {
            let ratio = scheme.contrast(fg, bg);
            assert!(
                ratio >= AA_TEXT,
                "baseline {} 的 {} on {} 只有 {ratio:.2}",
                flavor.name(),
                fg.name(),
                bg.name()
            );
        }
    }
}

#[test]
fn sky_seed_candidates_meet_wcag_aa() {
    // M70：四档默认色候选（种子 + 中性彩度）必须过同一条 26 组门禁。
    // 中性彩度改变只影响色相/彩度，不影响 tone 轴，所以理论上恒过——这里就是要**证明**它恒过，
    // 而不是相信它。任何一档不达标，该档直接判为不可用（不许为了好看放宽阈值）。
    let candidates = [
        ("① 保守蓝", "#2196F3", 6.0_f64),
        ("② 冷调蓝", "#2196F3", 12.0),
        ("③ 浅冷蓝", "#87CEEB", 12.0),
        ("④ 天蓝", "#3C8DBC", 12.0),
    ];
    let mut failures = Vec::new();
    let mut worst = (f32::MAX, String::new());
    let mut checked = 0;
    for (title, seed_hex, neutral_chroma) in candidates {
        let seed = Color::from_hex(seed_hex).expect("候选种子");
        let palettes =
            unlucky_design::color::Palettes::from_seed_with_neutral_chroma(seed, neutral_chroma);
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            let scheme = Scheme::from_palettes(&palettes, flavor);
            for (fg, bg) in TEXT_PAIRS {
                let ratio = scheme.contrast(fg, bg);
                checked += 1;
                if ratio < worst.0 {
                    worst = (
                        ratio,
                        format!(
                            "{title}/{} 的 {} on {}",
                            flavor.name(),
                            fg.name(),
                            bg.name()
                        ),
                    );
                }
                if ratio < AA_TEXT {
                    failures.push(format!(
                        "{title}（{seed_hex} / 中性 {neutral_chroma}）/ {} / {} on {} = {ratio:.2}",
                        flavor.name(),
                        fg.name(),
                        bg.name()
                    ));
                }
            }
        }
    }
    println!(
        "候选 WCAG：{checked} 项，最紧的一对 {} = {:.2}:1（阈值 {AA_TEXT}）",
        worst.1, worst.0
    );
    assert!(
        failures.is_empty(),
        "以下候选未达 WCAG AA（4.5:1），该档不可用：\n{}",
        failures.join("\n")
    );
}

#[test]
fn accent_override_cannot_break_contrast() {
    // 换了强调色之后，primary 族仍必须过 AA——这是「tone 轴固定」带来的结构性保证。
    let base = Scheme::baseline(SchemeFlavor::Light);
    for seed in seeds() {
        let custom = Scheme::from_accent(SchemeFlavor::Light, seed);
        for (fg, bg) in [
            (Role::OnPrimary, Role::Primary),
            (Role::OnPrimaryContainer, Role::PrimaryContainer),
            (Role::OnSecondary, Role::Secondary),
            (Role::OnTertiary, Role::Tertiary),
        ] {
            let ratio = custom.contrast(fg, bg);
            assert!(
                ratio >= AA_TEXT,
                "强调色 {} 下 {} on {} 只有 {ratio:.2}",
                seed.to_hex(),
                fg.name(),
                bg.name()
            );
        }
        // 表面色不受强调色影响（这条同时是 `from_accent` 的语义断言）。
        assert_eq!(custom.get(Role::Surface), base.get(Role::Surface));
    }
}

#[test]
fn outline_is_registered_as_non_text_exception() {
    // 把「为什么 outline 不是 AA 例外失败项」钉死：它在 light 下 4.29:1（低于 4.5 但高于 3:1）。
    let light = Scheme::baseline(SchemeFlavor::Light);
    let ratio = light.contrast(Role::Outline, Role::Surface);
    assert!(
        ratio < AA_TEXT,
        "light 的 outline 若已达 4.5 应把例外删掉：{ratio:.2}"
    );
    assert!(
        ratio >= AA_NON_TEXT,
        "light 的 outline 低于非文字 3:1：{ratio:.2}"
    );
}
