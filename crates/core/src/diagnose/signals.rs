//! 采集到的原子信号：日志环形缓冲 + 退出信号。
//!
//! 五类信号里的「退出码」和「日志环形缓冲」两种（analyse/02 §7.1 信号 A/B）。
//! 环境快照（信号 E）与 mod 清单在 [`crate::diagnose::collectors`] 里，因为它带采集动作。
//!
//! 环形缓冲相对 HMCL `Log.java`（固定 2000 行、一律淘汰）的升级：
//! 命中 `Exception|Error|Caused by|OutOfMemory` 的行**永久保留**，
//! 解决「崩在启动早期、根因被后续输出挤出缓冲区」（analyse/02 §7.10-3）。

use std::collections::VecDeque;
use std::fmt;

/// 一条日志文本的来源（诊断输出的 `evidence[]` 前缀也是它）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SignalOrigin {
    /// 游戏进程 stdout。
    Stdout,
    /// 游戏进程 stderr。
    Stderr,
    /// `logs/latest.log`（内存缓冲不够长时的兜底）。
    LatestLog,
    /// `crash-reports/crash-*.txt`。
    CrashReport,
    /// `hs_err_pid*.log` 的摘要文本。
    HsErr,
    /// 启动器自身日志 / 结构化事件（`cmd_built`、`assembly_failed` 等）。
    Launcher,
}

impl SignalOrigin {
    /// 稳定字符串（evidence 前缀 / 序列化用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
            Self::LatestLog => "latest_log",
            Self::CrashReport => "crash_report",
            Self::HsErr => "hs_err",
            Self::Launcher => "launcher",
        }
    }

    /// `launch` 侧 `LineSink` 的 `stream` 字段映射（`stdout` / `stderr`）。
    #[must_use]
    pub fn from_stream_name(stream: &str) -> Self {
        match stream.trim().to_ascii_lowercase().as_str() {
            "stderr" => Self::Stderr,
            _ => Self::Stdout,
        }
    }

    /// 取证优先级：同一规则在多个来源命中时保留 rank 最小者（崩溃报告 > hs_err > 日志）。
    #[must_use]
    pub const fn evidence_rank(self) -> u8 {
        match self {
            Self::CrashReport => 0,
            Self::HsErr => 1,
            Self::Launcher => 2,
            Self::LatestLog => 3,
            Self::Stdout => 4,
            Self::Stderr => 5,
        }
    }
}

impl fmt::Display for SignalOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 环形缓冲里的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// 来源。
    pub origin: SignalOrigin,
    /// 行文本（不含行尾换行）。
    pub text: String,
}

impl LogLine {
    /// 构造一行。
    #[must_use]
    pub fn new(origin: SignalOrigin, text: impl Into<String>) -> Self {
        Self {
            origin,
            text: text.into(),
        }
    }
}

/// 是否「关键行」：不随环形淘汰（HMCL 局限 3 的正面修正）。
#[must_use]
pub fn is_key_line(text: &str) -> bool {
    const MARKERS: [&str; 4] = ["Exception", "Error", "Caused by", "OutOfMemory"];
    MARKERS.iter().any(|marker| text.contains(marker))
}

/// 日志环形缓冲：最近 N 行 + 被判为关键行的历史永久保留。
#[derive(Debug, Clone)]
pub struct LogBuffer {
    capacity: usize,
    key_capacity: usize,
    recent: VecDeque<LogLine>,
    /// 已从环形区淘汰、但因为是关键行而留下来的行（按时间顺序）。
    kept: VecDeque<LogLine>,
    pushed: usize,
    dropped: usize,
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new(2000)
    }
}

impl LogBuffer {
    /// 默认容量 2000 行（与 HMCL 的 `Log` 一致）。
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            key_capacity: capacity.max(1),
            recent: VecDeque::new(),
            kept: VecDeque::new(),
            pushed: 0,
            dropped: 0,
        }
    }

    /// 关键行保留上限（默认 = 环形容量）。
    #[must_use]
    pub fn with_key_capacity(mut self, key_capacity: usize) -> Self {
        self.key_capacity = key_capacity.max(1);
        self
    }

    /// 记一行（`launch` 的 stdout/stderr 事件逐行喂进来）。
    pub fn push(&mut self, origin: SignalOrigin, line: &str) {
        let text = line.trim_end_matches(['\r', '\n']).to_owned();
        self.pushed += 1;
        if self.recent.len() >= self.capacity {
            if let Some(evicted) = self.recent.pop_front() {
                self.dropped += 1;
                if is_key_line(&evicted.text) {
                    if self.kept.len() >= self.key_capacity {
                        self.kept.pop_front();
                    }
                    self.kept.push_back(evicted);
                }
            }
        }
        self.recent.push_back(LogLine::new(origin, text));
    }

    /// 一次性喂多行（导入日志、`latest.log` 兜底）。
    pub fn extend<'a, I>(&mut self, origin: SignalOrigin, lines: I)
    where
        I: IntoIterator<Item = &'a str>,
    {
        for line in lines {
            self.push(origin, line);
        }
    }

    /// 按时间顺序返回全部保留行（关键行历史在前，最近 N 行在后）。
    pub fn lines(&self) -> impl Iterator<Item = &LogLine> {
        self.kept.iter().chain(self.recent.iter())
    }

    /// 拼成可供规则匹配的文本（`\n` 连接）。
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::with_capacity(self.byte_len());
        for line in self.lines() {
            out.push_str(&line.text);
            out.push('\n');
        }
        out
    }

    /// 当前保留行数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.kept.len() + self.recent.len()
    }

    /// 是否一行都没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 一共推入多少行。
    #[must_use]
    pub const fn pushed(&self) -> usize {
        self.pushed
    }

    /// 被淘汰多少行（不含被永久保留的关键行）。
    #[must_use]
    pub const fn dropped(&self) -> usize {
        self.dropped
    }

    /// 因关键行规则而保留下来的历史行数。
    #[must_use]
    pub fn kept_key_lines(&self) -> usize {
        self.kept.len()
    }

    fn byte_len(&self) -> usize {
        self.lines().map(|line| line.text.len() + 1).sum()
    }
}

/// 退出类型。字符串值与 `launch-接口约定.md` §3 `exit.exit_type` 完全一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ExitType {
    /// 正常退出（但可能是静默失败，见 [`ExitSignal::silent_failure`]）。
    #[default]
    Normal,
    /// 应用层崩溃（有崩溃关键字 / 非 0 退出码）。
    ApplicationError,
    /// JVM 没起来（`-Xmx` 过大、参数非法、JVM 致命错误）。
    JvmError,
    /// 被 SIGKILL（Linux 137 / 系统 OOM killer）。
    Sigkill,
    /// 被 SIGTERM（143）。
    Sigterm,
    /// 被启动器主动停止（用户取消 / watchdog）。
    Killed,
    /// 等待被中断（会话被丢弃）。
    Interrupted,
}

impl ExitType {
    /// 稳定字符串（与 launch 约定同值）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::ApplicationError => "application_error",
            Self::JvmError => "jvm_error",
            Self::Sigkill => "sigkill",
            Self::Sigterm => "sigterm",
            Self::Killed => "killed",
            Self::Interrupted => "interrupted",
        }
    }

    /// 从 launch 约定字符串反解（未知值回落 `ApplicationError`，绝不 panic）。
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "normal" => Self::Normal,
            "jvm_error" => Self::JvmError,
            "sigkill" => Self::Sigkill,
            "sigterm" => Self::Sigterm,
            "killed" => Self::Killed,
            "interrupted" => Self::Interrupted,
            _ => Self::ApplicationError,
        }
    }

    /// 是否异常结束（非 `Normal`）。
    #[must_use]
    pub const fn is_abnormal(self) -> bool {
        !matches!(self, Self::Normal)
    }
}

impl fmt::Display for ExitType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// JVM 没起来的三个签名（analyse/02 §7.2，`JVM_ERROR` 判定依据）。
pub const JVM_ERROR_KEYWORDS: [&str; 3] = [
    "Could not create the Java Virtual Machine",
    "Error occurred during initialization of VM",
    "A fatal exception has occurred. Program will exit.",
];

/// 应用层崩溃关键字（HMCL `ExitWaiter` 的 5 条 + 我们能拿到的补充）。
pub const CRASH_KEYWORDS: [&str; 5] = [
    "Game crashed! Crash report saved to",
    "Exception in thread",
    "Unhandled exception",
    "Unable to launch",
    "An exception was thrown, the game will display an error screen and halt",
];

/// Windows 常见退出码（`0xC0000005` 等按 `i32` 表示，analyse/02 §8.4-2）。
pub const EXIT_ACCESS_VIOLATION: i32 = -1_073_741_819;
/// `0xC0000409` STACK_BUFFER_OVERRUN。
pub const EXIT_STACK_BUFFER_OVERRUN: i32 = -1_073_740_791;
/// `0xC0000135` DLL_NOT_FOUND。
pub const EXIT_DLL_NOT_FOUND: i32 = -1_073_741_515;
/// POSIX SIGKILL（128 + 9）。
pub const EXIT_SIGKILL: i32 = 137;
/// POSIX SIGTERM（128 + 15）。
pub const EXIT_SIGTERM: i32 = 143;

/// 进程退出信号（`launch` 的 `exit` 事件映射过来；离线诊断时由 [`ExitSignal::classify`] 反推）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExitSignal {
    /// 退出码（被信号杀死时为负或 128+N）。
    pub code: Option<i32>,
    /// 退出类型。
    pub exit_type: ExitType,
    /// 进程存活毫秒数。
    pub uptime_ms: Option<u64>,
    /// stderr/错误行条数。
    pub error_lines: u32,
    /// 退出码 0 但疑似静默失败（存活 <30s 或 stderr 有错误特征）。
    pub silent_failure: bool,
    /// Windows 状态码原始串（`0xc0000005`）。
    pub windows_status: Option<String>,
    /// 命中的崩溃关键字。
    pub crash_keywords: Vec<String>,
}

impl ExitSignal {
    /// 从原始退出事实推断类型（离线诊断 / 导入日志用；launch 侧有权威版本）。
    #[must_use]
    pub fn classify(
        code: Option<i32>,
        uptime_ms: Option<u64>,
        error_lines: u32,
        error_text: &str,
    ) -> Self {
        let has = |keywords: &[&str]| keywords.iter().any(|kw| error_text.contains(kw));
        let mut crash_keywords = Vec::new();
        for keyword in CRASH_KEYWORDS {
            if error_text.contains(keyword) {
                crash_keywords.push(keyword.to_owned());
            }
        }
        let exit_type = match code {
            Some(EXIT_SIGKILL) => ExitType::Sigkill,
            Some(EXIT_SIGTERM) => ExitType::Sigterm,
            // 与 HMCL `ExitType` 一致：只有「非 0 退出码 + JVM 关键字」才算 JVM_ERROR。
            // exit 0 却打出这些关键字的场景归到靜默失败（silent_failure）而不是 JVM 错误。
            Some(0) => ExitType::Normal,
            Some(_) if has(&JVM_ERROR_KEYWORDS) => ExitType::JvmError,
            Some(_) => ExitType::ApplicationError,
            None => ExitType::ApplicationError,
        };
        let short_lived = uptime_ms.is_some_and(|ms| ms < 30_000);
        let silent_failure = exit_type == ExitType::Normal
            && (short_lived || error_lines > 0 || has(&CRASH_KEYWORDS));
        Self {
            code,
            exit_type,
            uptime_ms,
            error_lines,
            silent_failure,
            windows_status: code.and_then(hex_status),
            crash_keywords,
        }
    }

    /// Windows 状态码的十六进制串（如 `0xc0000005`）。
    #[must_use]
    pub fn status_hex(&self) -> Option<String> {
        self.windows_status
            .clone()
            .or_else(|| self.code.and_then(hex_status))
    }

    /// 退出码是否是 Windows 访问违例（`0xC0000005`）。
    #[must_use]
    pub fn is_access_violation(&self) -> bool {
        self.code == Some(EXIT_ACCESS_VIOLATION)
    }

    /// 一句话摘要（写进诊断报告 / 证据行）。
    #[must_use]
    pub fn summary(&self) -> String {
        let code = self
            .code
            .map_or_else(|| "未知".to_owned(), |code| code.to_string());
        let uptime = self
            .uptime_ms
            .map_or_else(String::new, |ms| format!("，存活 {} ms", ms));
        format!("退出码 {code}（{}）{uptime}", self.exit_type)
    }
}

fn hex_status(code: i32) -> Option<String> {
    // 只有 Windows 的 NTSTATUS 形态（负数高位为 1）才有意义。
    (code < 0).then(|| format!("0x{:08x}", code as u32))
}
