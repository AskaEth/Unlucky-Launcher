//! 原始行尾窗（M241）：诊断要在**脱敏前**的文本上跑，落盘仍然只有脱敏文本。
//!
//! 会话事件流是写前脱敏的，脱敏的两条形状规则有损（见 [`crate::diagnose::session`] 的模块
//! 文档）。所以边界是：原始行**只在本进程内存里活到会话收尾**，收尾时算一次规则结论
//! （`diagnose::session::conclude`），写进 JSONL 的是结论本体，仍然过 `Redactor::scrub_json`。
//!
//! 容量按展示侧 `DEFAULT_TAIL` 同量级取：规则引擎看的是最近的日志，不是开局那几行；
//! 错误行数另外全量统计（`exit0 静默失败` 那类判据要的是总数，不是尾窗里的数）。

use std::collections::VecDeque;

use crate::launch::pump::LogLine;

/// 保留多少行原始文本（尾窗）。
pub const CAPACITY: usize = 2000;

/// 原始行尾窗 + 全量错误行计数。
#[derive(Debug, Default)]
pub struct RawWindow {
    lines: VecDeque<(bool, String)>,
    errors: u32,
}

impl RawWindow {
    /// 记一行**原始**（未脱敏）文本。
    pub fn push(&mut self, line: &LogLine) {
        if line.level().is_error() {
            self.errors = self.errors.saturating_add(1);
        }
        if self.lines.len() >= CAPACITY {
            self.lines.pop_front();
        }
        self.lines
            .push_back((line.stream.is_stderr(), line.text.clone()));
    }

    /// 全量错误行数。
    pub fn error_lines(&self) -> u32 {
        self.errors
    }

    /// 尾窗按流拆开：`(stdout, stderr)`，各自按时间顺序。
    pub fn facts(&self) -> (Vec<String>, Vec<String>) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        for (is_stderr, text) in &self.lines {
            if *is_stderr {
                stderr.push(text.clone());
            } else {
                stdout.push(text.clone());
            }
        }
        (stdout, stderr)
    }
}

#[cfg(test)]
#[path = "raw_window_tests.rs"]
mod tests;
