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
