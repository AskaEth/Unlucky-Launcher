//! 传输细节：单连接（含续传）+ 多连接分块，产出「已校验的完整中间文件」。
//!
//! 本文件只管「一个候选源的一次尝试」——换源、退避、限流在 `engine.rs`。

use std::time::Instant;

use tokio::io::AsyncWriteExt;

use super::atomic::PartFile;
use super::cache::now_ms;
use super::chunked;
use super::engine::{AttemptOk, DownloadEngine};
use super::error::{DownloadError, DownloadErrorKind};
use super::http;
use super::progress::{ProgressEvent, ProgressThrottle};
use super::resume::{self, RestartReason, ResumeDecision, ResumeMeta};
use super::types::{Candidate, DownloadId, DownloadRequest, HashAlgo};
use super::verify;

/// 续传起点（无续传时全为默认值）。
struct ResumeCut {
    /// 已下字节数。
    offset: u64,
    /// 续传起点（事件字段）。
    resumed_from: u64,
    /// `If-Range` 的取值。
    if_range: Option<String>,
}

impl ResumeCut {
    /// 从头下载。
    const fn none() -> Self {
        Self {
            offset: 0,
            resumed_from: 0,
            if_range: None,
        }
    }
}

impl DownloadEngine {
    /// 一次尝试：续传判定 → 分块 or 单连接 → 流式校验。
    pub(crate) async fn attempt(
        &self,
        id: DownloadId,
        req: &DownloadRequest,
        candidate: &Candidate,
        part: &mut PartFile,
    ) -> Result<AttemptOk, DownloadError> {
        let integrity = req.integrity.as_ref();
        let meta_path = part.meta_path();
        let previous = ResumeMeta::load(&meta_path);
        // 「没有中间文件」和「有但 0 字节」是两种不同的原因，分开报出来便于诊断。
        let decision = if !part.exists() {
            ResumeDecision::Restart {
                reason: RestartReason::NoPart,
            }
        } else if part.is_empty() {
            ResumeDecision::Restart {
                reason: RestartReason::EmptyPart,
            }
        } else {
            resume::decide(
                req.resume,
                part.len(),
                previous.as_ref(),
                &candidate.url,
                integrity.map(|item| (item.algo, item.hex.as_str())),
                req.expected_size,
            )
        };

        let mut cut = ResumeCut::none();
        match decision {
            ResumeDecision::Complete { size } => {
                self.verify_part(part, integrity, id)?;
                return Ok(AttemptOk {
                    bytes: size,
                    resumed_from: size,
                });
            }
            ResumeDecision::Resume { offset, validator } => {
                if let Some(value) = validator.if_range_value() {
                    cut = ResumeCut {
                        offset,
                        resumed_from: offset,
                        if_range: Some(value.to_string()),
                    };
                }
            }
            ResumeDecision::Restart { .. } => {}
        }

        if cut.offset == 0 && self.cfg.chunked.applies_to(req.kind, req.expected_size) {
            match self
                .attempt_chunked(id, req, candidate, part, req.expected_size)
                .await
            {
                Ok(ok) => return Ok(ok),
                // Range 不支持/切不出足够段：静默退化为单连接（不算源失败）。
                Err(err) if err.kind == DownloadErrorKind::RangeUnsupported => {}
                Err(err) => return Err(err),
            }
        }

        self.attempt_single(id, req, candidate, part, cut).await
    }

    async fn attempt_single(
        &self,
        id: DownloadId,
        req: &DownloadRequest,
        candidate: &Candidate,
        part: &mut PartFile,
        cut: ResumeCut,
    ) -> Result<AttemptOk, DownloadError> {
        let integrity = req.integrity.as_ref();
        let mut offset = cut.offset;
        let mut resumed_from = cut.resumed_from;
        let mut request = self.http.get(&candidate.url);
        if offset > 0 {
            match cut.if_range.as_deref() {
                Some(value) => {
                    request = request
                        .header(reqwest::header::RANGE, format!("bytes={offset}-"))
                        .header(reqwest::header::IF_RANGE, value);
                }
                // 没有校验器就不能保证远端内容没变，直接从头下。
                None => {
                    offset = 0;
                    resumed_from = 0;
                }
            }
        }

        let response = request.send().await.map_err(|err| {
            http::classify_reqwest(&err, &candidate.url).with_source(candidate.source.as_str())
        })?;
        let status = response.status();

        if status.as_u16() == 416 {
            return match resume::on_range_unsatisfiable(part.len(), req.expected_size) {
                ResumeDecision::Complete { size } => {
                    self.verify_part(part, integrity, id)?;
                    Ok(AttemptOk {
                        bytes: size,
                        resumed_from: size,
                    })
                }
                _ => {
                    part.discard();
                    Err(DownloadError::new(
                        DownloadErrorKind::RangeUnsupported,
                        "416：丢弃中间文件重下",
                    )
                    .with_source(candidate.source.as_str()))
                }
            };
        }
        if !status.is_success() {
            return Err(http::classify_status(status.as_u16(), &candidate.url)
                .with_source(candidate.source.as_str()));
        }

        // 2xx 不等于「拿到归档」：镜像门户对任意路径都回 200 + HTML（华为云实测）。
        // 归到 NotFound（换源，不算源坏不进冷却），否则假阳性源会一直挡在队首。
        if let Some(reason) =
            http::html_mirror_reason(response.headers(), req.expected_size, offset)
        {
            return Err(DownloadError::new(DownloadErrorKind::NotFound, reason)
                .with_source(candidate.source.as_str()));
        }

        // 服务器无视了 Range（200 而非 206）→ 从头写。
        if offset > 0 && status.as_u16() != 206 {
            offset = 0;
            resumed_from = 0;
        }

        let content_range = http::header_str(response.headers(), reqwest::header::CONTENT_RANGE)
            .and_then(chunked::parse_content_range);
        let etag = http::etag(response.headers());
        let last_modified = http::last_modified(response.headers());
        let content_length = http::content_length(response.headers());

        let mut total = req.expected_size;
        if let Some(content_range) = content_range {
            if content_range.start != offset {
                // 服务器给的起点和我们以为的不一样：偏移不可信，从头来。
                offset = 0;
                resumed_from = 0;
            }
            total = content_range.total.or(total);
        }
        total = total.or_else(|| content_length.map(|len| len + offset));

        // 记下续传元数据（跨进程续传的凭据）。
        let meta = ResumeMeta {
            url: candidate.url.clone(),
            etag,
            last_modified,
            total,
            algo: integrity.map_or(HashAlgo::Sha1, |item| item.algo),
            hash: integrity.map_or_else(String::new, |item| item.hex.clone()),
            saved_at_ms: now_ms(),
        };
        let _ = meta.save(&part.meta_path());

        let std_file = if offset > 0 {
            part.open_append()
        } else {
            part.open_truncate()
        }
        .map_err(|err| io_err("打开中间文件失败", &err))?;
        let mut file = tokio::fs::File::from_std(std_file);

        let mut hasher = integrity.map(|item| verify::Hasher::new(item.algo));
        if offset > 0 {
            if let Some(hasher) = hasher.as_mut() {
                verify::update_hasher_from_file(hasher, part.path(), offset)
                    .map_err(|err| io_err("读取续传前缀失败", &err))?;
            }
        }

        let mut throttle = ProgressThrottle::new(self.cfg.progress_throttle);
        let mut written = offset;
        let mut stream = response.bytes_stream();
        loop {
            let next = tokio::time::timeout(
                self.cfg.read_timeout,
                futures_util::StreamExt::next(&mut stream),
            )
            .await
            .map_err(|_| {
                DownloadError::new(
                    DownloadErrorKind::Timeout,
                    format!("读取超时（{}）", candidate.url),
                )
                .with_source(candidate.source.as_str())
            })?;
            let Some(chunk) = next else { break };
            let chunk = chunk.map_err(|err| http::classify_reqwest(&err, &candidate.url))?;
            if let Some(hasher) = hasher.as_mut() {
                hasher.update(&chunk);
            }
            file.write_all(&chunk)
                .await
                .map_err(|err| io_err("写入中间文件失败", &err))?;
            written += chunk.len() as u64;
            if throttle.should_emit(Instant::now(), false) {
                self.emit(ProgressEvent::Progress {
                    id,
                    downloaded: written,
                    total_bytes: total,
                });
            }
        }
        file.flush()
            .await
            .map_err(|err| io_err("刷新中间文件失败", &err))?;
        // Windows 上打开中的文件不能被 rename，必须先关。
        drop(file);

        if let Some(total) = total {
            if written != total {
                return Err(DownloadError::new(
                    DownloadErrorKind::Network,
                    format!("长度不符：期望 {total} 实得 {written}（{}）", candidate.url),
                )
                .with_source(candidate.source.as_str()));
            }
        }

        if let (Some(integrity), Some(hasher)) = (integrity, hasher) {
            self.emit(ProgressEvent::Verifying {
                id,
                algo: integrity.algo,
                expected_prefix: integrity.short_prefix().to_string(),
            });
            let actual = hasher.finish();
            if !verify::matches(&integrity.hex, &actual) {
                part.discard();
                return Err(DownloadError::new(
                    DownloadErrorKind::Integrity,
                    format!("校验不符：期望 {} 实得 {actual}", integrity.hex),
                )
                .with_source(candidate.source.as_str()));
            }
        }

        self.emit(ProgressEvent::Progress {
            id,
            downloaded: written,
            total_bytes: total,
        });
        Ok(AttemptOk {
            bytes: written,
            resumed_from,
        })
    }
}

fn io_err(context: &str, err: &std::io::Error) -> DownloadError {
    DownloadError::new(DownloadErrorKind::Io, format!("{context}: {err}"))
}
