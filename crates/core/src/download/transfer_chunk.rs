//! 大文件多连接分块的编排与校验。
//!
//! 分块本身由 `chunked.rs` 负责（纯规划 + 单块抓取），这里负责并发、进度上报、
//! 失败清理与「整文件重算一次摘要」（分块是乱序写，只能整文件校验）。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::atomic::PartFile;
use super::chunked;
use super::engine::{AttemptOk, DownloadEngine};
use super::error::{DownloadError, DownloadErrorKind};
use super::progress::ProgressEvent;
use super::types::{Candidate, DownloadId, DownloadRequest, Integrity};
use super::verify;

impl DownloadEngine {
    pub(crate) async fn attempt_chunked(
        &self,
        id: DownloadId,
        req: &DownloadRequest,
        candidate: &Candidate,
        part: &mut PartFile,
        total: Option<u64>,
    ) -> Result<AttemptOk, DownloadError> {
        let Some(total) = total.filter(|total| *total > 0) else {
            return Err(DownloadError::new(
                DownloadErrorKind::RangeUnsupported,
                "总长未知，无法分块",
            ));
        };
        let ranges = chunked::plan(total, self.cfg.chunked.effective_connections());
        if ranges.len() < 2 {
            return Err(DownloadError::new(
                DownloadErrorKind::RangeUnsupported,
                "可切分段不足，退化为单连接",
            ));
        }

        part.discard();
        part.ensure_parent_async()
            .await
            .map_err(|err| io_err("创建下载目录失败", &err))?;
        {
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(part.path())
                .await
                .map_err(|err| io_err("预分配中间文件失败", &err))?;
            file.set_len(total)
                .await
                .map_err(|err| io_err("预分配长度失败", &err))?;
        }

        let counter = Arc::new(AtomicU64::new(0));
        let reporter = self.spawn_reporter(id, Arc::clone(&counter), total);

        let mut set = tokio::task::JoinSet::new();
        for range in ranges {
            let client = self.http.clone();
            let url = candidate.url.clone();
            let path = part.path().to_path_buf();
            let read_timeout = self.cfg.read_timeout;
            let progress = Arc::clone(&counter);
            set.spawn(async move {
                chunked::fetch_chunk(&client, &url, range, &path, read_timeout, &progress).await
            });
        }

        let mut failure: Option<DownloadError> = None;
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok(_)) => {}
                Ok(Err(err)) => {
                    failure = Some(err.with_source(candidate.source.as_str()));
                    set.abort_all();
                    break;
                }
                Err(err) => {
                    failure = Some(DownloadError::new(
                        DownloadErrorKind::Network,
                        format!("分块任务异常：{err}"),
                    ));
                    set.abort_all();
                    break;
                }
            }
        }
        reporter.abort();

        if let Some(err) = failure {
            part.discard();
            return Err(err);
        }

        // 分块是乱序写，只能整文件重算一次摘要。
        self.verify_part(part, req.integrity.as_ref(), id)?;
        self.emit(ProgressEvent::Progress {
            id,
            downloaded: total,
            total_bytes: Some(total),
        });
        Ok(AttemptOk {
            bytes: total,
            resumed_from: 0,
        })
    }

    fn spawn_reporter(
        &self,
        id: DownloadId,
        counter: Arc<AtomicU64>,
        total: u64,
    ) -> tokio::task::JoinHandle<()> {
        let emitter = self.progress.clone();
        let interval = self.cfg.progress_throttle.max(Duration::from_millis(50));
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let downloaded = counter.load(Ordering::Relaxed).min(total);
                emitter.emit(ProgressEvent::Progress {
                    id,
                    downloaded,
                    total_bytes: Some(total),
                });
            }
        })
    }

    pub(crate) fn verify_part(
        &self,
        part: &mut PartFile,
        integrity: Option<&Integrity>,
        id: DownloadId,
    ) -> Result<(), DownloadError> {
        let Some(integrity) = integrity else {
            return Ok(());
        };
        self.emit(ProgressEvent::Verifying {
            id,
            algo: integrity.algo,
            expected_prefix: integrity.short_prefix().to_string(),
        });
        let actual = verify::digest_file(part.path(), integrity.algo)
            .map_err(|err| io_err("读取中间文件失败", &err))?;
        if verify::matches(&integrity.hex, &actual) {
            return Ok(());
        }
        part.discard();
        Err(DownloadError::new(
            DownloadErrorKind::Integrity,
            format!("校验不符：期望 {} 实得 {actual}", integrity.hex),
        ))
    }
}

fn io_err(context: &str, err: &std::io::Error) -> DownloadError {
    DownloadError::new(DownloadErrorKind::Io, format!("{context}: {err}"))
}
