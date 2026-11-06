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
