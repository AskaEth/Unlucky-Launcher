//! 退出分析：退出码语义 + `exit 0` 静默失败检测。
//!
//! 平移自 `analyse/02` §6.3/§7.2（`ExitWaiter` 的三源判定），并按 §8.4-2 / §7.10-10 做两处升级：
//!
//! 1. **完整退出码表**：不只认 Linux 的 `137`，Windows 的 NTSTATUS
//!    （`0xC0000005` 等）也要能说清是什么；
//! 2. **静默失败单独立标**：退出码 0 但存活 <30s 或 stderr 带错误特征时标记
//!    `silent_failure`——HMCL 会判 NORMAL 放过（LaunchWrapper 吞异常后 `exit 0` 是
//!    真实存在的失败形态）。

use crate::launch::pump::LogLine;

/// 退出判定类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitType {
    /// 正常退出。
    Normal,
    /// 应用层崩溃（游戏起来了但崩了）。
    ApplicationError,
    /// JVM 都没起来（`-Xmx` 过大、JVM 参数非法等）。
    JvmError,
    /// 被 `SIGKILL` 结束（POSIX 137）。
    SigKill,
    /// 被 `SIGTERM` 结束（POSIX 143）。
    SigTerm,
    /// 被启动器主动结束（用户点取消 / 看门狗强制）。
    Killed,
    /// 等待被中断。
    Interrupted,
}

impl ExitType {
    /// 事件流里的字符串形式（与 M15 `ExitType` 刻意保持一致）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::ApplicationError => "application_error",
            Self::JvmError => "jvm_error",
            Self::SigKill => "sigkill",
            Self::SigTerm => "sigterm",
            Self::Killed => "killed",
            Self::Interrupted => "interrupted",
        }
    }

    /// 是否属于“非正常退出”。
    pub fn is_abnormal(self) -> bool {
        self != Self::Normal
    }
}

/// JVM 未能启动的三条关键字（`analyse/02` §7.2）。
pub const JVM_FAILURE_KEYWORDS: &[&str] = &[
    "Could not create the Java Virtual Machine.",
    "Error occurred during initialization of VM",
    "A fatal exception has occurred. Program will exit.",
];

/// 应用层崩溃的五条关键字（`analyse/02` §7.2，覆盖三种崩法）。
pub const CRASH_KEYWORDS: &[&str] = &[
    "Crash report saved to",
    "Could not save crash report to",
    "This crash report has been saved to:",
    "Unable to launch",
    "An exception was thrown, the game will display an error screen and halt.",
];

/// 静默失败的存活阈值（毫秒）：<30s 视为“没真正跑起来”。
pub const SHORT_LIVED_MS: u64 = 30_000;

/// 退出分析的输入。
#[derive(Debug, Clone, Copy)]
pub struct ExitSignals<'a> {
    /// 退出码。
    pub code: i32,
    /// 进程存活毫秒数。
    pub uptime_ms: u64,
    /// 环形缓冲里的日志行。
    pub lines: &'a [LogLine],
    /// 是否启动器主动结束。
    pub killed: bool,
    /// 等待是否被中断。
    pub interrupted: bool,
    /// 是否 POSIX（决定 137/143 是否按信号解读）。
    pub is_posix: bool,
}

impl<'a> ExitSignals<'a> {
    /// 构造。
    pub fn new(code: i32, uptime_ms: u64, lines: &'a [LogLine]) -> Self {
        Self {
            code,
            uptime_ms,
            lines,
            killed: false,
            interrupted: false,
            is_posix: cfg!(unix),
        }
    }
}

/// 静默失败标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SilentFailure {
    /// 存活时间过短。
    pub short_lived: bool,
    /// stderr/日志里有错误特征。
    pub error_output: bool,
}

impl SilentFailure {
    /// 人读说明。
    pub fn describe(&self) -> String {
        match (self.short_lived, self.error_output) {
            (true, true) => "退出码 0，但存活不足 30 秒且输出含错误特征".to_string(),
            (true, false) => "退出码 0，但存活不足 30 秒".to_string(),
            (false, true) => "退出码 0，但输出含错误特征".to_string(),
            (false, false) => "退出码 0".to_string(),
        }
    }
}

/// 退出分析结果（`exit` 事件的 payload 来源）。
#[derive(Debug, Clone)]
pub struct ExitAnalysis {
    /// 退出码。
    pub code: i32,
    /// 判定类型。
    pub exit_type: ExitType,
    /// 存活毫秒数。
    pub uptime_ms: u64,
    /// 错误级日志行数。
    pub error_lines: u32,
    /// 命中的崩溃关键字。
    pub crash_keywords: Vec<String>,
    /// 命中的 JVM 失败关键字。
    pub jvm_keywords: Vec<String>,
    /// 静默失败标记（仅 `Normal` 时可能非空）。
    pub silent_failure: Option<SilentFailure>,
    /// Windows NTSTATUS 解释（例 `0xC0000005 ACCESS_VIOLATION`）。
    pub windows_status: Option<String>,
    /// 是否应该进入诊断流水线（M15）。
    pub needs_diagnosis: bool,
}

impl ExitAnalysis {
    /// 一句话摘要（UI/agent 可直接展示）。
    pub fn summary(&self) -> String {
        let mut text = format!(
            "退出码 {}（{}），存活 {:.1}s，错误行 {}",
            self.code,
            self.exit_type.as_str(),
            self.uptime_ms as f64 / 1000.0,
            self.error_lines
        );
        if let Some(status) = &self.windows_status {
            text.push_str(&format!("，{status}"));
        }
        if !self.crash_keywords.is_empty() {
            text.push_str(&format!("，崩溃关键字 {}", self.crash_keywords.len()));
        }
        if let Some(silent) = &self.silent_failure {
            text.push_str(&format!("，{}", silent.describe()));
        }
        text
    }
}

/// 做退出分析。
pub fn analyze(signals: &ExitSignals<'_>) -> ExitAnalysis {
    let error_lines = signals.lines.iter().filter(|line| line.is_error()).count();
    let crash_keywords = matched_keywords(signals.lines, CRASH_KEYWORDS);
    let jvm_keywords = matched_keywords(signals.lines, JVM_FAILURE_KEYWORDS);
    let error_output = signals
        .lines
        .iter()
        .any(|line| line.is_error() && looks_like_failure(&line.text));

    let exit_type = if signals.interrupted {
        ExitType::Interrupted
    } else if signals.killed {
        ExitType::Killed
    } else if signals.code != 0 && !jvm_keywords.is_empty() {
        ExitType::JvmError
    } else if signals.is_posix && signals.code == 137 {
        ExitType::SigKill
    } else if signals.is_posix && signals.code == 143 {
        ExitType::SigTerm
    } else if signals.code != 0 || !crash_keywords.is_empty() {
        ExitType::ApplicationError
    } else {
        ExitType::Normal
    };

    let silent_failure = if exit_type == ExitType::Normal {
        let short_lived = signals.uptime_ms < SHORT_LIVED_MS;
        if short_lived || error_output {
            Some(SilentFailure {
                short_lived,
                error_output,
            })
        } else {
            None
        }
    } else {
        None
    };

    ExitAnalysis {
        code: signals.code,
        exit_type,
        uptime_ms: signals.uptime_ms,
        error_lines: u32::try_from(error_lines).unwrap_or(u32::MAX),
        crash_keywords: crash_keywords.clone(),
        jvm_keywords,
        silent_failure,
        windows_status: windows_status_name(signals.code),
        needs_diagnosis: exit_type.is_abnormal()
            || silent_failure.is_some()
            || !crash_keywords.is_empty(),
    }
}

/// Windows NTSTATUS → 解释。
pub fn windows_status_name(code: i32) -> Option<String> {
    let (hex, name) = match code {
        -1_073_741_819 => ("0xC0000005", "ACCESS_VIOLATION"),
        -1_073_740_791 => ("0xC0000409", "STACK_BUFFER_OVERRUN"),
        -1_073_741_515 => ("0xC0000135", "DLL_NOT_FOUND"),
        -1_073_741_571 => ("0xC00000FD", "STACK_OVERFLOW"),
        -1_073_741_510 => ("0xC000013A", "CONTROL_C_EXIT"),
        _ => return None,
    };
    Some(format!("{hex} {name}"))
}

/// 退出码的人工提示（附录 C 速查表的裁剪版）。
pub fn exit_code_hint(code: i32, is_posix: bool) -> Option<&'static str> {
    match code {
        0 => Some("正常退出"),
        1 => Some("通用失败（含 JVM 启动失败）"),
        137 if is_posix => Some("SIGKILL：被强杀"),
        143 if is_posix => Some("SIGTERM：被请求退出"),
        _ => None,
    }
}

fn matched_keywords(lines: &[LogLine], keywords: &[&str]) -> Vec<String> {
    keywords
        .iter()
        .filter(|keyword| lines.iter().any(|line| line.text.contains(**keyword)))
        .map(|keyword| (*keyword).to_string())
        .collect()
}

/// “像失败的输出”：显式异常/错误特征（不是任何 stderr 输出都算）。
fn looks_like_failure(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("exception")
        || lower.contains("caused by")
        || lower.contains("fatal")
        || lower.contains("error:")
        || lower.contains("failed")
}
