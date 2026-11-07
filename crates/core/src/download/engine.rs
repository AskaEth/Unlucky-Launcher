//! 下载引擎：编排层。
//!
//! 一次 `download()` 的固定剧情：
//!
//! ```text
//! 校验请求 → 内容寻址缓存直取（零网络）
//!          → 候选规划（镜像表展开 + 健康度排序）
//!          → Semaphore 限流
//!          → 候选源外循环（换源），每源内循环（指数退避 + 抖动）
//!              → 续传判定（保守）→ 单连接 / 多连接分块 → 流式校验 → 原子落盘
//!          → 失败则保留中间文件供下次续传
//! ```
//!
//! 最后一步保留的是**本会话命名的**中间文件（`<name>.ullpart-<session>-<seq>`，不再固定
//! 叫 `<name>.ullpart`）：下次（甚至下次启动、重启后再下同一个文件）由 `PartFile::for_dest`
//! 在目标目录里认领它并改名成新的会话名，续传判定照旧。详细身份规则见 `atomic.rs` 模块头的
//! 「中间文件的身份」。
//!
//! 具体传输细节在 `transfer.rs`，错误分类在 `http.rs`，本文件只负责顺序与策略。

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

use super::atomic::PartFile;
use super::backoff::Jitter;
use super::cache::{now_ms, ContentCache};
use super::config::EngineConfig;
use super::error::{DownloadError, DownloadErrorKind};
use super::health::HealthRegistry;
use super::http;
use super::mirrors::MirrorTable;
use super::progress::{ProgressEmitter, ProgressEvent};
use super::region::RegionProfile;
use super::types::{
    Candidate, DownloadId, DownloadOutcome, DownloadRequest, ResourceKind, ResumePolicy,
    SOURCE_OFFICIAL,
};

/// 一次成功尝试的产物描述。
pub(crate) struct AttemptOk {
    /// 文件总字节数。
    pub bytes: u64,
    /// 续传起点（0 = 从头下）。
    pub resumed_from: u64,
}

/// 下载引擎（`Arc` 后全局共享；内部无状态以外的可变数据只有抖动源）。
#[derive(Debug)]
pub struct DownloadEngine {
    pub(crate) http: reqwest::Client,
    pub(crate) cfg: EngineConfig,
    pub(crate) progress: ProgressEmitter,
    pub(crate) health: Arc<HealthRegistry>,
    pub(crate) cache: Arc<ContentCache>,
    pub(crate) jitter: std::sync::Mutex<Jitter>,
    mirrors: Arc<MirrorTable>,
    region: Arc<RegionProfile>,
    sem: Arc<Semaphore>,
}

impl DownloadEngine {
    /// 构造引擎（用 `with_progress` 挂事件流）。
    #[must_use]
    pub fn new(
        cfg: EngineConfig,
        mirrors: Arc<MirrorTable>,
        region: Arc<RegionProfile>,
        health: Arc<HealthRegistry>,
        cache: Arc<ContentCache>,
    ) -> Self {
        let http = http::build_client(&cfg).unwrap_or_else(|_| reqwest::Client::new());
        let sem = Arc::new(Semaphore::new(cfg.max_concurrency.max(1)));
        let seed = now_ms() ^ (u64::from(cfg.max_concurrency as u32) << 32);
        Self {
            http,
            cfg,
            progress: ProgressEmitter::silent(),
            health,
            cache,
            jitter: std::sync::Mutex::new(Jitter::new(seed)),
            mirrors,
            region,
            sem,
        }
    }

    /// 挂上进度事件发射器。
    #[must_use]
    pub fn with_progress(mut self, progress: ProgressEmitter) -> Self {
        self.progress = progress;
        self
    }

    /// 配置。
    #[must_use]
    pub fn config(&self) -> &EngineConfig {
        &self.cfg
    }

    /// HTTP 客户端（同一条连接池与超时配置）。
    ///
    /// 给「一锤子查询」用——例如 Adoptium 的版本查询（`core::game::java::api`）：
    /// 没必要为了一个 GET 再造一个 client，代理/超时/证书栈都在这里统一。
    #[must_use]
    pub fn client(&self) -> &reqwest::Client {
        &self.http
    }

    /// 当前地区档案（设置页展示）。
    #[must_use]
    pub fn region(&self) -> &RegionProfile {
        &self.region
    }

    /// 镜像表（`game::java` 的 Oracle / MS / Zulu 分发表在这里查）。
    #[must_use]
    pub fn mirrors(&self) -> &MirrorTable {
        &self.mirrors
    }

    /// 健康度注册表（诊断页）。
    #[must_use]
    pub fn health(&self) -> &HealthRegistry {
        &self.health
    }

    /// 内容寻址缓存。
    #[must_use]
    pub fn cache(&self) -> &ContentCache {
        &self.cache
    }

    /// 候选规划：镜像展开 → 官方位置保持不变 → 其余按健康度排序。
    #[must_use]
    pub fn plan(&self, req: &DownloadRequest, now_ms: u64) -> Vec<Candidate> {
        let expanded = self.mirrors.expand(&req.url, req.kind, &self.region);
        let official_first = expanded
            .first()
            .is_none_or(|candidate| candidate.source == SOURCE_OFFICIAL);
        let mut official = Vec::new();
        let mut mirrors = Vec::new();
        for candidate in expanded {
            if candidate.source == SOURCE_OFFICIAL {
                official.push(candidate);
            } else {
                mirrors.push(candidate);
            }
        }
        let ranked = self.health.rank(&mirrors, req.kind, now_ms);
        if official_first {
            official.extend(ranked);
            official
        } else {
            let mut out = ranked;
            out.extend(official);
            out
        }
    }

    /// 单文件下载。
    pub async fn download(
        &self,
        mut req: DownloadRequest,
    ) -> Result<DownloadOutcome, DownloadError> {
        let started = Instant::now();
        let id = DownloadId::next();
        req.validate()?;
        if self.cfg.resume == ResumePolicy::Never {
            req.resume = ResumePolicy::Never;
        }

        if let Some(integrity) = req.integrity.clone() {
            if let Some(cached) = self.cache.lookup(&integrity, now_ms()) {
                let bytes = materialize(&cached, &req.dest).map_err(|err| {
                    DownloadError::new(
                        DownloadErrorKind::Io,
                        format!("缓存直取失败（{}）：{err}", req.dest.display()),
                    )
                })?;
                self.emit(ProgressEvent::Completed {
                    id,
                    bytes,
                    from_cache: true,
                    source: "cache".to_string(),
                    elapsed_ms: millis(started.elapsed()),
                });
                return Ok(DownloadOutcome {
                    id,
                    path: req.dest,
                    bytes,
                    from_cache: true,
                    source: "cache".to_string(),
                    resumed_from: 0,
                    elapsed: started.elapsed(),
                });
            }
        }

        let plan = self.plan(&req, now_ms());
        self.announce(&id, &req, plan.len());
        if plan.is_empty() {
            return Err(DownloadError::new(
                DownloadErrorKind::InvalidRequest,
                format!("没有可用候选源：{}", req.url),
            ));
        }

        let _permit = self.acquire().await?;
        let mut part = PartFile::for_dest(&req.dest);
        let mut suppressed: Vec<String> = Vec::new();
        let mut previous: Option<String> = None;

        for candidate in &plan {
            if let Some(from) = previous.take() {
                self.emit(ProgressEvent::SourceSwitched {
                    id,
                    from,
                    to: candidate.source.clone(),
                });
            }
            // 冷却中 / 刚出冷却的源只给一次机会（那就是「恢复探测」）。
            let max_attempts = self.attempts_for(&candidate.source, req.kind, now_ms());

            for attempt in 0..max_attempts {
                let attempt_started = Instant::now();
                let outcome = self.attempt(id, &req, candidate, &mut part).await;
                match outcome {
                    Ok(ok) => {
                        self.health.record_success(
                            &candidate.source,
                            req.kind,
                            attempt_started.elapsed(),
                            now_ms(),
                        );
                        let outcome = self.finish(&req, part, id, candidate, ok, started)?;
                        self.flush_health();
                        return Ok(outcome);
                    }
                    Err(err) => {
                        // 4xx（NotFound）不算「源坏」：院校镜像只留最新补丁，请求旧补丁必然 404，
                        // 记成失败会把唯一可用镜像冷却掉（M33/M34 实测：TUNA 每个目录只保 1 个归档）。
                        if err.kind == DownloadErrorKind::NotFound {
                            self.health.record_missing(
                                &candidate.source,
                                req.kind,
                                &err.message,
                                now_ms(),
                            );
                        } else {
                            self.health.record_failure(
                                &candidate.source,
                                req.kind,
                                &err.message,
                                now_ms(),
                            );
                        }
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
            previous = Some(candidate.source.clone());
        }

        // 全部候选失败：保留中间文件与元数据，下次（甚至下次启动）可续传。
        part.preserve();
        self.flush_health();
        Err(DownloadError::new(
            DownloadErrorKind::AllSourcesFailed,
            format!("{} 个候选源全部失败：{}", plan.len(), req.url),
        )
        .with_suppressed(suppressed))
    }

    fn finish(
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

    pub(crate) fn announce(&self, id: &DownloadId, req: &DownloadRequest, candidates: usize) {
        self.emit(ProgressEvent::Started {
            id: *id,
            kind: req.kind,
            url: req.url.clone(),
            dest: req.dest.display().to_string(),
            candidates,
            total_bytes: req.expected_size,
        });
    }

    pub(crate) async fn acquire(&self) -> Result<tokio::sync::OwnedSemaphorePermit, DownloadError> {
        Arc::clone(&self.sem).acquire_owned().await.map_err(|_| {
            DownloadError::new(DownloadErrorKind::Io, "并发闸门已关闭（引擎正在销毁）")
        })
    }

    /// 该源此刻给几次尝试：
    ///
    /// * 冷却中 → 1 次（这次就是「恢复探测」，失败会重新冷却）；
    /// * 刚出冷却、还没洗白 → 也只给 1 次（`is_probe()`，别让死源每次到期都吃满 3 次）；
    /// * 正常 → 配置的 `max_attempts_per_source`。
    pub(crate) fn attempts_for(&self, source: &str, kind: ResourceKind, now_ms: u64) -> u32 {
        let health = self.health.health_of(source, kind);
        if health.is_cooling_down(now_ms) || health.is_probe(now_ms) {
            1
        } else {
            self.cfg.max_attempts_per_source.max(1)
        }
    }

    /// 一次下载收尾时把健康度节流落盘（对齐 `cache.persist()` 的位置，但带节流）。
    pub(crate) fn flush_health(&self) {
        let _ = self.health.flush(now_ms());
    }

    pub(crate) fn next_delay(&self, attempt: u32) -> Duration {
        let mut jitter = self
            .jitter
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.cfg.backoff.next_delay(attempt, &mut jitter)
    }

    pub(crate) fn emit(&self, event: ProgressEvent) {
        self.progress.emit(event);
    }
}

/// 缓存命中时把文件变成工作副本（硬链接优先，跨卷退化为复制）。
fn materialize(source: &Path, dest: &Path) -> std::io::Result<u64> {
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
