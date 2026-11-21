//! 弹簧驱动的**一条腿**（M432）：把 design 的 [`Spring`] 接到泵的时间轴上，产出 0~1 的进度。
//!
//! ## 我打算怎么把 `t_ms` → 进度（这一步公式）
//!
//! ```text
//! thr        = 0.5 / 幅度px                     // 「画出来已经是终点」的进度残差（见下）
//! settle     = spring.settle_ms(thr)            // design 的解析解（`spring.rs:70`）
//! total_ms   = settle * scale.factor()          // `MotionScale` 是唯一缩放口（与 `leg_ms` 同一条口径）
//!
//! progress(t) = 0                                       t <= 0
//!             = 1                                       t >= total_ms   （端点**严格** 1.0）
//!             = spring.sample(t / factor)               其余            （`spring.rs:40` 的阶跃响应）
//! ```
//!
//! **为什么阈值取「半个像素」而不是 `0.001`**：这条腿的出口是 `transition::enter_offset` 的 `round()`——
//! 位移 = `round(from * (1 - progress))`，所以 `1 - progress < 0.5 / |from|` 之后每一帧都画成同一个
//! 终点，再跑只是在重复同一帧。按 `0.001` 判（`MotionScale::spring_ms` 的口径）时 12px 的位移要等到
//! 残差 0.012px：expressive 默认弹簧算出来 ≈476ms，**远超** M3E 的 300ms 硬线（`PlanDoc/14` §7-1）。
//! 按像素算，同一根弹簧是 237ms（12px）/ 255ms（16px）——落在短档里，而且终点与「取整后到终点」一致。
//!
//! ## `MotionScale::HALF` 对弹簧意味着什么（定义，不是猜）
//!
//! **压缩时间轴**：`progress(t) = sample(t / factor)`，等价于刚度 `k → k/factor²`（阻尼比 ζ 不变）。
//! 所以回弹峰值不变、稳定时间减半。这条定义不是本模块发明的：`spring.rs:241` 的
//! `MotionScale::spring_ms` 已经是 `settle_ms(...) * factor`，本模块只是把同一个 factor 用在采样上。
//! `NONE`（factor = 0）走不到这里——`Pump::play_page` 在 `is_off()` 时**不插轨道**。
//!
//! ## 回弹（欠阻尼弹簧的过冲）在这个消费点上**看不见**
//!
//! `sample` 在 ζ<1 时会超过 1（expressive 空间默认 ζ=0.8，解析峰值 ≈1.015）。但出场的一步是
//! `enter_offset`：它先把进度 `clamp(0,1)` 再取整 ⇒ 过冲最多 0.13px（12px × 1.1%），**取整后为 0**。
//! 所以逐帧位移仍然单调不增（`tests/spring_leg.rs` 把这条钉住，并把进度层面的回弹段单独列出来）。

use unlucky_design::motion::{MotionScale, Spring};

/// 「半个像素」对应到进度残差：位移取整后与终点**逐位相同**的那条线（见模块文档）。
pub(crate) fn settle_threshold(amplitude_px: f32) -> f32 {
    if amplitude_px > 0.0 {
        (0.5 / amplitude_px).clamp(1.0e-6, 0.5)
    } else {
        // 幅度未知（现有四档位移都非 0，走不到这里）：退回 `spring.rs` 自带的 0.1% 口径。
        0.001
    }
}

/// 一条弹簧腿：弹簧参数 + 已按档位缩放的时间轴。
#[derive(Debug, Clone, Copy)]
pub(crate) struct SpringLeg {
    spring: Spring,
    /// 时间轴缩放系数（`MotionScale::factor`；1.0 = 原速）。
    factor: f32,
    /// 稳定时间（已缩放）：到这里进度**严格**是 1.0。
    total_ms: f64,
}

impl SpringLeg {
    /// 按「弹簧 + 这条腿的位移幅度 + 当前档位」定出一条腿。
    pub(crate) fn new(spring: Spring, amplitude_px: f32, scale: MotionScale) -> Self {
        let factor = scale.factor();
        let settle_ms = f64::from(spring.settle_ms(settle_threshold(amplitude_px)));
        Self {
            spring,
            factor,
            total_ms: settle_ms * f64::from(factor),
        }
    }

    /// 这条腿活多久（泵的到期判据必须问它，不能再用 `enter.duration` ——那会把还在走的弹簧掐掉）。
    pub(crate) fn total_ms(&self) -> f64 {
        self.total_ms
    }

    /// `elapsed_ms` 毫秒之后的进度：`<= 0` 严格 0、`>= total` 严格 1，中间是 `Spring::sample` 原样。
    pub(crate) fn progress(&self, elapsed_ms: f64) -> f32 {
        if elapsed_ms <= 0.0 {
            return 0.0;
        }
        // `factor <= 0`（`MotionScale::NONE`）时 `total_ms` 也是 0 ⇒ 一句话就是「直接落终态」。
        if self.factor <= 0.0 || elapsed_ms >= self.total_ms {
            return 1.0;
        }
        self.spring
            .sample((elapsed_ms / f64::from(self.factor)) as f32)
    }
}
