//! 指数退避 + 抖动。
//!
//! 修 HMCL 的坑（analyse/03 §7.6 第 5 点）：它的重试循环里没有 sleep，5xx 会变成
//! 「6 次瞬时连打」，把弱网镜像直接压垮。这里给出 1s / 2s / 4s… 上限 30s，
//! ±30% 抖动打散多实例/多文件的同步重试风暴。
//!
//! 抖动源是自带的 xorshift64*，不引 `rand`（体积 + Win7 依赖面）。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// 退避策略（可序列化进设置）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BackoffPolicy {
    /// 首次退避（毫秒）。
    pub base_ms: u64,
    /// 单次退避上限（毫秒）。
    pub max_ms: u64,
    /// 抖动比例（0.0~1.0，0.3 = ±30%）。
    pub jitter_ratio: f64,
    /// 每个源的尝试次数上限。
    pub max_attempts: u32,
}

impl BackoffPolicy {
    /// 不重试（一次失败立刻换源）。
    #[must_use]
    pub const fn none() -> Self {
        Self {
            base_ms: 0,
            max_ms: 0,
            jitter_ratio: 0.0,
            max_attempts: 1,
        }
    }

    /// 第 `attempt` 次重试前的**无抖动**等待时间（`attempt` 从 0 起）。
    #[must_use]
    pub fn delay(&self, attempt: u32) -> Duration {
        let factor = 1u64.checked_shl(attempt.min(32)).unwrap_or(u64::MAX).max(1);
        Duration::from_millis(self.base_ms.saturating_mul(factor).min(self.max_ms))
    }

    /// 第 `attempt` 次重试前的等待时间，`rng` 为 `[0,1)` 的抖动量（可复现，便于单测）。
    ///
    /// 结果永不超过 `max_ms`，且永不低于 `base_ms`（当 `base_ms <= max_ms` 时）。
    #[must_use]
    pub fn delay_jittered(&self, attempt: u32, rng: f64) -> Duration {
        let base_ms = self.delay(attempt).as_millis() as f64;
        let ratio = self.jitter_ratio.clamp(0.0, 1.0);
        let factor = 1.0 + ratio * (2.0 * rng.clamp(0.0, 1.0) - 1.0);
        let ceiling = (self.max_ms as f64).max(base_ms);
        let jittered_ms = (base_ms * factor).clamp(0.0, ceiling);
        Duration::from_millis(jittered_ms as u64)
    }

    /// 用抖动源取一次等待时间。
    #[must_use]
    pub fn next_delay(&self, attempt: u32, jitter: &mut Jitter) -> Duration {
        self.delay_jittered(attempt, jitter.next_fraction())
    }
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            base_ms: 1_000,
            max_ms: 30_000,
            jitter_ratio: 0.3,
            max_attempts: 3,
        }
    }
}

/// xorshift64* 抖动源：确定性、无外部依赖。
#[derive(Debug, Clone)]
pub struct Jitter {
    state: u64,
}

impl Jitter {
    /// 固定种子构造（同种子的序列可复现 → 单测友好）。
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    /// 下一个 64 位随机数。
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// 下一个 `[0,1)` 的浮点。
    pub fn next_fraction(&mut self) -> f64 {
        // 取高 53 位映射到 [0,1)，与 f64 的尾数精度一致。
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

impl Default for Jitter {
    fn default() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x1234_5678_9ABC_DEF0);
        Self::new(seed)
    }
}
