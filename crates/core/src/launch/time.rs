//! 时间工具：RFC3339（毫秒，UTC）格式化与会话时钟。
//!
//! 会话事件流的 `ts` / `t_ms` 字段由这里产出（schema 见
//! `DevDocs/SubAgentsTalk/launch-接口约定.md` §2）。
//! 只用 `std::time`，不引入 chrono/time 依赖。

use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// 把 `SystemTime` 格式化成 RFC3339 UTC 毫秒精度串，例 `2026-10-29T12:45:01.123Z`。
///
/// 早于 Unix 纪元的时间按纪元处理（不会 panic：事件流宁可有 1970 的时间戳也不能丢事件）。
pub fn format_rfc3339_millis(time: SystemTime) -> String {
    let since_epoch = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since_epoch.as_secs();
    let millis = since_epoch.subsec_millis();
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
    let secs_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// 当前墙上时间（便于测试替换的单一入口）。
pub fn now() -> SystemTime {
    SystemTime::now()
}

/// 由“自 1970-01-01 起的天数”还原公历年月日（Howard Hinnant 的 civil_from_days）。
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// 会话时钟：同时记录单调时刻（`t_ms`）与墙上时刻（`ts`）。
#[derive(Debug, Clone)]
pub struct SessionClock {
    start_instant: Instant,
    started_at: SystemTime,
}

impl SessionClock {
    /// 以“现在”为起点创建时钟。
    pub fn start_new() -> Self {
        Self {
            start_instant: Instant::now(),
            started_at: now(),
        }
    }

    /// 用给定的墙上时间与单调时刻创建（测试可注入）。
    pub fn started_at(start_instant: Instant, started_at: SystemTime) -> Self {
        Self {
            start_instant,
            started_at,
        }
    }

    /// 会话开始的墙上时间。
    pub fn started_at_wall(&self) -> SystemTime {
        self.started_at
    }

    /// 会话开始时刻的 RFC3339 串。
    pub fn started_at_iso(&self) -> String {
        format_rfc3339_millis(self.started_at)
    }

    /// 距会话开始的毫秒数（`t_ms`）。
    pub fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.start_instant.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// 当前墙上时间的 RFC3339 串（`ts`）。
    pub fn now_iso(&self) -> String {
        format_rfc3339_millis(now())
    }
}

impl Default for SessionClock {
    fn default() -> Self {
        Self::start_new()
    }
}

/// 生成会话 id：`YYYYMMDDTHHMMSS-<6 位十六进制>`。
///
/// 十六进制段由墙上时间与 pid 混合而来——不引入 rand 依赖，同一秒同一进程内也能区分
/// （调用方可在冲突时追加序号）。
pub fn make_session_id(wall: SystemTime, pid: u32) -> String {
    let since_epoch = wall.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since_epoch.as_secs();
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
    let (year, month, day) = civil_from_days(days);
    let secs_of_day = secs % 86_400;
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;
    let mut hash: u64 = u64::from(since_epoch.subsec_nanos()).wrapping_add(u64::from(pid));
    hash ^= secs.rotate_left(17);
    hash = hash.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let suffix = (hash >> 40) & 0xFF_FFFF;
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}-{suffix:06x}")
}
