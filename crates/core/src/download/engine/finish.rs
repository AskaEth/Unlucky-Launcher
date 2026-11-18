//! 一次尝试成功后的收尾：缓存写入 → 原子落盘 → 完成事件；外加两个小工具。
//!
//! 从 `engine.rs` 拆出只为守 400 行硬线（M289 r1 复审又给 engine 加了许可上界与
//! 预算耗尽短路，贴线了）。搬出来的代码逐字节未改，只有可见性从私有放宽到
//! `pub(super)` / `pub(crate)`（跨模块调用需要）。

use std::path::Path;
use std::time::{Duration, Instant};

use super::super::atomic::PartFile;
use super::super::cache::now_ms;
use super::super::error::{DownloadError, DownloadErrorKind};
use super::super::progress::ProgressEvent;
use super::super::types::{Candidate, DownloadId, DownloadOutcome, DownloadRequest};
use super::{AttemptOk, DownloadEngine};

impl DownloadEngine {
    pub(super) fn finish(
        &self,
        req: &DownloadRequest,
        part: PartFile,
        id: DownloadId,
        candidate: &Candidate,
        ok: AttemptOk,
        started: Instant,
    ) -> Result<DownloadOutcome, DownloadError> {
        if let Some(integrity) = &req.integrity {
            // 缓存写入失败不影响交付（缓存纯属加速）。
            let _ = self.cache.insert(part.path(), integrity, now_ms());
            let _ = self.cache.persist();
        }
        part.commit(&req.dest).map_err(|err| {
            DownloadError::new(
                DownloadErrorKind::Io,
                format!("原子落盘失败（{}）：{err}", req.dest.display()),
            )
        })?;
        let elapsed = started.elapsed();
        self.emit(ProgressEvent::Completed {
            id,
            bytes: ok.bytes,
            from_cache: false,
            source: candidate.source.clone(),
            elapsed_ms: millis(elapsed),
        });
        Ok(DownloadOutcome {
            id,
            path: req.dest.clone(),
            bytes: ok.bytes,
            from_cache: false,
            source: candidate.source.clone(),
            resumed_from: ok.resumed_from,
            elapsed,
        })
    }
}

/// 缓存命中时把文件变成工作副本（硬链接优先，跨卷退化为复制）。
pub(super) fn materialize(source: &Path, dest: &Path) -> std::io::Result<u64> {
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    if dest.exists() {
        let _ = std::fs::remove_file(dest);
    }
    if std::fs::hard_link(source, dest).is_err() {
        std::fs::copy(source, dest)?;
    }
    Ok(std::fs::metadata(dest)?.len())
}

pub(crate) fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
