//! 崩溃现场扫描：`crash-reports/*.txt`、`hs_err_pid*.log`、`logs/latest.log`。
//!
//! 按 `DevDocs/SubAgentsTalk/launch-接口约定.md` §3：`crash_found` 事件的 `path` 是
//! **权威字段**（M15 按路径自己读盘，`excerpt` 仅供人读）。所以这里只做“找到并给出路径”，
//! 不做解析——解析（hs_err 的失败帧/内存段）是 M15 的 `hs_err` 解析器。
//!
//! 只收**本次会话之后**动过的文件（`since` = 进程启动时刻），避免把上一局的崩溃报告
//! 算到这一局头上（这是 HMCL 会犯的错）。

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::launch::exit_analysis::ExitAnalysis;

/// 崩溃现场文件来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashSource {
    /// `crash-reports/crash-*.txt`（游戏自己写的权威 Description + 栈）。
    CrashReport,
    /// JVM 致命错误 `hs_err_pid*.log`（HMCL 完全不解析的空白区）。
    HsErr,
    /// `logs/latest.log`（内存缓冲不够长时的兜底全文）。
    LatestLog,
}

impl CrashSource {
    /// 事件流里的字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CrashReport => "crash_report",
            Self::HsErr => "hs_err",
            Self::LatestLog => "latest_log",
        }
    }

    /// 这个来源本身能否**作为崩溃的证据**（M211/D3）。
    ///
    /// `crash-reports/*.txt` 与 `hs_err_pid*.log` 只有崩溃时才会出现；`logs/latest.log` 是
    /// **每局都写**的普通日志，它的存在什么也证明不了——把「文件在」当崩溃判据，等于给每一局
    /// 都盖上「游戏崩溃」的章（M208 实机：连 `code 0 / error_lines 0` 的干净退出也被标成崩溃）。
    pub fn is_authoritative(self) -> bool {
        !matches!(self, Self::LatestLog)
    }
}

/// 一个现场文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashArtifact {
    /// 来源类型。
    pub source: CrashSource,
    /// 绝对/相对路径（交给 M15）。
    pub path: PathBuf,
    /// 供人读的开头片段（`latest.log` 不给，太大）。见 `DevDocs/SubAgentsTalk/launch-接口约定.md` §4（写入前脱敏）。
    pub excerpt: Option<String>,
}

/// 默认摘录上限（字符）。
pub const MAX_EXCERPT_CHARS: usize = 2000;

/// 扫描一次运行目录，返回按修改时间倒序的现场文件（最多 `limit` 个）。
///
/// `since` 之前的文件视为上一局遗留，忽略；给 `since` 留 1 秒容差以吸收文件系统时间精度。
///
/// **`logs/latest.log` 只在已经找到权威现场文件时才被带上**（M211/D3）：它是「崩溃报告/
/// hs_err 都缺、只能看全文」时的补充证据，每局都会写，不能当崩溃判据。
pub fn scan(run_dir: &Path, since: SystemTime, limit: usize) -> Vec<CrashArtifact> {
    let since = since.checked_sub(Duration::from_secs(1)).unwrap_or(since);
    let mut found: Vec<(SystemTime, CrashArtifact)> = Vec::new();

    collect_matching(
        &run_dir.join("crash-reports"),
        CrashSource::CrashReport,
        since,
        &mut found,
    );
    collect_matching(run_dir, CrashSource::HsErr, since, &mut found);
    if found.is_empty() {
        return Vec::new();
    }
    if let Some(artifact) = latest_log(run_dir, since) {
        let modified = artifact_time(&artifact.path).unwrap_or(since);
        found.push((modified, artifact));
    }

    found.sort_by_key(|item| std::cmp::Reverse(item.0));
    found
        .into_iter()
        .take(limit)
        .map(|(_, artifact)| artifact)
        .collect()
}

/// 这一局是否**有真实崩溃证据**（M211/D3：索引里的 `crash_found` 只认这个结论）。
///
/// 判据只有两类，都要么是崩溃专属产物、要么是「异常退出 + 日志里真有错误行」：
/// 1. 现场文件里有权威来源（`crash-reports/*.txt` / `hs_err_pid*.log`）；
/// 2. 非 0 退出码且错误级日志行数 > 0。
///
/// 干净退出（`code 0`、`error_lines 0`）落不进任何一条 ⇒ `false`。`logs/latest.log` 存在与否
/// 不参与判定（每次启动都会写它）。
pub fn has_crash_evidence(artifacts: &[CrashArtifact], exit: &ExitAnalysis) -> bool {
    if artifacts.iter().any(|item| item.source.is_authoritative()) {
        return true;
    }
    exit.code != 0 && exit.error_lines > 0
}

fn collect_matching(
    dir: &Path,
    source: CrashSource,
    since: SystemTime,
    out: &mut Vec<(SystemTime, CrashArtifact)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_candidate(&path, source) {
            continue;
        }
        let Some(modified) = artifact_time(&path) else {
            continue;
        };
        if modified < since {
            continue;
        }
        out.push((
            modified,
            CrashArtifact {
                source,
                path: path.clone(),
                excerpt: read_excerpt(&path, MAX_EXCERPT_CHARS),
            },
        ));
    }
}

fn is_candidate(path: &Path, source: CrashSource) -> bool {
    if !path.is_file() {
        return false;
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match source {
        CrashSource::CrashReport => name.ends_with(".txt"),
        CrashSource::HsErr => name.starts_with("hs_err_pid") && name.ends_with(".log"),
        CrashSource::LatestLog => false,
    }
}

fn latest_log(run_dir: &Path, since: SystemTime) -> Option<CrashArtifact> {
    let path = run_dir.join("logs").join("latest.log");
    let modified = artifact_time(&path)?;
    if modified < since {
        return None;
    }
    Some(CrashArtifact {
        source: CrashSource::LatestLog,
        path,
        excerpt: None,
    })
}

fn artifact_time(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// 读文件开头 `max_chars` 个字符（读不到就 `None`，不因权限/编码问题中断收尾）。
pub fn read_excerpt(path: &Path, max_chars: usize) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut excerpt = String::new();
    for (count, ch) in text.chars().enumerate() {
        if count >= max_chars {
            excerpt.push('…');
            break;
        }
        excerpt.push(ch);
    }
    Some(excerpt)
}

#[cfg(test)]
mod tests {
    use super::{has_crash_evidence, scan, CrashSource};
    use crate::launch::exit_analysis::{self, ExitAnalysis, ExitSignals};
    use crate::launch::pump::LogLine;
    use crate::launch::pump::Stream;
    use std::path::{Path, PathBuf};
    use std::time::SystemTime;

    fn temp_dir(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("ull-m211-scan-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建夹具目录");
        root
    }

    /// 每局都会写的普通日志：干净退出时它**也在**，所以「它在」什么也证明不了。
    fn write_latest_log(run_dir: &Path) {
        let logs = run_dir.join("logs");
        std::fs::create_dir_all(&logs).expect("建 logs");
        std::fs::write(logs.join("latest.log"), "[10:15:00] [main/INFO]: hello\n")
            .expect("写 latest.log");
    }

    fn analyze(code: i32, errors: usize) -> ExitAnalysis {
        let lines: Vec<LogLine> = (0..errors)
            .map(|index| {
                LogLine::new(
                    Stream::Stderr,
                    format!("java.lang.RuntimeException: boom {index}"),
                )
            })
            .collect();
        exit_analysis::analyze(&ExitSignals::new(code, 60_000, &lines))
    }

    /// D3 主用例（**改实现之前是红的**）：干净退出 + 只写了 `logs/latest.log` ⇒ 没有现场文件。
    ///
    /// 变异证明：把 `scan` 里 `if found.is_empty() { return Vec::new() }` 删掉（即恢复「
    /// latest.log 无条件当现场文件」）⇒ 本条拿到 1 个 `latest_log`，红。
    #[test]
    fn a_clean_run_produces_no_crash_artifact() {
        let run_dir = temp_dir("clean");
        write_latest_log(&run_dir);

        let artifacts = scan(&run_dir, SystemTime::now(), 8);
        assert!(
            artifacts.is_empty(),
            "只有 latest.log 时不该报现场文件：{artifacts:?}"
        );
        std::fs::remove_dir_all(&run_dir).ok();
    }

    /// 真有崩溃现场时，`latest.log` 才作为补充证据跟着走（它的诊断价值要保住）。
    ///
    /// 变异证明：把 `scan` 里的 `latest_log(...)` 整段删掉 ⇒ 本条只剩 hs_err 一个，红。
    #[test]
    fn latest_log_rides_along_only_when_a_crash_scene_exists() {
        let run_dir = temp_dir("ride");
        write_latest_log(&run_dir);
        std::fs::write(run_dir.join("hs_err_pid4242.log"), "SIGSEGV\n").expect("写 hs_err");

        let artifacts = scan(&run_dir, SystemTime::now(), 8);
        let sources: Vec<CrashSource> = artifacts.iter().map(|item| item.source).collect();
        assert!(
            sources.contains(&CrashSource::HsErr),
            "hs_err 必须被认出来：{sources:?}"
        );
        assert!(
            sources.contains(&CrashSource::LatestLog),
            "有权威现场时 latest.log 仍要作为补充带上：{sources:?}"
        );
        std::fs::remove_dir_all(&run_dir).ok();
    }

    /// 崩溃结论的三条判据：权威现场 / 非 0 退出 + 错误行 / 其余一律不算。
    ///
    /// 变异证明：把 `has_crash_evidence` 最后一行改成 `true`（或改成只看
    /// `!artifacts.is_empty()`）⇒ 干净退出那条断言红。
    #[test]
    fn crash_evidence_needs_something_that_only_happens_on_a_crash() {
        let hs_err = super::CrashArtifact {
            source: CrashSource::HsErr,
            path: PathBuf::from("hs_err_pid1.log"),
            excerpt: None,
        };
        let latest = super::CrashArtifact {
            source: CrashSource::LatestLog,
            path: PathBuf::from("logs/latest.log"),
            excerpt: None,
        };
        let clean = analyze(0, 0);
        let crashed = analyze(1, 2);

        assert!(
            !has_crash_evidence(std::slice::from_ref(&latest), &clean),
            "只有 latest.log + 干净退出 ⇒ 不是崩溃"
        );
        assert!(!has_crash_evidence(&[], &clean), "干净退出 ⇒ 不是崩溃");
        assert!(
            !has_crash_evidence(&[], &analyze(1, 0)),
            "非 0 退出但没有错误行 ⇒ 证据不足，不算崩溃"
        );
        assert!(has_crash_evidence(&[hs_err], &clean), "hs_err 就是崩溃证据");
        assert!(
            has_crash_evidence(&[], &crashed),
            "非 0 退出 + 错误行 ⇒ 崩溃"
        );
    }
}
