//! 对比度校验与**收敛**：任意一套方案（不管种子来自图片还是色卡）都要过同一道门。
//!
//! 为什么需要这一层：`tests/contrast_aa.rs` 的 208 项门禁只覆盖**候选色卡**（它拿 6 个手挑的
//! 种子逐对断言）。D38/D40 之后「方案生成」还可能吃图片来的种子，而远程证据不能只靠
//! 「tone 轴固定所以一定达标」这句话——那条性质对本 crate 的生成器成立，但一旦有人手动改角色、
//! 或者将来接别的生成器（比如非 TonalSpot 变体），门禁必须当场发现。所以：
//!
//! 1. [`check`] 对**任何** [`Scheme`] 跑全表（26 对文字/图标对，AA ≥ 4.5:1）；
//! 2. [`converge`] 不达标就**沿 tone 轴修正前景色**（保住色相，只动明度），并把每一处修正
//!    连同「改前/改后的实测对比度」记进 [`Adjustment`]，让 UI 能说「为了可读性微调了强调色」；
//! 3. 修正后仍然不达标的方案，由 [`super::resolve`] 回落到色卡并在 `Provenance` 里登记原因。
//!
//! ## 为什么只改前景、不改背景
//!
//! 背景（`surface` / `*-container`）决定整个界面的骨架与层级，改它会让「同一套方案的两个页面
//! 底色不一样」；前景（`on-*`）本来就是为可读性服务的，动它代价最小。方向也不是猜的：
//! 背景 tone ≥ 50（浅底）就压暗前景，否则提亮前景——修正方向永远朝着「离背景更远」的那一侧。
//!
//! ## 只做 4.5:1 这一档，`outline` 不在门禁里
//!
//! M3 里 `outline` 是非文字元素（输入框描边），规范目标是 3:1，且它按设计就贴着 surface；
//! `outline-variant` 是纯装饰分隔线。这两个的例外登记在 `crates/design/src/color/mod.rs` 的
//! 「对比度纪律」小节，本模块不重复断言（重复断言只会让两边口径打架）。

use crate::color::{Color, Hct, Role, Scheme};

/// 正文/图标对的 AA 门槛（WCAG 2.x：普通文字 4.5:1）。
pub const AA_TEXT: f32 = 4.5;
/// 非文字元素的门槛（描边/图标块 3:1）——本模块不自动收敛这一档，只提供常量给调用方参考。
pub const AA_NON_TEXT: f32 = 3.0;

/// 文字/图标对（前景, 承载色）。与 `tests/contrast_aa.rs` 的 `TEXT_PAIRS` 同表。
pub const TEXT_PAIRS: [(Role, Role); 26] = [
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
    (Role::OnSurface, Role::SurfaceContainerLowest),
    (Role::OnSurface, Role::SurfaceContainerLow),
    (Role::OnSurface, Role::SurfaceContainer),
    (Role::OnSurface, Role::SurfaceContainerHigh),
    (Role::OnSurface, Role::SurfaceContainerHighest),
    (Role::OnSurfaceVariant, Role::SurfaceContainerHighest),
    (Role::OnSurfaceVariant, Role::SurfaceContainerLow),
];

/// 一处不达标。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Failure {
    /// 前景角色。
    pub fg: Role,
    /// 承载色角色。
    pub bg: Role,
    /// 实测对比度。
    pub ratio: f32,
    /// 要求的下限（目前恒为 [`AA_TEXT`]）。
    pub required: f32,
}

/// 一套方案的对比度体检结果。
#[derive(Debug, Clone, PartialEq)]
pub struct WcagReport {
    /// 全部不达标项（空 = 通过）。
    pub failures: Vec<Failure>,
    /// 全表里最低的对比度（用来观察「刚好卡线」的方案）。
    pub worst: f32,
}

impl WcagReport {
    /// 是否全部达标。
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }
}

/// 对任意方案跑全表断言。
pub fn check(scheme: &Scheme) -> WcagReport {
    let mut failures = Vec::new();
    let mut worst = f32::INFINITY;
    for (fg, bg) in TEXT_PAIRS {
        let ratio = scheme.contrast(fg, bg);
        worst = worst.min(ratio);
        if ratio < AA_TEXT {
            failures.push(Failure {
                fg,
                bg,
                ratio,
                required: AA_TEXT,
            });
        }
    }
    WcagReport { failures, worst }
}

/// 一处自动修正的记录（UI 可以拿去写「为可读性微调了强调色」）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adjustment {
    /// 被改的角色。
    pub role: Role,
    /// 为了跟谁达标而改的（承载色角色）。
    pub against: Role,
    /// 改前的色值。
    pub before: Color,
    /// 改后的色值。
    pub after: Color,
    /// 改前的对比度。
    pub before_ratio: f32,
    /// 改后的对比度。
    pub after_ratio: f32,
}

/// 不达标就沿 tone 轴修正，返回（修正后的方案, 修正记录）。
///
/// 最多三遍：一遍修正后可能让「同一个前景对另一个承载色」变得不达标（例如 `on-surface`
/// 同时压在 6 个 surface-container 上），多遍保证收敛。修不动的情况（理论上不存在：纯黑/纯白
/// 对任意背景都 ≥ 4.58:1）会保留原值，由调用方用 [`check`] 复核后决定是否回落。
pub fn converge(scheme: Scheme) -> (Scheme, Vec<Adjustment>) {
    let mut colors = [Color::BLACK; 49];
    for (role, color) in scheme.iter() {
        colors[role as usize] = color;
    }

    let mut adjustments = Vec::new();
    for _ in 0..3 {
        let mut changed = false;
        for (fg, bg) in TEXT_PAIRS {
            let before = colors[fg as usize];
            let background = colors[bg as usize];
            let before_ratio = before.contrast_ratio(background);
            if before_ratio >= AA_TEXT {
                continue;
            }
            let Some(after) = shift_away_from(before, background, AA_TEXT) else {
                continue;
            };
            colors[fg as usize] = after;
            adjustments.push(Adjustment {
                role: fg,
                against: bg,
                before,
                after,
                before_ratio,
                after_ratio: after.contrast_ratio(background),
            });
            changed = true;
        }
        if !changed {
            break;
        }
    }

    (Scheme::from_colors(scheme.flavor(), colors), adjustments)
}

/// 保住色相、压掉彩度地往「离背景更远」的 tone 走，直到达标或到尽头。
fn shift_away_from(foreground: Color, background: Color, min_ratio: f32) -> Option<Color> {
    let hct = Hct::from_color(foreground);
    let darken = background.lstar() >= 50.0;
    let step = if darken { -4.0 } else { 4.0 };
    let mut tone = hct.tone.round();
    for _ in 0..=25 {
        tone += step;
        if !(0.0..=100.0).contains(&tone) {
            break;
        }
        // 修正是「为了可读性」而不是「换个品牌色」：色相不动，彩度让求解器自己压到色域内。
        let candidate = Hct::from_components(hct.hue, hct.chroma, tone).to_color();
        if candidate.contrast_ratio(background) >= min_ratio {
            return Some(candidate);
        }
    }
    // 尽头兜底：纯黑/纯白对任何背景都在 4.58:1 以上（中灰处的理论最低点）。
    let extreme = if darken { Color::BLACK } else { Color::WHITE };
    (extreme.contrast_ratio(background) >= min_ratio).then_some(extreme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::SchemeFlavor;

    #[test]
    fn baseline_is_clean() {
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            let report = check(&Scheme::baseline(flavor));
            assert!(report.is_clean(), "{flavor:?}: {:?}", report.failures);
            assert!(report.worst >= AA_TEXT, "{flavor:?} worst {}", report.worst);
        }
    }

    #[test]
    fn hostile_scheme_is_detected() {
        // 反证：把 on-surface 换成 surface（对比度 1:1），校验器必须报出来。
        let base = Scheme::baseline(SchemeFlavor::Light);
        let mut colors = [Color::BLACK; 49];
        for (role, color) in base.iter() {
            colors[role as usize] = color;
        }
        colors[Role::OnSurface as usize] = base.get(Role::Surface);
        let hostile = Scheme::from_colors(SchemeFlavor::Light, colors);
        let report = check(&hostile);
        assert!(!report.is_clean());
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.fg == Role::OnSurface && failure.bg == Role::Surface));
    }

    #[test]
    fn converge_repairs_a_hostile_scheme() {
        let base = Scheme::baseline(SchemeFlavor::Light);
        let mut colors = [Color::BLACK; 49];
        for (role, color) in base.iter() {
            colors[role as usize] = color;
        }
        colors[Role::OnSurface as usize] = base.get(Role::Surface);
        colors[Role::OnPrimary as usize] = base.get(Role::Primary);
        let hostile = Scheme::from_colors(SchemeFlavor::Light, colors);

        let (fixed, adjustments) = converge(hostile);
        assert!(!adjustments.is_empty(), "应当记录了修正");
        assert!(check(&fixed).is_clean(), "{:?}", check(&fixed).failures);
        for adjustment in &adjustments {
            assert!(adjustment.after_ratio >= AA_TEXT);
            assert!(adjustment.after_ratio > adjustment.before_ratio);
        }
    }

    #[test]
    fn converge_is_a_no_op_on_a_healthy_scheme() {
        let clean = Scheme::baseline(SchemeFlavor::Dark);
        let (fixed, adjustments) = converge(clean);
        assert!(adjustments.is_empty());
        assert_eq!(fixed, clean);
    }

    #[test]
    fn shift_darkens_on_light_background_and_lightens_on_dark() {
        let light_bg = Scheme::baseline(SchemeFlavor::Light);
        let surface = light_bg.get(Role::Surface);
        let fixed = shift_away_from(surface, surface, 4.5).expect("浅底上前景可以压暗");
        assert!(
            fixed.lstar() < surface.lstar() - 20.0,
            "压暗后 tone = {}",
            fixed.lstar()
        );
        assert!(fixed.contrast_ratio(surface) >= 4.5);

        let dark_bg = Scheme::baseline(SchemeFlavor::Dark);
        let surface = dark_bg.get(Role::Surface);
        let fixed = shift_away_from(surface, surface, 4.5).expect("深底上前景可以提亮");
        assert!(
            fixed.lstar() > surface.lstar() + 20.0,
            "提亮后 tone = {}",
            fixed.lstar()
        );
        assert!(fixed.contrast_ratio(surface) >= 4.5);
        // 色相不跟着动：修正只动明度（改前改后的 CAM16 色相差 < 2°）。
        let before = Hct::from_color(surface);
        let after = Hct::from_color(fixed);
        let delta = (before.hue - after.hue)
            .abs()
            .min(360.0 - (before.hue - after.hue).abs());
        assert!(delta < 2.0, "色相被改了 {delta}°");
    }
}
