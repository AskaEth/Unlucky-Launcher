//! 启动看门狗：把「还在慢慢加载」与「活着但没动静」分开（D6）。
//!
//! M208 实机：1.12.2 关窗后 `java.exe` 仍存活、CPU 继续涨，`ull launch` 阻塞 >2 分钟，
//! 用户视角就是启动器卡死。`watchdog.rs` 只管**日志泵**的预算，进程本身是无限等的。
//!
//! 这里只做判据（纯逻辑，不碰时钟也不碰进程），两条互不干扰：
//!
//! 1. **窗口从未出现**：进程起来后 `startup_limit` 内**既没见到游戏窗口、也没有任何新日志**，
//!    ⇒ `startup_timeout`。窗口一旦出现过，这条**永久失效**——大整合包首次启动慢是正常现象。
//!    只按「多久没窗口」判会把慢加载判死，所以判据是**静默**：还在打日志就说明它还在干活。
//! 2. **已经在关闭**：日志里出现关闭标记（`Stopping!` 等）后 `shutdown_limit` 仍不退出
//!    ⇒ `shutdown_timeout`。这条只在游戏自己说要退出之后才生效，正是 D6 那个「关窗后挂死」。
//!
//! 两个上限都有默认值、都可配（[`crate::launch::process::ProcessConfig`]），也可以关掉（`0`）。
//! 判出超时**不等于**要杀进程：调用方只停止等待，见
//! [`crate::launch::launch_session::LaunchSession::finish`]。

use std::time::Duration;

/// 「还要不要继续等」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchVerdict {
    /// 继续等。
    Wait,
    /// 窗口从未出现、且连续这么久没有任何日志输出。
    StartupTimeout {
        /// 已经静默了多久（从最后一行日志算）。
        waited: Duration,
        /// 配置的静默预算。
        limit: Duration,
    },
    /// 已出现关闭标记，且超出宽限仍未退出。
    ShutdownTimeout {
        /// 从关闭标记到现在等了多久。
        waited: Duration,
        /// 配置的宽限。
        limit: Duration,
    },
}

impl WatchVerdict {
    /// 是否应当继续等。
    #[must_use]
    pub fn is_wait(self) -> bool {
        self == Self::Wait
    }

    /// 稳定字符串 id（`launcher.log` / `--json` / 错误码用）。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Wait => "wait",
            Self::StartupTimeout { .. } => "startup_timeout",
            Self::ShutdownTimeout { .. } => "shutdown_timeout",
        }
    }

    /// 判据上限（毫秒；`Wait` 为 0）。
    #[must_use]
    pub fn limit_ms(self) -> u64 {
        let limit = match self {
            Self::Wait => Duration::ZERO,
            Self::StartupTimeout { limit, .. } | Self::ShutdownTimeout { limit, .. } => limit,
        };
        millis(limit)
    }

    /// 已经等了多久（毫秒；`Wait` 为 0）。
    #[must_use]
    pub fn waited_ms(self) -> u64 {
        match self {
            Self::Wait => 0,
            Self::StartupTimeout { waited, .. } | Self::ShutdownTimeout { waited, .. } => {
                millis(waited)
            }
        }
    }

    /// 人读一行。
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::Wait => "继续等待".to_string(),
            Self::StartupTimeout { waited, limit } => format!(
                "启动 {:.1}s 没有任何日志输出、也没见到游戏窗口（静默预算 {:.1}s）",
                seconds(waited),
                seconds(limit)
            ),
            Self::ShutdownTimeout { waited, limit } => format!(
                "已开始关闭但 {:.1}s 仍未退出（宽限 {:.1}s）",
                seconds(waited),
                seconds(limit)
            ),
        }
    }
}

fn seconds(value: Duration) -> f64 {
    value.as_secs_f64()
}

fn millis(value: Duration) -> u64 {
    u64::try_from(value.as_millis()).unwrap_or(u64::MAX)
}

/// 看门狗状态：见过的信号 + 两个上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupWatch {
    startup_limit: Duration,
    shutdown_limit: Duration,
    window_seen: bool,
    last_activity: Duration,
    shutdown_at: Option<Duration>,
}

impl StartupWatch {
    /// 用两个上限构造（`Duration::ZERO` = 该条不判）。
    #[must_use]
    pub fn new(startup_limit: Duration, shutdown_limit: Duration) -> Self {
        Self {
            startup_limit,
            shutdown_limit,
            window_seen: false,
            last_activity: Duration::ZERO,
            shutdown_at: None,
        }
    }

    /// 见到游戏窗口（之后启动预算永久失效）。
    pub fn mark_window(&mut self) {
        self.window_seen = true;
    }

    /// 见到一行日志（进程在干活，静默计时重新开始）。
    pub fn mark_activity(&mut self, at: Duration) {
        if at > self.last_activity {
            self.last_activity = at;
        }
    }

    /// 见到关闭标记（只记第一次出现的时刻，相对进程启动）。
    pub fn mark_shutdown(&mut self, at: Duration) {
        if self.shutdown_at.is_none() {
            self.shutdown_at = Some(at);
        }
    }

    /// 是否见过窗口。
    #[must_use]
    pub fn is_window_seen(&self) -> bool {
        self.window_seen
    }

    /// 关闭标记出现的时刻（相对进程启动）。
    #[must_use]
    pub fn shutdown_at(&self) -> Option<Duration> {
        self.shutdown_at
    }

    /// 按「进程起来多久」判一次。
    #[must_use]
    pub fn verdict(&self, elapsed: Duration) -> WatchVerdict {
        // 关闭优先：游戏自己说要退了，这时候的「没窗口」不再是信息。
        if let Some(at) = self.shutdown_at {
            let waited = elapsed.saturating_sub(at);
            if !self.shutdown_limit.is_zero() && waited > self.shutdown_limit {
                return WatchVerdict::ShutdownTimeout {
                    waited,
                    limit: self.shutdown_limit,
                };
            }
            return WatchVerdict::Wait;
        }
        if !self.window_seen && !self.startup_limit.is_zero() {
            // 判据是**静默**而不是「等了多久」：还在打日志就说明它还在加载。
            let quiet = elapsed.saturating_sub(self.last_activity);
            if quiet > self.startup_limit {
                return WatchVerdict::StartupTimeout {
                    waited: quiet,
                    limit: self.startup_limit,
                };
            }
        }
        WatchVerdict::Wait
    }
}

#[cfg(test)]
mod tests {
    use super::{StartupWatch, WatchVerdict};
    use std::time::Duration;

    fn secs(value: u64) -> Duration {
        Duration::from_secs(value)
    }

    fn watch() -> StartupWatch {
        StartupWatch::new(secs(180), secs(60))
    }

    #[test]
    fn waits_inside_the_startup_budget() {
        let watch = watch();
        assert_eq!(watch.verdict(secs(179)), WatchVerdict::Wait);
        assert!(matches!(
            watch.verdict(secs(181)),
            WatchVerdict::StartupTimeout { .. }
        ));
    }

    #[test]
    fn waiting_stays_alive_while_log_lines_keep_coming() {
        // 慢加载：一直没有窗口，但每 20s 都有一行日志 → 静默永远不越限。
        let mut watch = watch();
        for second in 1..=30u64 {
            watch.mark_activity(secs(second * 20));
            assert_eq!(
                watch.verdict(secs(second * 20 + 1)),
                WatchVerdict::Wait,
                "第 {second} 分钟还在打日志，不该判卡死"
            );
        }
        // 之后真的静默了 → 按静默时长报，而不是按总时长。
        assert_eq!(
            watch.verdict(secs(600 + 181)),
            WatchVerdict::StartupTimeout {
                waited: secs(181),
                limit: secs(180)
            }
        );
    }

    #[test]
    fn a_seen_window_disables_the_startup_budget_for_good() {
        // 大整合包慢慢加载：窗口出现过就不再判「卡在启动」，等到天荒地老也只是 Wait。
        let mut watch = watch();
        watch.mark_window();
        assert_eq!(watch.verdict(secs(1800)), WatchVerdict::Wait);
    }

    #[test]
    fn shutdown_grace_only_applies_after_the_marker() {
        let mut watch = watch();
        watch.mark_shutdown(secs(300));
        assert_eq!(watch.verdict(secs(330)), WatchVerdict::Wait);
        assert_eq!(
            watch.verdict(secs(361)),
            WatchVerdict::ShutdownTimeout {
                waited: secs(61),
                limit: secs(60)
            }
        );
    }

    #[test]
    fn shutdown_marker_wins_over_the_startup_budget() {
        // 没窗口但已经打了「正在关闭」：按关闭判，不按启动判（哪怕总时长已越过启动预算）。
        let mut watch = watch();
        watch.mark_shutdown(secs(175));
        assert_eq!(watch.verdict(secs(200)), WatchVerdict::Wait);
        // 宽限一到就改口：从关闭标记算起超过 60s 才是「卡在关闭」。
        assert!(matches!(
            watch.verdict(secs(236)),
            WatchVerdict::ShutdownTimeout { .. }
        ));
    }

    #[test]
    fn zero_limits_disable_the_checks() {
        let mut watch = StartupWatch::new(Duration::ZERO, Duration::ZERO);
        assert_eq!(watch.verdict(secs(10_000)), WatchVerdict::Wait);
        watch.mark_shutdown(secs(1));
        assert_eq!(watch.verdict(secs(10_000)), WatchVerdict::Wait);
    }

    #[test]
    fn codes_and_descriptions_are_stable_and_name_both_numbers() {
        let verdict = WatchVerdict::StartupTimeout {
            waited: secs(300),
            limit: secs(180),
        };
        assert_eq!(verdict.code(), "startup_timeout");
        let text = verdict.describe();
        assert!(text.contains("300.0s"), "{text}");
        assert!(text.contains("180.0s"), "{text}");
        assert_eq!(
            WatchVerdict::ShutdownTimeout {
                waited: secs(90),
                limit: secs(60)
            }
            .code(),
            "shutdown_timeout"
        );
        assert!(WatchVerdict::Wait.is_wait());
        assert!(!verdict.is_wait());
    }
}
