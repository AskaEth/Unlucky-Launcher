//! 五类信号的具体采集器（推送类 2 个 + 磁盘类 3 个）。
//!
//! 由 `collectors.rs` 用 `#[path]` 声明并统一导出；拆分只为守单文件 400 行上限。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::diagnose::hs_err;

use super::{
    CapturedText, CollectError, CollectedSources, CollectionContext, EnvironmentSnapshot,
    ExitSignal, SignalBundle, SignalCollector, SignalKind, SignalOrigin,
};

/// 推送类：退出信号（launch 的 `exit` 事件已算好）。
#[derive(Debug, Clone)]
pub struct ExitCollector {
    signal: ExitSignal,
}

impl ExitCollector {
    /// 构造。
    #[must_use]
    pub fn new(signal: ExitSignal) -> Self {
        Self { signal }
    }
}

impl SignalCollector for ExitCollector {
    fn kind(&self) -> SignalKind {
        SignalKind::Exit
    }

    fn collect(
        &self,
        _ctx: &CollectionContext,
        out: &mut SignalBundle,
    ) -> Result<CollectedSources, CollectError> {
        out.exit = Some(self.signal.clone());
        Ok(CollectedSources::new())
    }
}

/// 推送类：环境快照 + mod 清单（launch/game 侧已有数据）。
#[derive(Debug, Clone, Default)]
pub struct EnvironmentCollector {
    snapshot: EnvironmentSnapshot,
}

impl EnvironmentCollector {
    /// 构造。
    #[must_use]
    pub fn new(snapshot: EnvironmentSnapshot) -> Self {
        Self { snapshot }
    }
}

impl SignalCollector for EnvironmentCollector {
    fn kind(&self) -> SignalKind {
        SignalKind::Environment
    }

    fn collect(
        &self,
        _ctx: &CollectionContext,
        out: &mut SignalBundle,
    ) -> Result<CollectedSources, CollectError> {
        out.mods = self.snapshot.mods.clone();
        out.environment = Some(self.snapshot.clone());
        Ok(CollectedSources::new())
    }
}

/// 磁盘类：`crash-reports/crash-*.txt`（双通道里的「权威全文」）。
#[derive(Debug, Clone, Default)]
pub struct CrashReportCollector {
    dir: Option<PathBuf>,
}

impl CrashReportCollector {
    /// 默认扫 `<run_dir>/crash-reports`。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 指定目录。
    #[must_use]
    pub fn with_dir(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: Some(dir.into()),
        }
    }
}

impl SignalCollector for CrashReportCollector {
    fn kind(&self) -> SignalKind {
        SignalKind::CrashReport
    }

    fn collect(
        &self,
        ctx: &CollectionContext,
        out: &mut SignalBundle,
    ) -> Result<CollectedSources, CollectError> {
        let dir = self
            .dir
            .clone()
            .unwrap_or_else(|| ctx.run_dir.join("crash-reports"));
        let Some(path) = newest_file(&dir, ctx.started_at, |name| {
            name.starts_with("crash-") && name.ends_with(".txt")
        }) else {
            return Ok(CollectedSources::new());
        };
        let text = read_lossy(SignalKind::CrashReport, &path)?;
        let modified = modified_at(&path);
        let mut sources = CollectedSources::new();
        sources.push(&path);
        out.sources.push(path.clone());
        out.crash_report = Some(CapturedText {
            path,
            text,
            modified,
        });
        Ok(sources)
    }
}

/// 磁盘类：`hs_err_pid*.log`（HMCL 的空白区，显卡驱动类崩溃的唯一权威证据）。
#[derive(Debug, Clone, Default)]
pub struct HsErrCollector {
    dirs: Vec<PathBuf>,
}

impl HsErrCollector {
    /// 默认扫 `<run_dir>`（JVM 把 hs_err 写在进程工作目录）。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 指定候选目录表（**整表替换**，不是追加；给出后不再自动扫 `run_dir`）。
    ///
    /// 典型用法：`HsErrCollector::with_dirs([run_dir.clone(), home_dir])`。
    #[must_use]
    pub fn with_dirs(dirs: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            dirs: dirs.into_iter().collect(),
        }
    }
}

impl SignalCollector for HsErrCollector {
    fn kind(&self) -> SignalKind {
        SignalKind::HsErr
    }

    fn collect(
        &self,
        ctx: &CollectionContext,
        out: &mut SignalBundle,
    ) -> Result<CollectedSources, CollectError> {
        let dirs = if self.dirs.is_empty() {
            vec![ctx.run_dir.clone()]
        } else {
            self.dirs.clone()
        };
        let mut newest: Option<PathBuf> = None;
        for dir in dirs {
            if let Some(path) = newest_file(&dir, ctx.started_at, |name| {
                name.starts_with("hs_err_pid") && name.ends_with(".log")
            }) {
                newest = match newest {
                    Some(current) => match (modified_at(&current), modified_at(&path)) {
                        (Some(a), Some(b)) if b <= a => Some(current),
                        _ => Some(path),
                    },
                    None => Some(path),
                };
            }
        }
        let Some(path) = newest else {
            return Ok(CollectedSources::new());
        };
        let report = hs_err::parse_file(&path).map_err(|error| CollectError {
            kind: SignalKind::HsErr,
            path: Some(path.clone()),
            message: error.to_string(),
        })?;
        let mut sources = CollectedSources::new();
        sources.push(&path);
        out.sources.push(path);
        out.hs_err = Some(report);
        Ok(sources)
    }
}

/// 磁盘类：`logs/latest.log` 尾巴（内存缓冲不够长时的兜底）。
#[derive(Debug, Clone)]
pub struct LatestLogCollector {
    path: Option<PathBuf>,
    max_lines: usize,
}

impl Default for LatestLogCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl LatestLogCollector {
    /// 默认读 `<run_dir>/logs/latest.log` 的最后 5000 行。
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: None,
            max_lines: 5000,
        }
    }

    /// 保留行数上限。
    #[must_use]
    pub fn with_max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = max_lines.max(1);
        self
    }

    /// 指定文件。
    #[must_use]
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
            max_lines: 5000,
        }
    }
}

impl SignalCollector for LatestLogCollector {
    fn kind(&self) -> SignalKind {
        SignalKind::Log
    }

    fn collect(
        &self,
        ctx: &CollectionContext,
        out: &mut SignalBundle,
    ) -> Result<CollectedSources, CollectError> {
        let path = self
            .path
            .clone()
            .unwrap_or_else(|| ctx.run_dir.join("logs").join("latest.log"));
        if !path.is_file() {
            return Ok(CollectedSources::new());
        }
        let text = read_lossy(SignalKind::Log, &path)?;
        let lines = text.lines().collect::<Vec<_>>();
        let start = lines.len().saturating_sub(self.max_lines);
        out.log
            .extend(SignalOrigin::LatestLog, lines[start..].iter().copied());
        let mut sources = CollectedSources::new();
        sources.push(&path);
        out.sources.push(path);
        Ok(sources)
    }
}

fn newest_file(
    dir: &Path,
    since: Option<SystemTime>,
    matches: impl Fn(&str) -> bool,
) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !matches(&name) {
                return None;
            }
            let path = entry.path();
            let modified = modified_at(&path)?;
            if since.is_some_and(|since| modified < since) {
                return None;
            }
            Some((modified, path))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, path)| path)
}

fn modified_at(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

fn read_lossy(kind: SignalKind, path: &Path) -> Result<String, CollectError> {
    let bytes = fs::read(path).map_err(|error| CollectError::io(kind, path, &error))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
