//! 把会话事件流接到日志泵上：逐行落盘 + 自动窗口出现检测。
//!
//! 放在单独文件是为了让 [`crate::launch::session`] 保持“只写事件、只管索引”的单一职责，
//! 也避免单文件超过 400 行的硬约束。

use std::sync::Mutex;
use std::time::Instant;

use crate::launch::pump::{LineSink, LogLine};
use crate::launch::session::SessionWriter;
use crate::launch::window_detect::{shutdown_marker, WindowDetector};

/// 会话日志 sink。
#[derive(Debug)]
pub struct SessionSink {
    writer: Mutex<SessionWriter>,
    detector: Mutex<WindowDetector>,
    shutdown_at: Mutex<Option<Instant>>,
    last_line_at: Mutex<Option<Instant>>,
}

impl SessionSink {
    /// 以进程启动时刻为窗口检测起点。
    pub fn new(writer: SessionWriter) -> Self {
        Self::with_start(writer, Instant::now())
    }

    /// 指定窗口检测计时起点（进程 spawn 时刻）。
    pub fn with_start(writer: SessionWriter, started_at: Instant) -> Self {
        Self {
            writer: Mutex::new(writer),
            detector: Mutex::new(WindowDetector::with_start(started_at)),
            shutdown_at: Mutex::new(None),
            last_line_at: Mutex::new(None),
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

    /// 第一次见到关闭标记（游戏自己说要退出）的时刻（D6）。
    pub fn shutdown_seen_at(&self) -> Option<Instant> {
        self.shutdown_at.lock().ok().and_then(|guard| *guard)
    }

    /// 最后一行的时刻（D6：还在打日志就说明还在加载，不算卡死）。
    pub fn last_line_at(&self) -> Option<Instant> {
        self.last_line_at.lock().ok().and_then(|guard| *guard)
    }
}

impl LineSink for SessionSink {
    fn on_line(&self, line: &LogLine) {
        let detection = match self.detector.lock() {
            Ok(mut detector) => detector.observe(&line.text),
            Err(_) => None,
        };
        if let Ok(mut guard) = self.last_line_at.lock() {
            *guard = Some(Instant::now());
        }
        if shutdown_marker(&line.text).is_some() {
            if let Ok(mut guard) = self.shutdown_at.lock() {
                guard.get_or_insert_with(Instant::now);
            }
        }
        let Ok(mut writer) = self.writer.lock() else {
            return;
        };
        // 会话已经收尾（看门狗放弃等待后进程还在打日志）：不再往事件流里追加，
        // 否则 `session_end` 之后会出现孤儿行。
        if writer.is_finished() {
            return;
        }
        if let Some(detection) = detection {
            let _ = writer.window_detected(&detection);
        }
        let _ = writer.line(line);
    }
}
