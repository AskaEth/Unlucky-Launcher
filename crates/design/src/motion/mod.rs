//! 动效令牌：时长、缓动（可采样）、M3 Expressive 弹簧参数。
//!
//! 三件东西分开表达，因为适配层要的接口不同：
//! - [`DurationToken`]：M3 的 short/medium/long/extra-long 十六档，给「固定时长 + easing」用；
//! - [`Easing`]：cubic-bezier 曲线，纯函数采样（适配层自己决定帧率）；
//! - [`Spring`]（见 [`spring`]）：Expressive 的物理参数（stiffness/damping/mass），
//!   可用于实时求解或预估时长。
//!
//! **CPU / 减少动效路径**：把 [`MotionScale`] 设成 [`MotionScale::NONE`]，
//! 所有时长归零、`Easing::sample` 仍返回 0/1 端值（适配层按 `t >= 1` 直接落终态）。
//!
//! ## [`MotionScale::HALF`] 是什么（读实现的结论，不是注释的措辞）
//!
//! `MotionScale` 只有一个 `factor`，`ms(token) = token.ms() * factor`（见 `spring` 模块的
//! `motion_scale_scales_or_kills_duration`），所以 `HALF` 的语义是**时长减半**（同一段位移在
//! 一半的时间里走完 ⇒ 观感上**更快**，不是"半速"）。`spring_ms` 同理：收敛时间减半。
//!
//! `spring.rs` 里 `HALF` 的行内注释写作「半速」，那是**措辞错误**（MOTION1 核清）：本行文档是
//! 事实口径，改 `spring.rs` 的注释不在 M256 的 scope 内，已作为 finding 报给 tower。

mod spring;

pub use spring::{
    spring, MotionScale, MotionSchemeKind, Spring, SpringCategory, SpringSpeed,
    SPRING_EFFECTS_DEFAULT, SPRING_EFFECTS_FAST, SPRING_EFFECTS_SLOW,
    SPRING_EXPRESSIVE_SPATIAL_DEFAULT, SPRING_EXPRESSIVE_SPATIAL_FAST,
    SPRING_EXPRESSIVE_SPATIAL_SLOW, SPRING_STANDARD_SPATIAL_DEFAULT, SPRING_STANDARD_SPATIAL_FAST,
    SPRING_STANDARD_SPATIAL_SLOW,
};

/// 模态（对话框 / 危险确认）**进场**的起始缩放：`0.96 → 1.0`。
///
/// 出处：`DevDocs/PlanDoc/14-motion-and-animation.md` §3 的 A3 行——「遮罩淡入 + 卡片
/// scale 0.96→1（**不弹跳**）」。**退场走同一条线反向**（1.0 → 0.96），所以只需要这一个常量。
///
/// 为什么不是 0.8 或 0.9：0.9 以下的缩放会让卡片里的正文在动画期间小到读不清（对话框是
/// 「必须读」的界面，不是装饰）；0.96 是 M3 里 dialog 的形变幅度口径，也让「它在放大」这件事
/// 恰好可察觉而不夸张。**不弹跳**：曲线只取单调段（`*Decelerate` / `*Accelerate`），
/// 弹簧类曲线不用于模态。
pub const DIALOG_ENTER_SCALE: f32 = 0.96;

/// 动效时长档位（M3 动效令牌，单位毫秒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurationToken {
    /// 50ms：最小状态回弹。
    Short1,
    /// 100ms：小图标/勾选。
    Short2,
    /// 150ms：开关、小尺寸 fade。
    Short3,
    /// 200ms：按钮状态、hover。
    Short4,
    /// 250ms：小卡片展开。
    Medium1,
    /// 300ms：导航指示条、tab 切换。
    Medium2,
    /// 350ms：卡片展开、列表重排。
    Medium3,
    /// 400ms：面板开合。
    Medium4,
    /// 450ms：页面切换。
    Long1,
    /// 500ms：大面板进出。
    Long2,
    /// 550ms：全屏浮层。
    Long3,
    /// 600ms：对话框进出。
    Long4,
    /// 700ms：结构化过渡。
    ExtraLong1,
    /// 800ms：结构化过渡。
    ExtraLong2,
    /// 900ms：结构化过渡。
    ExtraLong3,
    /// 1000ms：整屏形变（shape morph 的慢档）。
    ExtraLong4,
}

impl DurationToken {
    /// 毫秒值。
    pub const fn ms(self) -> u32 {
        match self {
            DurationToken::Short1 => 50,
            DurationToken::Short2 => 100,
            DurationToken::Short3 => 150,
            DurationToken::Short4 => 200,
            DurationToken::Medium1 => 250,
            DurationToken::Medium2 => 300,
            DurationToken::Medium3 => 350,
            DurationToken::Medium4 => 400,
            DurationToken::Long1 => 450,
            DurationToken::Long2 => 500,
            DurationToken::Long3 => 550,
            DurationToken::Long4 => 600,
            DurationToken::ExtraLong1 => 700,
            DurationToken::ExtraLong2 => 800,
            DurationToken::ExtraLong3 => 900,
            DurationToken::ExtraLong4 => 1000,
        }
    }

    /// 稳定字符串名（`short1` … `extra-long4`），写配置与日志用。
    pub const fn name(self) -> &'static str {
        match self {
            DurationToken::Short1 => "short1",
            DurationToken::Short2 => "short2",
            DurationToken::Short3 => "short3",
            DurationToken::Short4 => "short4",
            DurationToken::Medium1 => "medium1",
            DurationToken::Medium2 => "medium2",
            DurationToken::Medium3 => "medium3",
            DurationToken::Medium4 => "medium4",
            DurationToken::Long1 => "long1",
            DurationToken::Long2 => "long2",
            DurationToken::Long3 => "long3",
            DurationToken::Long4 => "long4",
            DurationToken::ExtraLong1 => "extra-long1",
            DurationToken::ExtraLong2 => "extra-long2",
            DurationToken::ExtraLong3 => "extra-long3",
            DurationToken::ExtraLong4 => "extra-long4",
        }
    }
}

/// 三次贝塞尔缓动曲线（CSS `cubic-bezier` 同样的参数含义）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    /// 控制点 1 的 x（0..1）。
    pub x1: f32,
    /// 控制点 1 的 y。
    pub y1: f32,
    /// 控制点 2 的 x（0..1）。
    pub x2: f32,
    /// 控制点 2 的 y。
    pub y2: f32,
}

impl CubicBezier {
    /// 线性。
    pub const LINEAR: Self = Self {
        x1: 0.0,
        y1: 0.0,
        x2: 1.0,
        y2: 1.0,
    };

    /// 给定时间参数 `t`（0..1），返回进度（0..1，标准曲线不越界）。
    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if t == 0.0 || t == 1.0 {
            return t;
        }
        // 二分求 x(s) = t 的曲线参数 s，再取 y(s)。
        let mut lo = 0.0_f32;
        let mut hi = 1.0_f32;
        let mut s = t;
        for _ in 0..24 {
            s = 0.5 * (lo + hi);
            let x = bezier_axis(self.x1, self.x2, s);
            if x < t {
                lo = s;
            } else {
                hi = s;
            }
        }
        bezier_axis(self.y1, self.y2, s)
    }
}

/// 一维三次贝塞尔：端点固定为 0/1，控制点为 `c1`/`c2`。
fn bezier_axis(c1: f32, c2: f32, s: f32) -> f32 {
    let u = 1.0 - s;
    3.0 * u * u * s * c1 + 3.0 * u * s * s * c2 + s * s * s
}

/// M3 缓动档位（值取自 Material 动效令牌）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Easing {
    /// 线性（进度指示器）。
    Linear,
    /// 标准：起止都平滑。
    Standard,
    /// 标准减速：进场（出现在屏幕上）。
    StandardDecelerate,
    /// 标准加速：退场。
    StandardAccelerate,
    /// 强调：注意力转移的主曲线。
    Emphasized,
    /// 强调减速：强调类进场。
    EmphasizedDecelerate,
    /// 强调加速：强调类退场。
    EmphasizedAccelerate,
}

impl Easing {
    /// 曲线参数。
    pub const fn curve(self) -> CubicBezier {
        match self {
            Easing::Linear => CubicBezier::LINEAR,
            Easing::Standard | Easing::Emphasized => CubicBezier {
                x1: 0.2,
                y1: 0.0,
                x2: 0.0,
                y2: 1.0,
            },
            Easing::StandardDecelerate => CubicBezier {
                x1: 0.0,
                y1: 0.0,
                x2: 0.0,
                y2: 1.0,
            },
            Easing::StandardAccelerate => CubicBezier {
                x1: 0.3,
                y1: 0.0,
                x2: 1.0,
                y2: 1.0,
            },
            Easing::EmphasizedDecelerate => CubicBezier {
                x1: 0.05,
                y1: 0.7,
                x2: 0.1,
                y2: 1.0,
            },
            Easing::EmphasizedAccelerate => CubicBezier {
                x1: 0.3,
                y1: 0.0,
                x2: 0.8,
                y2: 0.15,
            },
        }
    }

    /// 采样：`t` 0..1 → 进度 0..1。
    pub fn sample(self, t: f32) -> f32 {
        self.curve().sample(t)
    }

    /// 该曲线搭配的常用时长（我们的组件映射口径，非规范值）。
    pub const fn default_duration(self) -> DurationToken {
        match self {
            Easing::Linear => DurationToken::Long2,
            _ => DurationToken::Medium2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_EASINGS: [Easing; 7] = [
        Easing::Linear,
        Easing::Standard,
        Easing::StandardDecelerate,
        Easing::StandardAccelerate,
        Easing::Emphasized,
        Easing::EmphasizedDecelerate,
        Easing::EmphasizedAccelerate,
    ];

    #[test]
    fn durations_are_monotone() {
        let ladder = [
            DurationToken::Short1,
            DurationToken::Short2,
            DurationToken::Short3,
            DurationToken::Short4,
            DurationToken::Medium1,
            DurationToken::Medium2,
            DurationToken::Medium3,
            DurationToken::Medium4,
            DurationToken::Long1,
            DurationToken::Long2,
            DurationToken::Long3,
            DurationToken::Long4,
            DurationToken::ExtraLong1,
            DurationToken::ExtraLong2,
            DurationToken::ExtraLong3,
            DurationToken::ExtraLong4,
        ];
        assert_eq!(ladder.len(), 16);
        for pair in ladder.windows(2) {
            assert!(pair[0].ms() < pair[1].ms());
        }
        assert_eq!(DurationToken::Short1.ms(), 50);
        assert_eq!(DurationToken::ExtraLong4.ms(), 1000);
        // 名字唯一
        let mut names = std::collections::BTreeSet::new();
        for token in ladder {
            assert!(names.insert(token.name()), "时长名重复：{}", token.name());
        }
    }

    #[test]
    fn easing_hits_endpoints_and_stays_in_range() {
        for easing in ALL_EASINGS {
            assert!(
                (easing.sample(0.0) - 0.0).abs() < 1.0e-6,
                "{easing:?} 起点不为 0"
            );
            assert!(
                (easing.sample(1.0) - 1.0).abs() < 1.0e-6,
                "{easing:?} 终点不为 1"
            );
            let mut prev = 0.0;
            for step in 1..=20 {
                let v = easing.sample(step as f32 / 20.0);
                assert!((-0.001..=1.001).contains(&v), "{easing:?} 采样越界：{v}");
                assert!(v + 1.0e-3 >= prev, "{easing:?} 采样非单调：{prev} → {v}");
                prev = v;
            }
        }
    }

    #[test]
    fn emphasized_decelerate_is_faster_early_than_accelerate() {
        let early_decel = Easing::EmphasizedDecelerate.sample(0.25);
        let early_accel = Easing::EmphasizedAccelerate.sample(0.25);
        assert!(early_decel > early_accel);
    }

    #[test]
    fn default_durations_are_registered_tokens() {
        for easing in ALL_EASINGS {
            assert!(easing.default_duration().ms() >= 250);
        }
    }
}
