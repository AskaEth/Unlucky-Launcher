//! 时间源：审计日志、事件流、工具耗时都要时间戳，但**测试不能依赖真实时钟**
//! （否则断言不稳定）。所以照既有做法注入 [`Clock`]。
//!
//! 时间戳格式化自己实现（[`format_unix_ms`]），不引 `chrono`：agent 的依赖面按
//! `DevDocs/PlanDoc/02-ai-agent-design.md` §0 越窄越好，RFC3339 UTC 只需要一个
//! 民用历换算（Howard Hinnant 的 `civil_from_days`）。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// 时间源。生产用 [`SystemClock`]，测试用 [`FixedClock`]。
pub trait Clock: Send + Sync {
    /// 当前 Unix 毫秒时间戳。
    fn now_unix_ms(&self) -> u64;
}

/// 真实系统时钟（单调性不保证，审计只要"大致有序"）。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64)
    }
}

/// 可控时钟：测试里固定起点、必要时手动推进。
#[derive(Debug)]
pub struct FixedClock {
    ms: AtomicU64,
    step_ms: u64,
}

impl FixedClock {
    /// 从固定时间戳开始，每次读走一步 `step_ms`（0 = 不自动推进）。
    #[must_use]
    pub fn new(start_ms: u64) -> Self {
        Self {
            ms: AtomicU64::new(start_ms),
            step_ms: 0,
        }
    }

    /// 每次读取自动前进 `step_ms`，用来断言工具耗时字段。
    #[must_use]
    pub fn with_step(start_ms: u64, step_ms: u64) -> Self {
        Self {
            ms: AtomicU64::new(start_ms),
            step_ms,
        }
    }

    /// 手动推进。
    pub fn advance(&self, ms: u64) {
        self.ms.fetch_add(ms, Ordering::Relaxed);
    }
}

impl Clock for FixedClock {
    fn now_unix_ms(&self) -> u64 {
        if self.step_ms == 0 {
            return self.ms.load(Ordering::Relaxed);
        }
        self.ms.fetch_add(self.step_ms, Ordering::Relaxed) + self.step_ms
    }
}

/// `Arc<dyn Clock>` 的简写。
pub type SharedClock = Arc<dyn Clock>;

/// Unix 毫秒 → `YYYY-MM-DDTHH:MM:SS.mmmZ`（UTC）。
#[must_use]
pub fn format_unix_ms(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let millis = ms % 1000;
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let (h, mi, s) = (sod / 3600, (sod % 3600) / 60, sod % 60);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{millis:03}Z")
}

/// Unix 毫秒 → `YYYY-MM-DD`（UTC），审计落盘按天分文件用。
#[must_use]
pub fn date_of_unix_ms(ms: u64) -> String {
    let (y, m, d) = civil_from_days(((ms / 1000) as i64).div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// 1970-01-01 起的天数 → (年, 月, 日)。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_epoch_and_known_instants() {
        assert_eq!(format_unix_ms(0), "1970-01-01T00:00:00.000Z");
        // 2026-10-29T20:12:33.123Z
        assert_eq!(
            format_unix_ms(1_793_304_753_123),
            "2026-10-29T20:12:33.123Z"
        );
        assert_eq!(date_of_unix_ms(1_793_304_753_123), "2026-10-29");
    }

    #[test]
    fn handles_leap_day() {
        // 2024-02-29T00:00:00Z
        assert_eq!(
            format_unix_ms(1_709_164_800_000),
            "2024-02-29T00:00:00.000Z"
        );
    }

    #[test]
    fn fixed_clock_can_step() {
        let clock = FixedClock::with_step(1_000, 5);
        assert_eq!(clock.now_unix_ms(), 1_005);
        assert_eq!(clock.now_unix_ms(), 1_010);
        clock.advance(10);
        assert_eq!(clock.now_unix_ms(), 1_025);
    }
}
