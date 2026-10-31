//! stdout/stderr 泵与日志环形缓冲。
//!
//! 平移自 `analyse/02` §6.1/§6.2（`StreamPump` + `ManagedProcess.lines`），并补两处改进：
//! 缓冲按“行数 + 字节数”双上限（§8.4-9），且命中 `Exception|Error|Caused by|OutOfMemory`
//! 的关键行**不随环形淘汰**——2000 行截断会把根因行挤掉（§7.10-3）。
//!
//! 解码：我们强制给子进程下发 `-Dstdout.encoding=UTF-8`（`-Dsun.stdout.encoding`
//! for Java <19），因此泵按 UTF-8 宽松解码；用户把编码改成 GBK 时会出现替换字符，
//! 阶段B 再视需要引入 `encoding_rs`（记录在 `launch-接口约定.md` §7）。

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::{ChildStderr, ChildStdout};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::launch::log_level;

/// 默认环形缓冲行数（与 HMCL 一致）。
pub const DEFAULT_MAX_LINES: usize = 2000;
/// 默认环形缓冲字节上限。
pub const DEFAULT_MAX_BYTES: usize = 4 * 1024 * 1024;
/// 单行最大字符数（超出截断并标注，避免单行撑爆事件流）。
pub const MAX_LINE_CHARS: usize = 8192;

/// 输出流。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// 标准输出。
    Stdout,
    /// 标准错误。
    Stderr,
}

impl Stream {
    /// 事件流里的字符串形式（`"stdout"` / `"stderr"`）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }

    /// 是否标准错误。
    pub fn is_stderr(self) -> bool {
        self == Self::Stderr
    }
}

/// 一行日志。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// 来源流。
    pub stream: Stream,
    /// 文本（已去行尾、超长已截断）。
    pub text: String,
}

impl LogLine {
    /// 构造。
    pub fn new(stream: Stream, text: impl Into<String>) -> Self {
        Self {
            stream,
            text: text.into(),
        }
    }

    /// 猜测的级别。
    pub fn level(&self) -> log_level::Level {
        log_level::guess(self.stream.is_stderr(), &self.text)
    }

    /// 是否错误级。
    pub fn is_error(&self) -> bool {
        self.level().is_error()
    }
}

/// 日志观察者（会话事件流、M15 采集器适配层都实现它）。
pub trait LineSink: Send + Sync {
    /// 收到一行日志。
    fn on_line(&self, line: &LogLine);
}

/// 丢弃所有行的 sink（测试/无监听场景）。
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSink;

impl LineSink for NullSink {
    fn on_line(&self, _line: &LogLine) {}
}

/// 环形缓冲配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingBufferConfig {
    /// 行数上限。
    pub max_lines: usize,
    /// 字节上限。
    pub max_bytes: usize,
}

impl Default for RingBufferConfig {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_MAX_LINES,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

/// 环形日志缓冲：近期行 + 永久保留的关键行。
#[derive(Debug)]
pub struct RingBuffer {
    config: RingBufferConfig,
    recent: VecDeque<LogLine>,
    retained: Vec<LogLine>,
    bytes: usize,
    pushed: u64,
}

impl RingBuffer {
    /// 构造。
    pub fn new(config: RingBufferConfig) -> Self {
        Self {
            config,
            recent: VecDeque::new(),
            retained: Vec::new(),
            bytes: 0,
            pushed: 0,
        }
    }

    /// 压入一行，必要时淘汰最早的行（关键行转入 `retained`）。
    pub fn push(&mut self, line: LogLine) {
        self.pushed += 1;
        let size = line.text.len();
        self.bytes += size;
        self.recent.push_back(line);
        while self.recent.len() > self.config.max_lines || self.bytes > self.config.max_bytes {
            let Some(evicted) = self.recent.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(evicted.text.len());
            if log_level::is_key_line(&evicted.text) {
                self.retained.push(evicted);
            }
        }
    }

    /// 按时间顺序取全部可读行（保留的关键行在前）。
    pub fn lines(&self) -> impl Iterator<Item = &LogLine> {
        self.retained.iter().chain(self.recent.iter())
    }

    /// 拼接成文本（喂给规则引擎/日志切片）。
    pub fn text(&self) -> String {
        let mut out = String::with_capacity(self.bytes.min(64 * 1024));
        for line in self.lines() {
            out.push_str(&line.text);
            out.push('\n');
        }
        out
    }

    /// 只要错误级行（退出分析的输入）。
    pub fn error_lines(&self) -> Vec<&LogLine> {
        self.lines().filter(|line| line.is_error()).collect()
    }

    /// 当前可读行数。
    pub fn len(&self) -> usize {
        self.retained.len() + self.recent.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 累计压入行数（含已被淘汰的）。
    pub fn total_pushed(&self) -> u64 {
        self.pushed
    }
}

impl Default for RingBuffer {
    fn default() -> Self {
        Self::new(RingBufferConfig::default())
    }
}

/// 单条流结束的回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamFinished {
    /// 哪条流。
    pub stream: Stream,
    /// 累计行数。
    pub line_count: u64,
    /// 读取错误（有的话）。
    pub error: Option<String>,
}

/// 一次等待 PumpGroup 的结果。
#[derive(Debug, Clone, Default)]
pub struct PumpCompletion {
    /// 已结束的流。
    pub finished: Vec<StreamFinished>,
    /// 超时仍未结束的流。
    pub stalled: Vec<Stream>,
}

impl PumpCompletion {
    /// 是否全部结束。
    pub fn is_complete(&self) -> bool {
        self.stalled.is_empty()
    }

    /// 累计行数。
    pub fn total_lines(&self) -> u64 {
        self.finished.iter().map(|item| item.line_count).sum()
    }
}

/// 一组泵线程（stdout + stderr）。
#[derive(Debug)]
pub struct PumpGroup {
    handles: Vec<JoinHandle<()>>,
    receiver: Receiver<StreamFinished>,
    streams: Vec<Stream>,
    completed: Vec<Stream>,
}

impl PumpGroup {
    /// 为子进程的输出流派生泵线程。
    ///
    /// `buffer` 与 `sink` 同时被两个泵线程共享：缓冲区用于“最后 N 行”，
    /// sink 用于事件流落盘。
    pub fn spawn(
        stdout: Option<ChildStdout>,
        stderr: Option<ChildStderr>,
        buffer: Arc<Mutex<RingBuffer>>,
        sink: Arc<dyn LineSink>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let mut handles = Vec::new();
        let mut streams = Vec::new();
        if let Some(stdout) = stdout {
            streams.push(Stream::Stdout);
            handles.push(spawn_one(
                Box::new(stdout),
                Stream::Stdout,
                buffer.clone(),
                sink.clone(),
                sender.clone(),
            ));
        }
        if let Some(stderr) = stderr {
            streams.push(Stream::Stderr);
            handles.push(spawn_one(
                Box::new(stderr),
                Stream::Stderr,
                buffer,
                sink,
                sender.clone(),
            ));
        }
        drop(sender);
        Self {
            handles,
            receiver,
            streams,
            completed: Vec::new(),
        }
    }

    /// 期望的流数。
    pub fn expected(&self) -> usize {
        self.streams.len()
    }

    /// 等待所有泵结束（带超时，避免 HMCL `pump.join()` 永久挂死的风险）。
    pub fn wait(&mut self, timeout: Duration) -> PumpCompletion {
        let deadline = Instant::now() + timeout;
        let mut finished = Vec::new();
        while self.completed.len() < self.streams.len() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match self.receiver.recv_timeout(remaining) {
                Ok(item) => {
                    if !self.completed.contains(&item.stream) {
                        self.completed.push(item.stream);
                    }
                    finished.push(item);
                }
                Err(_) => break,
            }
        }
        let stalled = self
            .streams
            .iter()
            .copied()
            .filter(|stream| !self.completed.contains(stream))
            .collect();
        PumpCompletion { finished, stalled }
    }

    /// 强制 join（可能永久阻塞——只在上层已经确认流已关闭时使用）。
    pub fn join_all(&mut self) {
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

type BoxedReader = Box<dyn Read + Send>;

fn spawn_one(
    reader: BoxedReader,
    stream: Stream,
    buffer: Arc<Mutex<RingBuffer>>,
    sink: Arc<dyn LineSink>,
    sender: Sender<StreamFinished>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name(format!("launch-pump-{}", stream.as_str()))
        .spawn(move || {
            let mut reader = BufReader::new(reader);
            let mut raw = Vec::with_capacity(4096);
            let mut line_count = 0u64;
            let mut error = None;
            loop {
                raw.clear();
                match reader.read_until(b'\n', &mut raw) {
                    Ok(0) => break,
                    Ok(_) => {
                        let text = decode_line(&raw);
                        if text.is_empty() {
                            continue;
                        }
                        line_count += 1;
                        let line = LogLine::new(stream, text);
                        if let Ok(mut guard) = buffer.lock() {
                            guard.push(line.clone());
                        }
                        sink.on_line(&line);
                    }
                    Err(err) => {
                        error = Some(err.to_string());
                        break;
                    }
                }
            }
            let _ = sender.send(StreamFinished {
                stream,
                line_count,
                error,
            });
        })
        .expect("spawn log pump thread")
}

fn decode_line(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let trimmed = text.trim_end_matches(['\r', '\n']);
    truncate_line(trimmed)
}

fn truncate_line(text: &str) -> String {
    if text.chars().count() <= MAX_LINE_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(MAX_LINE_CHARS).collect();
    let extra = text.chars().count() - MAX_LINE_CHARS;
    format!("{head}…[truncated {extra} chars]")
}
