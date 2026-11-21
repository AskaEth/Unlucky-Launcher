//! 日志 sink 组合：把同一个进程的输出同时喂给多个消费者。
//!
//! 设计依据 `DevDocs/SubAgentsTalk/launch-接口约定.md` §3（单一真相源）与 tower 批复的
//! “复合 sink”方案：会话事件流（落盘）是必需的一个 sink，M15 采集器 / UI 日志页 /
//! 测试断言各自挂自己的 sink，彼此不感知。
//!
//! 接 M15 的姿态：diagnose 不解析 jsonl、也不 import launch 类型，所以它只需要一个
//! `FnSink` 包一层 `LogBuffer::push(stream, line)`；这里只提供通用的组合原语。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::launch::pump::{LineSink, LogLine, Stream};

/// 把一条日志广播给多个 sink（顺序 = 注册顺序）。
#[derive(Default)]
pub struct CompositeSink {
    sinks: Vec<Arc<dyn LineSink>>,
}

impl CompositeSink {
    /// 空组合。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一个 sink（builder 风格）。
    pub fn with(mut self, sink: Arc<dyn LineSink>) -> Self {
        self.sinks.push(sink);
        self
    }

    /// 追加一个 sink。
    pub fn push(&mut self, sink: Arc<dyn LineSink>) {
        self.sinks.push(sink);
    }

    /// 已注册的 sink 数。
    pub fn len(&self) -> usize {
        self.sinks.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.sinks.is_empty()
    }
}

impl std::fmt::Debug for CompositeSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeSink")
            .field("sinks", &self.sinks.len())
            .finish()
    }
}

impl LineSink for CompositeSink {
    fn on_line(&self, line: &LogLine) {
        for sink in &self.sinks {
            sink.on_line(line);
        }
    }
}

/// 闭包 sink：把 M15 采集器、UI 通道、测试断言接进来的最轻方式。
///
/// ```ignore
/// let sink = FnSink::new(move |line: &LogLine| {
///     bundle.log.push(stream_kind(line.stream), &line.text);
/// });
/// ```
pub struct FnSink<F: Fn(&LogLine) + Send + Sync> {
    action: F,
}

impl<F: Fn(&LogLine) + Send + Sync> FnSink<F> {
    /// 用闭包构造。
    pub fn new(action: F) -> Self {
        Self { action }
    }
}

impl<F: Fn(&LogLine) + Send + Sync + 'static> FnSink<F> {
    /// 转成 `Arc<dyn LineSink>`（可直接塞进 [`CompositeSink`] / `RunningProcess`）。
    pub fn shared(self) -> Arc<dyn LineSink> {
        Arc::new(self)
    }
}

impl<F: Fn(&LogLine) + Send + Sync> std::fmt::Debug for FnSink<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FnSink")
    }
}

impl<F: Fn(&LogLine) + Send + Sync> LineSink for FnSink<F> {
    fn on_line(&self, line: &LogLine) {
        (self.action)(line);
    }
}

/// 计数 sink：给进度条/测试一个“已收多少行”的廉价探针。
#[derive(Debug, Default)]
pub struct CountingSink {
    stdout: AtomicU64,
    stderr: AtomicU64,
}

impl CountingSink {
    /// 新计数器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 标准输出行数。
    pub fn stdout_lines(&self) -> u64 {
        self.stdout.load(Ordering::Relaxed)
    }

    /// 标准错误行数。
    pub fn stderr_lines(&self) -> u64 {
        self.stderr.load(Ordering::Relaxed)
    }

    /// 总行数。
    pub fn total_lines(&self) -> u64 {
        self.stdout_lines() + self.stderr_lines()
    }
}

impl LineSink for CountingSink {
    fn on_line(&self, line: &LogLine) {
        match line.stream {
            Stream::Stdout => self.stdout.fetch_add(1, Ordering::Relaxed),
            Stream::Stderr => self.stderr.fetch_add(1, Ordering::Relaxed),
        };
    }
}
