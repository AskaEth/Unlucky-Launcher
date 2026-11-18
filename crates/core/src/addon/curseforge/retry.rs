//! 限流重试的**上界**与**开关**。
//!
//! # 为什么重试归本模块、不归下载引擎
//!
//! `core::download` 的引擎（[`super::EngineTransport`] 之外那条路）自带退避，但它**不支持请求头**，
//! 而 CurseForge 的每一次请求都要带 `x-api-key`。所以本模块直发一次请求（不做任何决策），
//! 重试策略留在这里：能读站点的 `Retry-After`、能有上界、能被关掉。
//!
//! # 为什么默认是 3 次
//!
//! 429 的 `Retry-After` 常常是 1~5 秒（CF 的官方配额是每分钟若干次）。3 次 40 秒上限的等待
//! 已经覆盖「一次误触配额」，再多的等待应该交给用户点重试——搜索界面上转 60 秒的 spinner 比
//! 一句「稍后再试」更糟。
//!
//! # 关闭就是 `attempts = 1`
//!
//! 不引入 `enabled: bool`：那会造出「关掉了但 attempts=5」这种自相矛盾的状态。`attempts` 是
//! 唯一事实——**1 次就是只发一次**（[`RetryPolicy::disabled`]）。

use std::time::Duration;

/// 出厂的重试次数（含首次）。
pub const DEFAULT_ATTEMPTS: u32 = 3;

/// 出厂的指数退避起点（1s、2s、4s…）。
pub const DEFAULT_BASE_DELAY: Duration = Duration::from_secs(1);

/// 单次等待的上限（站点给了更大的 `Retry-After` 也只等这么久）。
pub const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(30);

/// 重试策略（有界、可关）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    attempts: u32,
    base_delay: Duration,
    max_delay: Duration,
}

impl RetryPolicy {
    /// 出厂策略：3 次，1s 起指数退避，单次封顶 30s。
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            attempts: DEFAULT_ATTEMPTS,
            base_delay: DEFAULT_BASE_DELAY,
            max_delay: DEFAULT_MAX_DELAY,
        }
    }

    /// 关掉重试：只发一次（429 也直接如实返回，由上层提示用户）。
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            attempts: 1,
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        }
    }

    /// 改成别的次数（`0` 会被钳成 1：**没有「发零次」这种策略**）。
    #[must_use]
    pub const fn with_attempts(mut self, attempts: u32) -> Self {
        self.attempts = if attempts == 0 { 1 } else { attempts };
        self
    }

    /// 改成别的退避参数（测试用来把等待压成 0，生产用来跟随用户设置）。
    #[must_use]
    pub const fn with_delays(mut self, base: Duration, max: Duration) -> Self {
        self.base_delay = base;
        self.max_delay = max;
        self
    }

    /// 最多发几次（含首次）。
    #[must_use]
    pub const fn attempts(&self) -> u32 {
        self.attempts
    }

    /// 会重试吗（`false` = 只发一次）。
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.attempts > 1
    }

    /// 第 `attempt` 次失败（0 起）之后等多久。
    ///
    /// 站点给了 `Retry-After` 就**听站点的**（封顶 `max_delay`）：它比我们的指数退避更有依据。
    /// 封顶是为了不被一个写错的 `Retry-After: 86400` 卡死一整天。
    #[must_use]
    pub fn delay(&self, attempt: u32, retry_after: Option<u64>) -> Duration {
        if let Some(seconds) = retry_after {
            return Duration::from_secs(seconds).min(self.max_delay);
        }
        // 左移不能溢出：先按 min(attempt, 16) 钳住（2^16 秒已远超 max_delay）。
        let factor = 1u32 << attempt.min(16);
        self.base_delay.saturating_mul(factor).min(self.max_delay)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::standard()
    }
}
