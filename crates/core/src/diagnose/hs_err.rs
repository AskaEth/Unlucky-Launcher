//! `hs_err_pid*.log`（JVM 致命错误日志）解析器。
//!
//! HMCL 完全不解析它（analyse/02 §7.10-8），而 `EXCEPTION_ACCESS_VIOLATION` 这类问题
//! 只有这份文件能给出权威证据（失败帧落在显卡驱动 DLL 里就是驱动问题）。
//! 格式稳定：**标题行 + 线程段 + 内存段**，本模块只做容错抽取，字段全部 `Option`，永不失败。
//!
//! 用法（launch 对接见 `CONTRACT.md` §5）：
//! - `parse(&text)`：已有全文时用（首选，避免二次截断）；
//! - `parse_file(&path)`：给路径时用；
//! - [`HsErrReport::digest`] 产出的摘要既是规则匹配的文本，也是给它人看的证据。

use std::fs;
use std::io;
use std::path::Path;

#[path = "hs_err_report.rs"]
mod report;

pub use report::{FailedFrame, FrameKind, HsErrKind, HsErrReport, MemorySummary};

/// 解析文本（永不失败；无法识别的行直接跳过）。
#[must_use]
pub fn parse(text: &str) -> HsErrReport {
    let mut report = HsErrReport::default();
    let lines = text.lines().collect::<Vec<_>>();

    for (index, raw) in lines.iter().enumerate() {
        let line = raw.trim_end_matches('\r');
        let body = line.trim_start_matches('#').trim();
        if report.header.is_none() && is_header(body) {
            report.header = Some(body.to_owned());
        }
        if report.exception.is_none() {
            report.exception = extract_exception(body);
        }
        if report.siginfo.is_none() && body.starts_with("siginfo:") {
            report.siginfo = Some(body.to_owned());
        }
        if report.signal.is_none() {
            report.signal = body
                .contains("si_signo:")
                .then(|| body.to_owned())
                .or_else(|| extract_signal(body));
        }
        if report.jre_version.is_none() && body.contains("JRE version:") {
            report.jre_version = Some(body.to_owned());
        }
        if report.java_vm.is_none() && body.contains("Java VM:") {
            report.java_vm = Some(body.to_owned());
        }
        if report.os.is_none() && body.starts_with("OS:") {
            report.os = Some(body.to_owned());
        }
        if report.command_line.is_none() && body.starts_with("java_command:") {
            report.command_line = Some(body.to_owned());
        }
        if report.current_thread.is_none() && body.starts_with("Current thread (") {
            report.current_thread = Some(body.to_owned());
        }
        if report.memory.is_none() && body.starts_with("Memory:") {
            report.memory = parse_memory(body);
        }
        if report.failed_frame.is_none() && body.eq_ignore_ascii_case("Problematic frame:") {
            report.failed_frame = lines
                .iter()
                .skip(index + 1)
                .map(|next| next.trim_end_matches('\r'))
                .find(|next| !next.trim().is_empty() && next.trim() != "#")
                .map(|next| parse_frame(next.trim_start_matches('#').trim()));
        }
        if report.pc.is_none() && body.contains("pc=") {
            report.pc = field(body, "pc=");
        }
        if report.pid.is_none() && body.contains("pid=") {
            report.pid = field(body, "pid=").and_then(|raw| raw.parse::<u32>().ok());
        }
        if report.tid.is_none() && body.contains("tid=") {
            report.tid = field(body, "tid=");
        }
        if body.eq_ignore_ascii_case("VM Arguments:") {
            // 真实 hs_err 里 jvm_args/java_command 是顶格写的，不缩进
            report.vm_arguments = collect_unindented(&lines, index + 1, 32);
        }
        if body.starts_with("Heap:") {
            // "Heap:" 之后的缩进行才是内容（garbage-first heap total … used …）
            report.heap = collect_block(&lines, index + 1, 6);
        }
    }

    report.header_lines = collect_header_block(&lines);
    report.pc = report
        .exception
        .as_deref()
        .and_then(|line| field(line, "pc="))
        .or(report.pc);
    // pid/tid 在「异常行」和「Out of Memory Error (os_*.cpp:NNN), pid=…」两种标题里都出现，
    // 所以两种形态都认，先到先得。
    if report.pid.is_none() {
        report.pid = report
            .exception
            .as_deref()
            .and_then(|line| field(line, "pid="))
            .and_then(|raw| raw.parse::<u32>().ok());
    }
    if report.tid.is_none() {
        report.tid = report
            .exception
            .as_deref()
            .and_then(|line| field(line, "tid="));
    }
    report.kind = classify(&report);
    report
}

/// 读文件并解析（非 UTF-8 字节按 U+FFFD 替换，不报错）。
pub fn parse_file(path: impl AsRef<Path>) -> io::Result<HsErrReport> {
    let path = path.as_ref();
    let bytes = fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut report = parse(&text);
    report.path = Some(path.to_path_buf());
    Ok(report)
}

fn is_header(body: &str) -> bool {
    const MARKERS: [&str; 5] = [
        "insufficient memory",
        "A fatal error has been detected",
        "Out of Memory Error",
        "Internal Error",
        "SIGSEGV",
    ];
    MARKERS.iter().any(|marker| body.contains(marker))
}

fn extract_exception(body: &str) -> Option<String> {
    let marker = "EXCEPTION_";
    let start = body.find(marker)?;
    let rest = &body[start..];
    let end = rest
        .find(" at pc=")
        .or_else(|| rest.find(", pid="))
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_owned())
}

fn extract_signal(body: &str) -> Option<String> {
    let start = body.find("SIG")?;
    let rest = &body[start..];
    let end = rest.find(')').map_or(rest.len(), |index| index + 1);
    Some(rest[..end].trim().to_owned())
}

/// 从 `key=value` 形态里取值（到下一个空格/逗号为止）。
fn field(line: &str, key: &str) -> Option<String> {
    let start = line.find(key)? + key.len();
    let rest = &line[start..];
    let end = rest.find([' ', ',', ']']).unwrap_or(rest.len());
    let value = rest[..end].trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn parse_frame(raw: &str) -> FailedFrame {
    let kind = match raw.chars().next() {
        Some('C') => FrameKind::Native,
        Some('J' | 'j') => FrameKind::Java,
        Some('V') => FrameKind::VmStub,
        _ => FrameKind::Unknown,
    };
    let (module, offset) = raw
        .find('[')
        .and_then(|start| {
            let inner = &raw[start + 1..];
            let inner = &inner[..inner.find(']')?];
            let mut parts = inner.split('+');
            let module = parts.next().map(str::trim).filter(|m| !m.is_empty())?;
            Some((
                Some(module.to_owned()),
                parts.next().map(|o| o.trim().to_owned()),
            ))
        })
        .unwrap_or((None, None));
    FailedFrame {
        raw: raw.to_owned(),
        kind,
        module,
        offset,
    }
}

fn parse_memory(body: &str) -> Option<MemorySummary> {
    let mut summary = MemorySummary {
        raw: body.to_owned(),
        ..MemorySummary::default()
    };
    for part in body.split(',') {
        let part = part.trim();
        if part.contains("page") {
            summary.page_size = part
                .trim_start_matches("Memory:")
                .split_whitespace()
                .next()
                .map(|value| value.trim_end_matches(',').to_owned());
        } else if part.starts_with("physical") {
            let (total, free) = parse_kb_pair(part);
            summary.physical_kb = total;
            summary.physical_free_kb = free;
        } else if part.starts_with("swap") {
            let (total, free) = parse_kb_pair(part);
            summary.swap_kb = total;
            summary.swap_free_kb = free;
        }
    }
    Some(summary)
}

/// `physical 16741480k(4716856k free)` → (16741480, 4716856)。
fn parse_kb_pair(part: &str) -> (Option<u64>, Option<u64>) {
    let numbers = part
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|token| !token.is_empty())
        .filter_map(|token| token.parse::<u64>().ok())
        .collect::<Vec<_>>();
    (numbers.first().copied(), numbers.get(1).copied())
}

/// 取从 `start` 开始、到第一个空行为止的连续行（VM Arguments / java_command 段）。
fn collect_unindented(lines: &[&str], start: usize, limit: usize) -> Vec<String> {
    lines
        .iter()
        .skip(start)
        .map(|line| line.trim_end_matches('\r'))
        .take_while(|line| !line.trim().is_empty())
        .take(limit)
        .map(|line| line.trim().to_owned())
        .collect()
}

/// 取从 `start` 开始、以空白/制表缩进的连续行（最多 `limit` 行）。
fn collect_block(lines: &[&str], start: usize, limit: usize) -> Vec<String> {
    lines
        .iter()
        .skip(start)
        .map(|line| line.trim_end_matches('\r'))
        .skip_while(|line| line.trim().is_empty())
        .take_while(|line| line.starts_with([' ', '\t']))
        .take(limit)
        .map(|line| line.trim().to_owned())
        .collect()
}

/// 文件开头的 `#` 注释块（真实 hs_err 的标题/异常/失败帧都在这里）。
fn collect_header_block(lines: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        let line = line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            if trimmed.starts_with('#') {
                out.push(trimmed.to_owned());
            }
        } else {
            break;
        }
        if out.len() >= 64 {
            break;
        }
    }
    out
}

fn classify(report: &HsErrReport) -> HsErrKind {
    let haystack = [
        report.header.as_deref(),
        report.exception.as_deref(),
        report.siginfo.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n");
    if haystack.contains("EXCEPTION_ACCESS_VIOLATION") || haystack.contains("SIGSEGV") {
        HsErrKind::AccessViolation
    } else if haystack.contains("insufficient memory")
        || haystack.contains("Out of Memory")
        || haystack.contains("OutOfMemory")
    {
        HsErrKind::OutOfMemory
    } else if haystack.contains("StackOverflow") || haystack.contains("stack overflow") {
        HsErrKind::StackOverflow
    } else if haystack.contains("Internal Error")
        || haystack.contains("A fatal error has been detected")
    {
        HsErrKind::InternalError
    } else {
        HsErrKind::Other
    }
}
