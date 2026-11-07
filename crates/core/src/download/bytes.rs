//! 内存下载：版本清单 / 版本 JSON / 资源索引这类「小文本」资源。
//!
//! 与文件下载共用候选源、退避与健康度逻辑，但不续传、不分块——它们要么很小，
//! 要么强时效（`ETag` 语义比续传更合适）。

use std::time::Instant;

use super::cache::now_ms;
use super::engine::{millis, DownloadEngine};
use super::error::{DownloadError, DownloadErrorKind};
use super::http;
use super::progress::ProgressEvent;
use super::types::{Candidate, DownloadId, DownloadRequest};
use super::verify;

/// 内存下载的上限（manifest 类资源不该有几十 MB；防御恶意镜像把人撑爆）。
pub const MAX_IN_MEMORY_BYTES: u64 = 64 * 1024 * 1024;

impl DownloadEngine {
    /// 内存下载（版本清单 / 版本 JSON / 索引等小文本资源）。
    pub async fn download_bytes(&self, req: DownloadRequest) -> Result<Vec<u8>, DownloadError> {
        let started = Instant::now();
        let id = DownloadId::next();
        req.validate()?;
        let plan = self.plan(&req, now_ms());
        self.announce(&id, &req, plan.len());
        if plan.is_empty() {
            return Err(DownloadError::new(
                DownloadErrorKind::InvalidRequest,
                format!("没有可用候选源：{}", req.url),
            ));
        }

        let _permit = self.acquire().await?;
        let mut suppressed: Vec<String> = Vec::new();

        for candidate in &plan {
            // 与文件下载同一套规则：冷却中/刚出冷却的源只给一次探测机会。
            let max_attempts = self.attempts_for(&candidate.source, req.kind, now_ms());
            for attempt in 0..max_attempts {
                match self.fetch_bytes(&req, candidate).await {
                    Ok(bytes) => {
                        self.health.record_success(
                            &candidate.source,
                            req.kind,
                            started.elapsed(),
                            now_ms(),
                        );
                        self.emit(ProgressEvent::Completed {
                            id,
                            bytes: bytes.len() as u64,
                            from_cache: false,
                            source: candidate.source.clone(),
                            elapsed_ms: millis(started.elapsed()),
                        });
                        self.flush_health();
                        return Ok(bytes);
                    }
                    Err(err) => {
                        self.health.record_failure(
                            &candidate.source,
                            req.kind,
                            &err.message,
                            now_ms(),
                        );
                        let retry = err.kind.is_retryable() && attempt + 1 < max_attempts;
                        let delay = self.next_delay(attempt);
                        self.emit(ProgressEvent::SourceFailed {
                            id,
                            source: candidate.source.clone(),
                            reason: err.message.clone(),
                            attempt,
                            will_retry: retry,
                            retry_in_ms: if retry { millis(delay) } else { 0 },
                        });
                        if !retry {
                            suppressed.push(format!("{}: {}", candidate.source, err.message));
                            break;
                        }
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        self.flush_health();
        Err(DownloadError::new(
            DownloadErrorKind::AllSourcesFailed,
            format!("所有候选源都取不到文本资源：{}", req.url),
        )
        .with_suppressed(suppressed))
    }

    async fn fetch_bytes(
        &self,
        req: &DownloadRequest,
        candidate: &Candidate,
    ) -> Result<Vec<u8>, DownloadError> {
        let response = self
            .http
            .get(&candidate.url)
            .send()
            .await
            .map_err(|err| http::classify_reqwest(&err, &candidate.url))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http::classify_status(status.as_u16(), &candidate.url)
                .with_source(candidate.source.as_str()));
        }
        if let Some(len) = http::content_length(response.headers()) {
            if len > MAX_IN_MEMORY_BYTES {
                return Err(DownloadError::new(
                    DownloadErrorKind::InvalidRequest,
                    format!(
                        "文本资源过大（{len} 字节），拒绝载入内存：{}",
                        candidate.url
                    ),
                )
                .with_source(candidate.source.as_str()));
            }
        }
        let body = response
            .bytes()
            .await
            .map_err(|err| http::classify_reqwest(&err, &candidate.url))?;
        if let Some(integrity) = &req.integrity {
            let actual = verify::digest_bytes(integrity.algo, &body);
            if !verify::matches(&integrity.hex, &actual) {
                return Err(DownloadError::new(
                    DownloadErrorKind::Integrity,
                    format!("校验不符：期望 {} 实得 {actual}", integrity.hex),
                )
                .with_source(candidate.source.as_str()));
            }
        }
        Ok(body.to_vec())
    }
}
