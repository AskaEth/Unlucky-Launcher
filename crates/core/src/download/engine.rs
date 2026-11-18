//! 下载引擎：编排层。
//!
//! 一次 `download()` 的固定剧情：
//!
//! ```text
//! 校验请求 → 内容寻址缓存直取（零网络）
//!          → 候选规划（镜像表展开 + 健康度排序）
//!          → Semaphore 限流          ← 容量来自设置（M193，见 config.rs）
//!          → 候选源外循环（换源），每源内循环（指数退避 + 抖动）
//!              → 续传判定（保守）→ 单连接 / 多连接分块 → 流式校验 → 原子落盘
//!          → 失败则保留中间文件供下次续传
//! ```
//!
//! 中间文件是本会话命名的（`<name>.ullpart-<session>-<seq>`），下次（甚至下次启动）由 `PartFile::for_dest` 认领并改名，
//! 续传判定照旧（身份规则见 `atomic.rs` 模块头）。传输细节在 `transfer.rs`，错误分类在 `http.rs`，本文件只负责顺序与策略。

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

use super::atomic::PartFile;
use super::backoff::Jitter;
use super::cache::{now_ms, ContentCache};
use super::config::{EngineConfig, MAX_CONCURRENCY};
use super::error::{DownloadError, DownloadErrorKind};
use super::health::HealthRegistry;
use super::http;
use super::mirrors::MirrorTable;
use super::progress::{ProgressEmitter, ProgressEvent};
use super::proxy::ProxySetting;
use super::region::RegionProfile;
use super::stall::{StallReport, StallStage};
mod finish;

pub(crate) use finish::millis;

use super::types::{
    Candidate, DownloadId, DownloadOutcome, DownloadRequest, ResourceKind, ResumePolicy,
    SOURCE_OFFICIAL,
};

mod inspect;

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
    /// 代理解析失败的原因（`None` = `cfg.proxy` 那一档原样生效）。见 [`Self::proxy_problem`]。
    pub(crate) proxy_problem: Option<String>,
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
        // 代理地址解析不了（手改文件写坏了 `proxy_url`）⇒ 回落到**直连**重建：不能用
        // `reqwest::Client::new()` 兜（那会跟随环境变量代理，「回落」就成了不确定行为，还会顺手
        // 丢掉 UA / 超时 / 重定向策略）。原因留在引擎上，宿主打印出来（[`Self::proxy_problem`]）。
        let (http, proxy_problem) = match http::build_client(&cfg) {
            Ok(client) => (client, None),
            Err(err) => {
                let mut direct = cfg.clone();
                direct.proxy = ProxySetting::Direct;
                let client = http::build_client(&direct).unwrap_or_else(|_| reqwest::Client::new());
                (client, Some(err.message))
            }
        };
        // 硬上限夹取（M193）：手改的 launcher.json / CLI 自拼的 cfg 都可能写着 9999，在唯一构造点再夹一次。
        let permits = cfg.max_concurrency.clamp(1, MAX_CONCURRENCY);
        let sem = Arc::new(Semaphore::new(permits));
        let seed = now_ms() ^ (u64::from(u32::try_from(permits).unwrap_or(u32::MAX)) << 32);
        // 纯记账（M165）：卡在构造这个点上，才统计得到绕过登记处的调用点（守卫靠 `constructions`）。
        super::shared::record_build(cache.root());
        Self {
            http,
            proxy_problem,
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
                let bytes = finish::materialize(&cached, &req.dest).map_err(|err| {
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

        let _permit = self.acquire(req.expected_size).await?;
        let mut part = PartFile::for_dest(&req.dest);
        let mut suppressed: Vec<String> = Vec::new();
        // 逐源的叶子分类（出去时汇总成一条，见 `DownloadError::leaf`）——与文本路径
        // (`bytes.rs`) 逐字同形。少了这一行，上层（`addon::import::fetch`）只看到
        // `AllSourcesFailed`，「内容与声明不符」会被读成可容忍的网络失败（M318 真包盘查）。
        let mut kinds: Vec<DownloadErrorKind> = Vec::new();
        let mut previous: Option<String> = None;
        // 失速现场只跟**最后一次**尝试：更早的源怎么挂的不能冒充根因。
        let mut last_stall: Option<StallReport> = None;
        // 整作业预算用完 ⇒ 不再试剩余候选源（见下面的 `job_budget_exhausted`）。
        let mut exhausted = false;

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
                        last_stall = err.stall.as_deref().copied();
                        // 4xx（NotFound）不算「源坏」：院校镜像只留最新补丁，请求旧补丁必然 404，
                        // 记成失败会把唯一可用镜像冷却掉（M33/M34 实测：TUNA 每目录只保 1 个归档）。
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
                        // 整作业总期限用完是**链路级**判决（同一份字节、同一条链路）：
                        // ① `err.is_retryable()` 让同源不再重试（每试一次都要再烧满一整个预算）；
                        // ② 下面外循环的 `exhausted` 让它连剩余候选源都不试。
                        // 复审 r1 p2-2 的算术：不这么做，188MB/20 KiB/s 的用户会从「1.6 小时
                        // 慢慢下完」变成「每源 3 次 × 每个候选 × 100 min」的反复重下。
                        exhausted = err.job_budget_exhausted();
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
            previous = Some(candidate.source.clone());
            if exhausted {
                break;
            }
        }

        // 全部候选失败：保留中间文件与元数据，下次（甚至下次启动）可续传。
        part.preserve();
        self.flush_health();
        // 用户唯一看得见的那一行（`silent()` 引擎零事件、零日志）：把最后一次失速的
        // 阶段与三个数带上，否则「卡住不动」只是换成「下载失败」，仍然什么也说不出来。
        let detail = match &last_stall {
            Some(stall) => format!("（最后一次失速：{stall}）"),
            None => String::new(),
        };
        // 叶子成因要跟着出去：上层（导入补全 / 在线内容源）靠它区分「这个文件不存在」、
        // 「内容与声明不符」与「没联网」；只留 `AllSourcesFailed` 的话那套分类在生产路径上
        // 永远不可达。三个后缀收在 [`DownloadError::all_sources_failed`] 一处——M318 就是在这里
        // 复制文本路径（`bytes.rs`）时漏了中间那行 `.with_leaf(..)`。
        Err(DownloadError::all_sources_failed(
            format!("{} 个候选源全部失败：{}{detail}", plan.len(), req.url),
            suppressed,
            kinds,
            last_stall,
        ))
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

    /// 抢一张并发许可（**有界**：等许可也是一次 `await`）。
    ///
    /// 为什么必须设界：M289 之前任何一处无界 `await` 都能让一个作业永久攥着许可，同缓存根
    /// 的其它下载就全堵在这行（复审 r1 p2-1 点名的连坐形态）。上界取 `permit_timeout`
    /// （必须 ≥ 单个作业的总期限，见 `stall` 模块头），到点报阶段「等许可」——
    /// 不冒充「服务器不给我数据」。
    pub(crate) async fn acquire(
        &self,
        expected: Option<u64>,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, DownloadError> {
        match tokio::time::timeout(
            self.cfg.permit_timeout,
            Arc::clone(&self.sem).acquire_owned(),
        )
        .await
        {
            Ok(Ok(permit)) => Ok(permit),
            Ok(Err(_)) => Err(DownloadError::new(
                DownloadErrorKind::Io,
                "并发闸门已关闭（引擎正在销毁）",
            )),
            Err(_) => Err(DownloadError::stalled(
                StallReport::deadline(
                    StallStage::AcquirePermit,
                    0,
                    expected,
                    self.cfg.permit_timeout,
                ),
                format!(
                    "等并发许可超过 {}s（此刻可用 {} 张）",
                    self.cfg.permit_timeout.as_secs(),
                    self.sem.available_permits()
                ),
            )),
        }
    }

    /// 该源此刻给几次尝试：冷却中 / 刚出冷却未洗白 → 1 次（那就是恢复探测，别让死源每次到期
    /// 都吃满预算）；正常 → 设置里的 `download.retry_limit`（= `max_attempts_per_source`）。
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
