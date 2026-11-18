//! 分块任务的回收：两道失速判据 + 已完成段的记账。
//!
//! 从 `transfer_chunk.rs` 拆出只为守 400 行硬线（M293 r1：换源校验器修复 + 两条 p3 收口之后）。
//! 搬出来的代码逐字节未改，仍挂在同一个 `impl DownloadEngine` 上。

use std::sync::atomic::{AtomicU64, Ordering};

use super::super::chunk_resume::ChunkJournal;
use super::super::chunked::ChunkFetch;
use super::super::engine::DownloadEngine;
use super::super::error::{DownloadError, DownloadErrorKind};
use super::super::stall::{StallReason, StallReport, StallStage};

impl DownloadEngine {
    /// 回收分块任务，同时盯着「整个作业有没有在动」。
    ///
    /// 两道判据必须分得开（另一处是单块自己的读空闲超时，阶段 = 等字节）：
    /// * **无进度**：`counter` 连续 `chunk_stall_timeout` 没有新字节。分块任务卡在写盘上
    ///   （杀软/网络盘）时单块读超时不会响，只有这道抓得住——现场留给 `join_next` 的正是
    ///   「188MB 落完 99% 之后永久挂住」那个形态；
    /// * **总期限**：`chunk_job_budget`（按平均吞吐下限反推，见 `stall` 模块头）用完。
    ///
    /// 两者都归到阶段 [`StallStage::AwaitChunkJoin`]，靠 [`StallReason`] 区分。
    ///
    /// 成功的段在这里**顺手记账**（[`ChunkJournal::record`]）：这是唯一同时握着「段结果」与
    /// 「段区间」的地方，落到元数据之后，本作业后面怎么挂都不影响这几段的续传价值。
    pub(super) async fn await_chunks(
        &self,
        set: &mut tokio::task::JoinSet<Result<ChunkFetch, DownloadError>>,
        counter: &AtomicU64,
        total: u64,
        url: &str,
        journal: &mut ChunkJournal,
    ) -> Option<DownloadError> {
        let budget = self.cfg.chunk_job_budget(Some(total));
        let hard_deadline = tokio::time::Instant::now() + budget;
        let mut last_seen = 0_u64;
        let mut last_progress = tokio::time::Instant::now();
        loop {
            let seen = counter.load(Ordering::Relaxed);
            if seen > last_seen {
                last_seen = seen;
                last_progress = tokio::time::Instant::now();
            }
            let wake = (last_progress + self.cfg.chunk_stall_timeout).min(hard_deadline);
            match tokio::time::timeout_at(wake, set.join_next()).await {
                Ok(Some(Ok(Ok(fetched)))) => {
                    // 「这个源给的还是不是同一个文件」：`Content-Range` 的总长必须与我们规划的
                    // 一致，否则写下去的段根本拼不成目标文件（单连接路径同一处判据是收尾的
                    // `written != total`）。
                    if fetched.total.is_some_and(|server| server != total) {
                        absorb_completed(journal, set, total);
                        set.abort_all();
                        return Some(DownloadError::new(
                            DownloadErrorKind::Network,
                            format!(
                                "分块总长不符：响应声明 {}，本次规划 {total}（{url}）",
                                fetched.total.unwrap_or_default()
                            ),
                        ));
                    }
                    journal.record(fetched);
                }
                Ok(Some(Ok(Err(err)))) => {
                    absorb_completed(journal, set, total);
                    set.abort_all();
                    return Some(err);
                }
                Ok(Some(Err(err))) => {
                    absorb_completed(journal, set, total);
                    set.abort_all();
                    return Some(DownloadError::new(
                        DownloadErrorKind::Network,
                        format!("分块任务异常：{err}"),
                    ));
                }
                // 全部回收（成功）。
                Ok(None) => return None,
                Err(_) => {
                    let now = tokio::time::Instant::now();
                    // 窗口内确实有字节进来 ⇒ 续期（「慢但有效」不被误杀的机制就在这一行）。
                    let seen = counter.load(Ordering::Relaxed);
                    if now < hard_deadline && seen > last_seen {
                        last_seen = seen;
                        last_progress = now;
                        continue;
                    }
                    let report = if now >= hard_deadline {
                        StallReport::deadline(
                            StallStage::AwaitChunkJoin,
                            seen,
                            Some(total),
                            now - last_progress,
                        )
                    } else {
                        StallReport::no_progress(
                            StallStage::AwaitChunkJoin,
                            seen,
                            Some(total),
                            now - last_progress,
                        )
                    };
                    absorb_completed(journal, set, total);
                    set.abort_all();
                    let context = match report.reason {
                        StallReason::Deadline => {
                            format!("分块作业超过总期限 {}s（{url}）", budget.as_secs())
                        }
                        StallReason::NoProgress => format!("分块作业长时间没有新字节（{url}）"),
                    };
                    return Some(DownloadError::stalled(report, context));
                }
            }
        }
    }
}

/// 把「已经落完、但还没被 `join_next` 收走」的段也记进账（复审 r1 p3-1）。
///
/// `abort_all()` 只中止任务、不替我们保存已完成的结果；失败分支直接 `return` 等于把那些**字节
/// 已经完整落盘**的段扔掉，下次整段重抓（4 连接时最多白丢 3 段）。这里在 abort 之前用
/// `try_join_next` 把就绪结果收干净，总长判据与正常回收同一道。
///
/// 边界：这是**尽力减少浪费**，不是正确性前提——没收走的段只会让下次多下，不会被误用
/// （记录带段摘要自证）。也正因为它是「谁先完成」的竞态，没有确定性的用例能钉住它：
/// 用例只能断言「失败后已完成的段被记账」，而失败与完成的先后顺序由调度器决定。
fn absorb_completed(
    journal: &mut ChunkJournal,
    set: &mut tokio::task::JoinSet<Result<ChunkFetch, DownloadError>>,
    total: u64,
) {
    while let Some(joined) = set.try_join_next() {
        if let Ok(Ok(fetched)) = joined {
            if fetched.total.is_none_or(|server| server == total) {
                journal.record(fetched);
            }
        }
    }
}
