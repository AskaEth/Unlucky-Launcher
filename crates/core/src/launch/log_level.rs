//! 日志级别猜测与“关键行”判定。
//!
//! 平移自 `analyse/02` §6.4（`Log4jLevel.guessLevel`）：先认 log4j 注入的
//! `[HH:mm:ss] [thread/LEVEL]` 格式，再退到 JUL 的 `[INFO]` 形态；`stderr` 一律当
//! ERROR（`[authlib-injector]` 例外）。
//!
//! 目的有两个：给事件流打 `level`，以及让环形缓冲能“关键行永久保留”
//! （`analyse/02` §7.10-3：2000 行截断会把根因行挤掉）。

/// 日志级别（认知顺序：Error 最严重）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// 错误。
    Error,
    /// 警告。
    Warn,
    /// 信息。
    Info,
    /// 调试。
    Debug,
    /// 无法判断。
    Unknown,
}

impl Level {
    /// 事件流里用的字符串形式。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Unknown => "unknown",
        }
    }

    /// 是否错误级（退出分析筛行用）。
    pub fn is_error(self) -> bool {
        self == Self::Error
    }
}

/// 猜测一行日志的级别。
pub fn guess(is_stderr: bool, line: &str) -> Level {
    let trimmed = line.trim_start();
    if is_stderr {
        // authlib-injector 往 stderr 打的都是正常信息（`analyse/02` §6.4）。
        if trimmed.starts_with("[authlib-injector]") {
            return Level::Info;
        }
        return Level::Error;
    }
    if let Some(level) = log4j_level(trimmed) {
        return level;
    }
    jul_level(trimmed).unwrap_or(Level::Unknown)
}

/// 是否“关键行”：命中这些特征的行不随环形淘汰（`Exception|Error|Caused by|OutOfMemory`）。
pub fn is_key_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("exception")
        || lower.contains("error")
        || lower.contains("caused by")
        || lower.contains("outofmemory")
        || lower.contains("fatal")
}

/// 从 `[12:34:56] [Render thread/INFO]: ...` 里取级别。
///
/// 逐段扫描方括号内容，取第一段形如 `<线程名>/<LEVEL>` 的级别，最多看 3 段。
fn log4j_level(line: &str) -> Option<Level> {
    let mut rest = line;
    for _ in 0..3 {
        let open = rest.find('[')?;
        let tail = &rest[open + 1..];
        let close = tail.find(']')?;
        let segment = &tail[..close];
        if let Some(slash) = segment.rfind('/') {
            if let Some(level) = level_from_name(&segment[slash + 1..]) {
                return Some(level);
            }
        }
        rest = &tail[close + 1..];
    }
    None
}

fn level_from_name(name: &str) -> Option<Level> {
    match name.trim() {
        "ERROR" | "FATAL" => Some(Level::Error),
        "WARN" | "WARNING" => Some(Level::Warn),
        "INFO" | "NOTICE" => Some(Level::Info),
        "DEBUG" | "TRACE" | "FINE" | "FINER" | "FINEST" => Some(Level::Debug),
        _ => None,
    }
}

/// JUL/裸标记形态：`[INFO]`、`[SEVERE]`、`[WARNING]`。
fn jul_level(line: &str) -> Option<Level> {
    const MARKERS: &[(&str, Level)] = &[
        ("[SEVERE]", Level::Error),
        ("[ERROR]", Level::Error),
        ("[WARNING]", Level::Warn),
        ("[WARN]", Level::Warn),
        ("[INFO]", Level::Info),
        ("[DEBUG]", Level::Debug),
        ("[FINE]", Level::Debug),
    ];
    MARKERS
        .iter()
        .find(|(marker, _)| line.starts_with(marker))
        .map(|(_, level)| *level)
}
