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
use super::stall::StallReport;
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

        let _permit = self.acquire(req.expected_size).await?;
        let mut suppressed: Vec<String> = Vec::new();
        // 逐源的叶子分类（出去时汇总成一条，见 `DownloadError::leaf`）。
        let mut kinds: Vec<DownloadErrorKind> = Vec::new();
        // 与 `download()` 一致：最后一次尝试的失速现场带出去（含阶段与三个数）。
        let mut last_stall: Option<StallReport> = None;

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
                        last_stall = err.stall.as_deref().copied();
                        self.health.record_failure(
                            &candidate.source,
                            req.kind,
                            &err.message,
                            now_ms(),
                        );
                        let retry = err.is_retryable() && attempt + 1 < max_attempts;
                        let delay = self.next_delay(attempt);
                        self.emit(ProgressEvent::SourceFailed {
                            id,
                            source: candidate.source.clone(),
                            kind: err.kind,
                            reason: err.message.clone(),
                            attempt,
                            will_retry: retry,
                            retry_in_ms: if retry { millis(delay) } else { 0 },
                        });
                        if !retry {
                            kinds.push(err.kind);
                            suppressed.push(format!("{}: {}", candidate.source, err.message));
                            break;
                        }
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        self.flush_health();
        // 叶子成因要跟着出去：上层（在线内容源的 `Transport`）靠它区分「这个 URL 不存在」
        // 与「没联网」，只留 `AllSourcesFailed` 的话那套分类在生产路径上永远不可达。
        // 三个后缀收在 [`DownloadError::all_sources_failed`] 一处（M318 就是在这里漏了叶子）。
        let detail = match &last_stall {
            Some(stall) => format!("（最后一次失速：{stall}）"),
            None => String::new(),
        };
        Err(DownloadError::all_sources_failed(
            format!("所有候选源都取不到文本资源：{}{detail}", req.url),
            suppressed,
            kinds,
            last_stall,
        ))
    }

    async fn fetch_bytes(
        &self,
        req: &DownloadRequest,
        candidate: &Candidate,
    ) -> Result<Vec<u8>, DownloadError> {
        // 两处 await 都有界（复审 r1 p2-1）：等响应头走 `send_bounded`（与文件下载同一构造点），
        // 整包读改成流式累加 + `read_step` 的空闲判定。不能再退回 `response.bytes()`：
        // 它是一次无界的整包读，`MAX_IN_MEMORY_BYTES` 只管大小不管时间。
        let deadlines = self.cfg.deadlines();
        let response = http::send_bounded(
            self.http.get(&candidate.url),
            &candidate.url,
            deadlines,
            req.expected_size,
        )
        .await
        .map_err(|err| err.with_source(candidate.source.as_str()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http::classify_status(status.as_u16(), &candidate.url)
                .with_source(candidate.source.as_str()));
        }
        let declared = http::content_length(response.headers());
        if let Some(len) = declared {
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

        let mut stream = response.bytes_stream();
        let mut body: Vec<u8> = Vec::with_capacity(
            declared
                .and_then(|len| usize::try_from(len).ok())
                .unwrap_or(0),
        );
        loop {
            let next = http::read_step(
                futures_util::StreamExt::next(&mut stream),
                deadlines.read_idle,
                body.len() as u64,
                declared,
                &candidate.url,
            )
            .await
            .map_err(|err| err.with_source(candidate.source.as_str()))?;
            let Some(chunk) = next else { break };
            let chunk = chunk.map_err(|err| {
                http::classify_reqwest(&err, &candidate.url).with_source(candidate.source.as_str())
            })?;
            body.extend_from_slice(&chunk);
            // `Content-Length` 可以撒谎、也可以缺失（靠关连接分帧）：边读边卡，不能只看头。
            if body.len() as u64 > MAX_IN_MEMORY_BYTES {
                return Err(DownloadError::new(
                    DownloadErrorKind::InvalidRequest,
                    format!(
                        "文本资源超过内存上限（{} 字节），拒绝继续载入：{}",
                        body.len(),
                        candidate.url
                    ),
                )
                .with_source(candidate.source.as_str()));
            }
        }

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
        Ok(body)
    }
}
