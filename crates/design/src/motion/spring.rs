//! 弹簧参数（M3 Expressive 动效的物理表达）与动效时长缩放。
//!
//! **来源 A 级**：`androidx.compose.material3.tokens.ExpressiveMotionTokens`（v0_14_0）给出六组
//! `Spring{Damping, Stiffness}`——注意 Compose 的 `Damping` 是**阻尼比 ζ**（不是绝对阻尼系数），
//! 所以本模块的 [`Spring`] 也按 (stiffness, damping_ratio, mass) 存，两者可直接对照。
//! 阶跃响应用标准二阶系统闭式解（欠阻尼 / 临界 / 过阻尼三分支），是纯数学、无状态。

/// 弹簧参数（Compose 口径：刚度 + 阻尼比 + 质量）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// 刚度 k（越大越快）。
    pub stiffness: f32,
    /// 阻尼比 ζ（1.0 = 临界阻尼，< 1 会轻微弹过）。
    pub damping_ratio: f32,
    /// 质量 m。
    pub mass: f32,
}

impl Spring {
    /// 按 M3 令牌口径构造（刚度 + 阻尼比 + 质量）。
    pub const fn from_ratio(stiffness: f32, damping_ratio: f32, mass: f32) -> Self {
        Self {
            stiffness,
            damping_ratio,
            mass,
        }
    }

    /// 自然角频率 ωn（rad/s）。
    pub fn natural_frequency(self) -> f32 {
        (self.stiffness / self.mass).sqrt()
    }

    /// 绝对阻尼系数 c = 2·ζ·√(k·m)（需要喂给「阻尼系数」式积分器时用）。
    pub fn damping_coefficient(self) -> f32 {
        2.0 * self.damping_ratio * (self.stiffness * self.mass).sqrt()
    }

    /// 阶跃响应：`t_ms` 毫秒后的进度（0..1，欠阻尼时会轻微超过 1 = 回弹）。
    pub fn sample(self, t_ms: f32) -> f32 {
        if t_ms <= 0.0 {
            return 0.0;
        }
        let t = t_ms / 1000.0;
        let wn = self.natural_frequency();
        let zeta = self.damping_ratio;
        if wn <= 0.0 {
            return 1.0;
        }
        if zeta < 1.0 {
            let wd = wn * (1.0 - zeta * zeta).sqrt();
            let decay = (-zeta * wn * t).exp();
            1.0 - decay * ((wd * t).cos() + (zeta * wn / wd) * (wd * t).sin())
        } else if (zeta - 1.0).abs() < 1.0e-4 {
            let decay = (-wn * t).exp();
            1.0 - decay * (1.0 + wn * t)
        } else {
            let root = (zeta * zeta - 1.0).sqrt();
            let r1 = -wn * (zeta - root);
            let r2 = -wn * (zeta + root);
            let denom = r2 - r1;
            if denom.abs() < f32::EPSILON {
                return 1.0;
            }
            1.0 - (r2 * (r1 * t).exp() - r1 * (r2 * t).exp()) / denom
        }
    }

    /// 收敛到 `threshold`（0.1% 是常用的「停下来」判定）所需毫秒数。
    pub fn settle_ms(self, threshold: f32) -> f32 {
        let wn = self.natural_frequency();
        let zeta = self.damping_ratio;
        if wn <= 0.0 {
            return 0.0;
        }
        let thr = threshold.clamp(1.0e-6, 0.5);
        let seconds = if zeta < 1.0 {
            let wd = (1.0 - zeta * zeta).sqrt().max(1.0e-4);
            -(thr * wd).ln() / (zeta * wn)
        } else if (zeta - 1.0).abs() < 1.0e-4 {
            -(thr).ln() / wn
        } else {
            let root = (zeta * zeta - 1.0).sqrt();
            let slow = wn * (zeta - root);
            -(thr).ln() / slow
        };
        (seconds * 1000.0).max(0.0)
    }
}

/// 动效方案（Expressive 把弹簧打包成 standard / expressive **两套**，不是一组）。
///
/// - [`MotionSchemeKind::Standard`]：保守、几乎不弹（spatial ζ=0.9）；
/// - [`MotionSchemeKind::Expressive`]：D26 的视觉基线，回弹明显（spatial ζ=0.6~0.8）。
///
/// `effects`（透明度/颜色类）在两套方案里数值相同——效应类不该有可见回弹。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotionSchemeKind {
    /// 标准方案（保守）。
    Standard,
    /// Expressive 方案（本项目默认）。
    Expressive,
}

/// 弹簧类别：空间类管**位移/尺寸**，效应类管**颜色/透明度**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpringCategory {
    /// 位移/尺寸（会产生可见回弹）。
    Spatial,
    /// 颜色/透明度（不允许过冲）。
    Effects,
}

/// 弹簧速度档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpringSpeed {
    /// 快速：hover/press、小元素。
    Fast,
    /// 默认：面板、卡片、按下形变。
    Default,
    /// 慢速：大位移（>120px 的面板/抽屉）。
    Slow,
}

/// 二维表查询：`方案 × 类别 × 速度 → 弹簧`（A 级：`StandardMotionTokens` 与
/// `ExpressiveMotionTokens`，经 `MotionScheme.standard()/expressive()` 组装）。
pub const fn spring(
    scheme: MotionSchemeKind,
    category: SpringCategory,
    speed: SpringSpeed,
) -> Spring {
    use MotionSchemeKind::{Expressive, Standard};
    use SpringCategory::{Effects, Spatial};
    use SpringSpeed::{Default, Fast, Slow};
    match (scheme, category, speed) {
        (Standard, Spatial, Fast) => Spring::from_ratio(1400.0, 0.9, 1.0),
        (Standard, Spatial, Default) => Spring::from_ratio(700.0, 0.9, 1.0),
        (Standard, Spatial, Slow) => Spring::from_ratio(300.0, 0.9, 1.0),
        (Expressive, Spatial, Fast) => Spring::from_ratio(800.0, 0.6, 1.0),
        (Expressive, Spatial, Default) => Spring::from_ratio(380.0, 0.8, 1.0),
        (Expressive, Spatial, Slow) => Spring::from_ratio(200.0, 0.8, 1.0),
        // effects 两套同值：ζ=1.0（临界阻尼，不过冲）。
        (_, Effects, Fast) => Spring::from_ratio(3800.0, 1.0, 1.0),
        (_, Effects, Default) => Spring::from_ratio(1600.0, 1.0, 1.0),
        (_, Effects, Slow) => Spring::from_ratio(800.0, 1.0, 1.0),
    }
}

/// 标准方案 · 空间 · 快速。
pub const SPRING_STANDARD_SPATIAL_FAST: Spring = spring(
    MotionSchemeKind::Standard,
    SpringCategory::Spatial,
    SpringSpeed::Fast,
);
/// 标准方案 · 空间 · 默认。
pub const SPRING_STANDARD_SPATIAL_DEFAULT: Spring = spring(
    MotionSchemeKind::Standard,
    SpringCategory::Spatial,
    SpringSpeed::Default,
);
/// 标准方案 · 空间 · 慢速。
pub const SPRING_STANDARD_SPATIAL_SLOW: Spring = spring(
    MotionSchemeKind::Standard,
    SpringCategory::Spatial,
    SpringSpeed::Slow,
);
/// Expressive 方案 · 空间 · 快速（D26 基线的默认选择）。
pub const SPRING_EXPRESSIVE_SPATIAL_FAST: Spring = spring(
    MotionSchemeKind::Expressive,
    SpringCategory::Spatial,
    SpringSpeed::Fast,
);
/// Expressive 方案 · 空间 · 默认：面板、卡片、按下形变。
pub const SPRING_EXPRESSIVE_SPATIAL_DEFAULT: Spring = spring(
    MotionSchemeKind::Expressive,
    SpringCategory::Spatial,
    SpringSpeed::Default,
);
/// Expressive 方案 · 空间 · 慢速：大位移（>120px）。
pub const SPRING_EXPRESSIVE_SPATIAL_SLOW: Spring = spring(
    MotionSchemeKind::Expressive,
    SpringCategory::Spatial,
    SpringSpeed::Slow,
);
/// 效应类 · 快速（两套方案同值）：hover/press 颜色。
pub const SPRING_EFFECTS_FAST: Spring = spring(
    MotionSchemeKind::Expressive,
    SpringCategory::Effects,
    SpringSpeed::Fast,
);
/// 效应类 · 默认（两套方案同值）。
pub const SPRING_EFFECTS_DEFAULT: Spring = spring(
    MotionSchemeKind::Expressive,
    SpringCategory::Effects,
    SpringSpeed::Default,
);
/// 效应类 · 慢速（两套方案同值）。
pub const SPRING_EFFECTS_SLOW: Spring = spring(
    MotionSchemeKind::Expressive,
    SpringCategory::Effects,
    SpringSpeed::Slow,
);

/// 动效时长的全局缩放（用户「减少动效」/CPU 路径关动画时用）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionScale {
    factor: f32,
}

impl MotionScale {
    /// 标准（1.0）。
    pub const STANDARD: MotionScale = MotionScale { factor: 1.0 };
    /// 关闭动效（时长归零；适配层应直接落终态）。
    pub const NONE: MotionScale = MotionScale { factor: 0.0 };
    /// 半速（弱动效机器/低帧率兜底）。
    pub const HALF: MotionScale = MotionScale { factor: 0.5 };

    /// 自定义缩放（夹取到 0.0..=1.0；不放大，避免慢到不可用）。
    pub fn new(factor: f32) -> Self {
        Self {
            factor: factor.clamp(0.0, 1.0),
        }
    }

    /// 缩放系数。
    pub fn factor(self) -> f32 {
        self.factor
    }

    /// 是否完全关闭（适配层据此跳过动画）。
    pub fn is_off(self) -> bool {
        self.factor <= 0.0
    }

    /// 把某一档时长换算成实际毫秒。
    pub fn ms(self, token: super::DurationToken) -> f32 {
        token.ms() as f32 * self.factor
    }

    /// 把弹簧的收敛时间换算成实际毫秒。
    pub fn spring_ms(self, spring: Spring) -> f32 {
        spring.settle_ms(0.001) * self.factor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::DurationToken;

    const PRESETS: [Spring; 9] = [
        SPRING_STANDARD_SPATIAL_FAST,
        SPRING_STANDARD_SPATIAL_DEFAULT,
        SPRING_STANDARD_SPATIAL_SLOW,
        SPRING_EXPRESSIVE_SPATIAL_FAST,
        SPRING_EXPRESSIVE_SPATIAL_DEFAULT,
        SPRING_EXPRESSIVE_SPATIAL_SLOW,
        SPRING_EFFECTS_FAST,
        SPRING_EFFECTS_DEFAULT,
        SPRING_EFFECTS_SLOW,
    ];

    #[test]
    fn every_cell_of_the_two_dimensional_table_resolves() {
        for scheme in [MotionSchemeKind::Standard, MotionSchemeKind::Expressive] {
            for category in [SpringCategory::Spatial, SpringCategory::Effects] {
                for speed in [SpringSpeed::Fast, SpringSpeed::Default, SpringSpeed::Slow] {
                    let spring = spring(scheme, category, speed);
                    assert!(spring.stiffness > 0.0);
                    assert!(spring.damping_ratio > 0.0);
                }
            }
        }
        // effects 两套方案同值（效应类不该有可见回弹差异）。
        for speed in [SpringSpeed::Fast, SpringSpeed::Default, SpringSpeed::Slow] {
            assert_eq!(
                spring(MotionSchemeKind::Standard, SpringCategory::Effects, speed),
                spring(MotionSchemeKind::Expressive, SpringCategory::Effects, speed)
            );
        }
        // expressive 的空间类更弹（阻尼比更低）。
        for speed in [SpringSpeed::Fast, SpringSpeed::Default, SpringSpeed::Slow] {
            assert!(
                spring(MotionSchemeKind::Expressive, SpringCategory::Spatial, speed).damping_ratio
                    < spring(MotionSchemeKind::Standard, SpringCategory::Spatial, speed)
                        .damping_ratio
            );
        }
    }

    #[test]
    fn spring_presets_behave() {
        for spring in PRESETS {
            assert_eq!(spring.sample(0.0), 0.0);
            let settle = spring.settle_ms(0.001);
            assert!(
                (60.0..1500.0).contains(&settle),
                "收敛时间 {settle}ms 不合理（k={} ζ={}）",
                spring.stiffness,
                spring.damping_ratio
            );
            let end = spring.sample(settle);
            assert!((end - 1.0).abs() < 0.01, "收敛后应回到 1.0，实测 {end}");
        }
    }

    #[test]
    fn effects_presets_are_critically_damped_and_never_overshoot() {
        for spring in [
            SPRING_EFFECTS_FAST,
            SPRING_EFFECTS_DEFAULT,
            SPRING_EFFECTS_SLOW,
        ] {
            assert!((spring.damping_ratio - 1.0).abs() < 1.0e-6);
            for step in 1..=60 {
                let v = spring.sample(step as f32 * 5.0);
                assert!(v <= 1.0 + 1.0e-3, "效应类弹簧不应过冲：{v}");
            }
        }
    }

    #[test]
    fn spatial_presets_overdamp_into_settle_without_wild_overshoot() {
        for spring in [
            SPRING_EXPRESSIVE_SPATIAL_FAST,
            SPRING_EXPRESSIVE_SPATIAL_DEFAULT,
            SPRING_EXPRESSIVE_SPATIAL_SLOW,
        ] {
            assert!(
                spring.damping_ratio < 1.0,
                "空间类弹簧欠阻尼（允许轻微过冲）"
            );
            let mut peak: f32 = 0.0;
            for step in 1..=200 {
                peak = peak.max(spring.sample(step as f32 * 2.0));
            }
            assert!(peak < 1.15, "过冲 {peak} 太大");
        }
    }

    #[test]
    fn critically_damped_spring_does_not_overshoot() {
        let wn = SPRING_EXPRESSIVE_SPATIAL_DEFAULT.natural_frequency();
        let critical = Spring::from_ratio(wn * wn, 1.0, 1.0);
        for step in 1..=40 {
            let v = critical.sample(step as f32 * 5.0);
            assert!(v <= 1.0 + 1.0e-3, "临界阻尼不应过冲：{v}");
        }
    }

    #[test]
    fn overdamped_spring_converges_slowly_without_overshoot() {
        let wn = SPRING_EXPRESSIVE_SPATIAL_DEFAULT.natural_frequency();
        let over = Spring::from_ratio(wn * wn, 2.0, 1.0);
        let settle = over.settle_ms(0.001);
        assert!((over.sample(settle) - 1.0).abs() < 0.01);
        for step in 1..=40 {
            assert!(over.sample(step as f32 * 10.0) <= 1.0 + 1.0e-3);
        }
    }

    #[test]
    fn damping_coefficient_matches_ratio() {
        let spring = SPRING_EXPRESSIVE_SPATIAL_DEFAULT;
        let c = spring.damping_coefficient();
        let zeta = c / (2.0 * (spring.stiffness * spring.mass).sqrt());
        assert!((zeta - spring.damping_ratio).abs() < 1.0e-5);
    }

    #[test]
    fn motion_scale_scales_or_kills_duration() {
        assert_eq!(MotionScale::STANDARD.ms(DurationToken::Medium2), 300.0);
        assert_eq!(MotionScale::HALF.ms(DurationToken::Medium2), 150.0);
        assert_eq!(MotionScale::NONE.ms(DurationToken::Medium2), 0.0);
        assert!(MotionScale::NONE.is_off());
        assert_eq!(MotionScale::new(5.0).factor(), 1.0);
        assert_eq!(MotionScale::new(-1.0).factor(), 0.0);
        assert!(MotionScale::STANDARD.spring_ms(SPRING_EXPRESSIVE_SPATIAL_DEFAULT) > 0.0);
    }
}
