//! 大文件多连接分块的编排与校验。
//!
//! 分块本身由 `chunked.rs` 负责（纯规划 + 单块抓取），这里负责并发、进度上报、
//! 续传元数据的读写与「整文件重算一次摘要」（分块是乱序写，只能整文件校验）。
//!
//! # 续传（M293：台账 P21 第 2 条 + P19(b)）
//!
//! 此前这里开头无条件 `part.discard()`、失败也 `discard()`，于是 188MB 的 JDK 任何一次失败
//! 都从 0 重下；M289 又给「预算耗尽」加了「不再重试、不再换源」的短路，慢链路上一次晚到的
//! 失败就变成「这个文件下不来」。现在：
//!
//! * 每落完一段就把它连同**这段字节的摘要**写进 `ResumeMeta::chunks`（`ChunkJournal`）；
//! * 失败**不再**丢中间文件——已完成的段是几十分钟的字节，元数据里带着它们的自证记录；
//! * 下次进本函数先过 [`super::chunk_resume::decide`]（身份）与 [`DownloadEngine::verify_chunks`]
//!   （旧字节自证），两道门都过才复用；任何一道不过就整份丢掉从头再来；
//! * 整文件摘要照旧在传输结束后整份重算，覆盖新下 + 复用的全部字节。
//!
//! 唯一仍然主动 `discard()` 的失败是 `RangeUnsupported`：服务器不支持 Range 时单连接路径
//! 接不住「预分配 + 乱序段」这种形状的文件（它只会截断重写），留着没意义。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::atomic::PartFile;
use super::cache::now_ms;
use super::chunk_resume::{self, ChunkDecision, ChunkJournal};
use super::chunked;
use super::engine::{AttemptOk, DownloadEngine};
use super::error::{DownloadError, DownloadErrorKind};
use super::progress::ProgressEvent;
use super::resume::{ChunkRecord, ResumeMeta};
use super::stall::{StallReport, StallStage};
use super::types::{Candidate, DownloadId, DownloadRequest, HashAlgo, Integrity};
use super::verify;

/// 分块任务的回收（失速判据 + 已完成段记账）：拆出守 400 行硬线。
mod join;

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
        let integrity = req.integrity.as_ref();

        // ① 身份：旧分块是不是**这次**下载的产物（纯判定，不碰盘）。
        let meta_path = part.meta_path();
        let previous = ResumeMeta::load(&meta_path);
        let decision = chunk_resume::decide(
            req.resume,
            part.len(),
            previous.as_ref(),
            &candidate.url,
            integrity.map(|item| (item.algo, item.hex.as_str())),
            total,
            &ranges,
        );
        // ② 自证：按记录重读那些字节、重算摘要。任何一段对不上就整份丢掉从头再来
        //    （理由见 `chunk_resume` 模块头：对不上说明这份元数据本身已经不可信）。
        let mut done: Vec<ChunkRecord> = Vec::new();
        let mut reused = 0_u64;
        match decision {
            ChunkDecision::Reuse { done: records } => {
                // 复用字节数直接取自证的结果（唯一真相源：少读一段就少算一段）。
                match self.verify_chunks(part, &records).await {
                    Some(bytes) => {
                        done = records;
                        reused = bytes;
                    }
                    None => part.discard(),
                }
            }
            ChunkDecision::Restart { .. } => part.discard(),
        }
        // 校验器既是**源**的属性（镜像/CDN/对象存储各自生成 ETag），也是**一份表示**的属性——
        // 两件事缺一不可，所以「谁能当校验器」只有 `carry` 这一个判定点：
        //
        // * **源**（复审 r1 p2-1）：`previous.url` 不是当前源就当没有。把 A 的校验器写进 `url=B`
        //   的新元数据之后，身份门的 `NoValidator` 会被一个不相干的值满足，于是拿 **A 的 ETag**
        //   去对 B 发 `If-Range`：按 RFC 7233 §3.2，服务器对不上就回 200 + 完整正文 ⇒
        //   `fetch_chunk` 判 `RangeUnsupported` ⇒ 丢掉整份 ⇒ 单连接从头下。
        // * **表示**（复审 r2 p2-1）：`done` 为空表示这一轮没有任何字节要接着用（`Restart`，或
        //   第一次跑），那上一份元数据里的校验器描述的是**另一份**表示。留着它这一轮看不出问题
        //   （`if_range` 本来就不发），但它会被原样写进新元数据，而 `record` 只在缺值时写 ⇒ 本轮
        //   响应的真实校验器永远盖不上它 ⇒ 下一次续传拿一个已知可能不匹配的值去问服务器 ⇒ 同上
        //   那条 200/丢整份的路。实测那是 2.775×，比「干脆不续传」的 1.775× 还费一整份。
        //   清掉之后，本轮响应里的真实校验器由 `ChunkJournal::record` 填进来；最坏也只是
        //   `NoValidator` ⇒ 整份重下（1.775× 那条基线），不会更坏。
        //
        // 为什么不选复审给的另两种修法：「换源时清空」（B）只是「源」这一维的特例写法，条件放在
        // 读取处比散到写入处更难漏；「校验器加来源字段」（C）会在 `ResumeMeta` 里多一份可与
        // `url` 漂移的真相，而当前模型是「一个中间文件 = 一个目标」，没有并存多源的形态。
        let same_source = previous.as_ref().filter(|meta| meta.url == candidate.url);
        let carry = if done.is_empty() { None } else { same_source };
        // 续传时把**同源、同一份表示**的旧校验器带上：这是 `If-Range` 的取值来源，也是「远端
        // 内容没变」的协议层证据。
        let if_range = carry.and_then(|meta| meta.validator().if_range_value().map(str::to_string));

        part.ensure_parent_async()
            .await
            .map_err(|err| io_err("创建下载目录失败", &err))?;
        {
            let mut options = tokio::fs::OpenOptions::new();
            options.create(true).write(true);
            if done.is_empty() {
                // 从头：先把可能存在的旧内容截掉，再预分配整长。
                options.truncate(true);
            }
            let file = options
                .open(part.path())
                .await
                .map_err(|err| io_err("预分配中间文件失败", &err))?;
            // 续传时这一行是「归一化」：分块写是随机偏移，长度本该已经是 total；
            // 若被外部截短/加长过，先摆正，缺的段下面的补抓会填满。
            file.set_len(total)
                .await
                .map_err(|err| io_err("预分配长度失败", &err))?;
        }

        let mut journal = ChunkJournal::begin(
            ResumeMeta {
                url: candidate.url.clone(),
                // 只沿用「同一个源 + 同一份表示」的旧校验器（见上面的 `carry`）；其余情况当没有，
                // 让下面每一段的响应自己去填。
                etag: carry.and_then(|meta| meta.etag.clone()),
                last_modified: carry.and_then(|meta| meta.last_modified.clone()),
                total: Some(total),
                algo: integrity.map_or(HashAlgo::Sha1, |item| item.algo),
                hash: integrity.map_or_else(String::new, |item| item.hex.clone()),
                saved_at_ms: now_ms(),
                chunks: Some(done),
            },
            meta_path,
        );

        let counter = Arc::new(AtomicU64::new(reused));
        let reporter = self.spawn_reporter(id, Arc::clone(&counter), total);

        let deadlines = self.cfg.deadlines();
        let todo = chunk_resume::remaining(&ranges, journal.done());
        let mut set = tokio::task::JoinSet::new();
        for range in todo {
            let client = self.http.clone();
            let url = candidate.url.clone();
            let path = part.path().to_path_buf();
            let progress = Arc::clone(&counter);
            let if_range = if_range.clone();
            set.spawn(async move {
                chunked::fetch_chunk(
                    &client,
                    &url,
                    range,
                    &path,
                    deadlines,
                    &progress,
                    if_range.as_deref(),
                )
                .await
            });
        }

        let failure = self
            .await_chunks(&mut set, &counter, total, &candidate.url, &mut journal)
            .await;
        reporter.abort();

        if let Some(err) = failure {
            // 已完成的段连同自证摘要都在元数据里（`ChunkJournal` 每段落一次盘），中间文件留着，
            // 下一次（甚至下次启动）接着下。只有「这个源不支持 Range」才把文件丢掉：
            // 单连接路径接不住预分配 + 乱序段的形状。
            if err.kind == DownloadErrorKind::RangeUnsupported {
                part.discard();
            }
            return Err(err.with_source(candidate.source.as_str()));
        }

        // 分块是乱序写，只能整文件重算一次摘要——这一步覆盖新下的字节**与**复用的旧字节。
        self.verify_part(part, integrity, Some(total), id).await?;
        self.emit(ProgressEvent::Progress {
            id,
            downloaded: total,
            total_bytes: Some(total),
        });
        Ok(AttemptOk {
            bytes: total,
            resumed_from: reused,
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

    /// 旧分块自证：按记录逐段重读、重算摘要。
    ///
    /// `Some(bytes)` = 每一段都对得上，返回值就是**可复用的字节数**（调用方直接拿它当
    /// `resumed_from`，别再从记录另算一遍——两份口径会静默漂移）；`None` = 证明不了
    /// （摘要不符 / 盘上不够长 / 读盘失败 / 超时），调用方必须整份丢掉。
    ///
    /// 超时也归到「证明不了」：`spawn_blocking` 的线程不可取消，到点后它还在读文件、还持着
    /// 句柄（与 `verify_part` 里那段注释同族，代价见 `chunk_resume` 模块头「已知代价」）。
    async fn verify_chunks(&self, part: &PartFile, records: &[ChunkRecord]) -> Option<u64> {
        let path = part.path().to_path_buf();
        let records = records.to_vec();
        let job =
            tokio::task::spawn_blocking(move || chunk_resume::verify_records(&path, &records));
        match tokio::time::timeout(self.cfg.verify_timeout, job).await {
            Ok(Ok(Ok(bytes))) => Some(bytes),
            _ => None,
        }
    }

    /// 整文件摘要（有界、分类见下）。
    ///
    /// `integrity: None` 时**直接返回 `Ok`**：这类请求没有任何端到端的摘要可比，末尾这一步
    /// 空转。两条路径都如此（单连接一直如此，本 mission 未改）。此时「续传后重新校验」只剩
    /// 旧段自证 + `If-Range` 两道，`chunk_resume` 模块头的「已知代价」写了这个一致性边界
    /// （长度相同、内容换了一份的文件不可察觉；现实入口是没有上游 sha1 的 `ModFile`）。
    pub(crate) async fn verify_part(
        &self,
        part: &mut PartFile,
        integrity: Option<&Integrity>,
        expected: Option<u64>,
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
        // 整文件重算是同步阻塞 IO：挪到 blocking 池并套上界，否则「校验阶段卡住」
        // （网络盘掉线、杀软锁文件）照样是永久挂住，且它发生在字节全部收完之后，
        // 从进度上看与「下载完成」一模一样。
        //
        // 代价（复审 r1 p3-1，未验）：`spawn_blocking` 的线程**不可取消**——到点后它还在
        // 读文件、还持着句柄，重试会再堆一个（每源最多 3 个）。**假设未验**：Windows 上
        // 那个句柄可能让随后的同名重建（`attempt_chunked` 的 `OpenOptions::create(true)`）
        // 拿到 `Access is denied`，表现为「预分配中间文件失败」而不是干净重试
        // （`PartFile::discard` 忽略错误）。仍然有界、有分类，只是诊断会拐一道。
        let on_disk = part.len();
        let path = part.path().to_path_buf();
        let algo = integrity.algo;
        let digest = tokio::time::timeout(
            self.cfg.verify_timeout,
            tokio::task::spawn_blocking(move || verify::digest_file(&path, algo)),
        )
        .await
        .map_err(|_| {
            DownloadError::stalled(
                StallReport::deadline(
                    StallStage::Verify,
                    on_disk,
                    expected,
                    self.cfg.verify_timeout,
                ),
                format!(
                    "整文件重算摘要超过 {}ms",
                    self.cfg.verify_timeout.as_millis()
                ),
            )
        })?
        .map_err(|err| {
            DownloadError::new(DownloadErrorKind::Io, format!("校验任务异常退出：{err}"))
        })?
        .map_err(|err| io_err("读取中间文件失败", &err))?;
        if verify::matches(&integrity.hex, &digest) {
            return Ok(());
        }
        part.discard();
        Err(DownloadError::new(
            DownloadErrorKind::Integrity,
            format!("校验不符：期望 {} 实得 {digest}", integrity.hex),
        ))
    }
}

fn io_err(context: &str, err: &std::io::Error) -> DownloadError {
    DownloadError::new(DownloadErrorKind::Io, format!("{context}: {err}"))
}
