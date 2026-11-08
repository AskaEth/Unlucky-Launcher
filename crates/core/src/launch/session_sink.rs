//! 把会话事件流接到日志泵上：逐行落盘 + 自动窗口出现检测。
//!
//! 放在单独文件是为了让 [`crate::launch::session`] 保持“只写事件、只管索引”的单一职责，
//! 也避免单文件超过 400 行的硬约束。

use std::sync::Mutex;

use crate::launch::pump::{LineSink, LogLine};
use crate::launch::session::SessionWriter;
use crate::launch::window_detect::WindowDetector;

/// 会话日志 sink。
#[derive(Debug)]
pub struct SessionSink {
    writer: Mutex<SessionWriter>,
    detector: Mutex<WindowDetector>,
}

impl SessionSink {
    /// 以进程启动时刻为窗口检测起点。
    pub fn new(writer: SessionWriter) -> Self {
        Self {
            writer: Mutex::new(writer),
            detector: Mutex::new(WindowDetector::new()),
        }
    }

    /// 指定窗口检测计时起点（进程 spawn 时刻）。
    pub fn with_start(writer: SessionWriter, started_at: std::time::Instant) -> Self {
        Self {
            writer: Mutex::new(writer),
            detector: Mutex::new(WindowDetector::with_start(started_at)),
        }
    }

    /// 借用写入器（退出分析、结束会话等需要可变访问的场合）。
    pub fn with_writer<R>(&self, action: impl FnOnce(&mut SessionWriter) -> R) -> Option<R> {
        self.writer.lock().ok().map(|mut guard| action(&mut guard))
    }

    /// 是否已检测到游戏窗口。
    pub fn window_detected(&self) -> bool {
        match self.writer.lock() {
            Ok(writer) => writer.is_window_detected(),
            Err(_) => false,
        }
    }
}

impl LineSink for SessionSink {
    fn on_line(&self, line: &LogLine) {
        let detection = match self.detector.lock() {
            Ok(mut detector) => detector.observe(&line.text),
            Err(_) => None,
        };
        let Ok(mut writer) = self.writer.lock() else {
            return;
        };
        if let Some(detection) = detection {
            let _ = writer.window_detected(&detection);
        }
        let _ = writer.line(line);
    }
}
