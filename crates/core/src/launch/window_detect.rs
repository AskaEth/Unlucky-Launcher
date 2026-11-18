//! “游戏窗口是否出现”的检测（日志启发式）。
//!
//! HMCL 只看两条小写子串（`analyse/02` §6.5：`lwjgl version` / `lwjgl openal`），
//! 换日志格式就失效。v1 保持同样的启发式但做成**可扩展模式表 + 命中证据 + 耗时**，
//! 这样 `window_detected` 事件能告诉 AI“靠哪一行判定的、花了多久”；
//! 进程/窗口级信号（Windows `EnumWindows` 按 PID）留 P1（`PlanDoc/06` §1 升级点 3）。

use std::time::Instant;

/// 一次成功检测。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    /// 启发式名称（事件流 `window_detected.heuristic`）。
    pub heuristic: String,
    /// 从进程启动到命中的毫秒数。
    pub took_ms: u64,
    /// 命中的日志行（事件流 `evidence`）。
    pub evidence: String,
}

/// 启发式模式表：（小写子串，启发式名称）。
pub const HEURISTICS: &[(&str, &str)] = &[
    ("lwjgl version", "lwjgl_version"),
    ("lwjgl openal", "lwjgl_openal"),
    ("sound engine started", "sound_engine"),
];

/// 关闭标记的模式表（小写子串 → 名称）。
///
/// 游戏**自己说要退出**时会打这几行；之后进程还不退，就是卡在关闭（D6 的实机形态：
/// 1.12.2 关窗后 `java.exe` 仍在跑）。表要保守——宁少判，不要把正常日志当关闭信号。
pub const SHUTDOWN_MARKERS: &[(&str, &str)] = &[
    ("stopping!", "stopping"),
    ("stopping render thread", "render_thread"),
    ("stopping worker threads", "worker_threads"),
];

/// 命中关闭标记时返回启发式名称。
#[must_use]
pub fn shutdown_marker(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    SHUTDOWN_MARKERS
        .iter()
        .find(|(needle, _)| lower.contains(needle))
        .map(|(_, name)| *name)
}

/// 窗口检测器。
#[derive(Debug, Clone)]
pub struct WindowDetector {
    started_at: Instant,
    detection: Option<Detection>,
}

impl WindowDetector {
    /// 以“现在”为计时起点。
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            detection: None,
        }
    }

    /// 指定计时起点（进程 spawn 时刻）。
    pub fn with_start(started_at: Instant) -> Self {
        Self {
            started_at,
            detection: None,
        }
    }

    /// 观察一行日志；首次命中返回检测结果。
    pub fn observe(&mut self, line: &str) -> Option<Detection> {
        if self.detection.is_some() {
            return None;
        }
        let lower = line.to_ascii_lowercase();
        let (needle, name) = HEURISTICS
            .iter()
            .find(|(needle, _)| lower.contains(needle))?;
        let _ = needle;
        let detection = Detection {
            heuristic: (*name).to_string(),
            took_ms: u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
            evidence: line.trim().to_string(),
        };
        self.detection = Some(detection.clone());
        Some(detection)
    }

    /// 是否已经检测到窗口。
    pub fn is_detected(&self) -> bool {
        self.detection.is_some()
    }

    /// 检测结果。
    pub fn detection(&self) -> Option<&Detection> {
        self.detection.as_ref()
    }
}

impl Default for WindowDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{shutdown_marker, WindowDetector};

    #[test]
    fn window_heuristics_are_case_insensitive_and_reported_once() {
        let mut detector = WindowDetector::new();
        assert!(detector
            .observe("[12:00:00] [main/INFO]: nothing here")
            .is_none());
        let detection = detector
            .observe("[12:00:01] [main/INFO]: LWJGL Version: 3.3.1")
            .expect("应命中窗口启发式");
        assert_eq!(detection.heuristic, "lwjgl_version");
        assert!(detector.is_detected());
        assert!(
            detector.observe("LWJGL Version again").is_none(),
            "只报一次"
        );
    }

    #[test]
    fn shutdown_markers_cover_both_log_generations() {
        // 1.12.2：`[Client thread/INFO] [minecraft/Minecraft]: Stopping!`
        assert_eq!(
            shutdown_marker("[Client thread/INFO]: Stopping!"),
            Some("stopping")
        );
        // 1.20+：渲染线程与工作线程各自打印。
        assert_eq!(
            shutdown_marker("[Render thread/INFO]: Stopping render thread"),
            Some("render_thread")
        );
        assert_eq!(
            shutdown_marker("[Worker-Main-1/INFO]: Stopping worker threads"),
            Some("worker_threads")
        );
        // 普通日志不能被当成关闭信号。
        assert_eq!(
            shutdown_marker("[main/INFO]: Started serving on 25565"),
            None
        );
        assert_eq!(shutdown_marker("Stopping server is not our business"), None);
    }
}
