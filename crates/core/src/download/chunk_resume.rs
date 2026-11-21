//! 分块下载的**续传判定与旧字节自证**。
//!
//! 台账 P21 第 2 条 + P19(b)：分块路径此前**不写**续传元数据、每次进 `attempt_chunked` 先
//! `part.discard()`，于是 188MB 的 JDK 任何一次失败都从 0 重下。这里补上「记住落了哪些段」，
//! 但**不是**把旧字节当可信——分块路径的旧字节比单连接的更可疑，原因有三条：
//!
//! 1. 中间文件被 `set_len(total)` 预分配过，长度恒等于总长：`resume::decide` 的「长度 = 已下」
//!    这条前提在这里根本不成立（它会判成 `Complete`，然后整文件校验把一份没下完的文件判死）；
//! 2. 段是**乱序**写的，磁盘上「有字节」不等于「这一段是完整的」——进程被强杀时正写到一半的段
//!    与写完了的段在文件里长得一模一样；
//! 3. 同一个 URL 的响应可能在两次尝试之间换了内容（镜像回源、滚动发布）。
//!
//! 所以复用旧段要过两道门，缺一不可：
//!
//! * **身份**（[`decide`]，纯函数）：策略允许续传、元数据在、URL / 总长 / 摘要算法 / 期望摘要
//!   全部与本次请求一致、有 ETag 或 Last-Modified 作校验器（续传请求会带 `If-Range`，让服务器
//!   自己回答「内容变了没有」），并且每条记录都能**精确对上**本次分块规划里的某一段；
//! * **自证**（[`verify_records`]）：按记录重读那段字节、重算摘要，与记录比对。
//!
//! 两道门任一不过 ⇒ 丢掉整份中间文件**从头再来**（不是「只补坏的那一段」）：
//! 摘要对不上意味着这份元数据本身已经不可信（盘上数据在腐坏，或者元数据是崩溃前写的、
//! 盘上并没有落那样），同一份元数据里的**其它**记录同样没有理由被信任。这条与
//! [`super::resume`] 的保守立场一致——重下 100MB 也比交付一个坏文件便宜。
//!
//! 最后还有第三道保险：整文件摘要在传输结束后**整份重算**（`transfer_chunk::verify_part`），
//! 覆盖新下的字节与复用的字节。前两道门只保证「旧字节是我们写的」，第三道门才保证
//! 「我们写的是对的那份」。
//!
//! 写的这一侧是本文末的 [`ChunkJournal`]：每成功回收一段就落一次盘（原子写），失败时元数据
//! 里已经有「哪几段、各自摘要是什么」，于是下一次尝试能接着下而不是从 0。
//!
//! # 校验器跟着**源**与**表示**走（复审 r1 + r2 的 p2-1）
//!
//! 校验器（ETag / Last-Modified）是「某个 URL 上**某一份表示**」的属性，不是这个文件的属性。
//! 它有两个独立的失效维度，缺一不可：
//!
//! * **源**：镜像 / CDN / 对象存储各自生成，跨源几乎必然不同。把 A 的校验器带进源 B 的元数据，
//!   身份门的「有校验器」就被一个不相干的值满足，于是拿 A 的 ETag 去问 B；
//! * **表示**：同一个 URL 也可能换了一份表示（重新发布、CDN 回源到新对象）。`done` 为空表示这
//!   一轮没有任何字节要接着用，此时上一份元数据里的校验器描述的不是这次要下的东西。
//!
//! 两条路的终点相同：按 RFC 7233 §3.2，服务器对不上的 `If-Range` 会回 200 + 完整正文，分块路径
//! 拿不到 Range ⇒ `RangeUnsupported` ⇒ 整份丢掉退化单连接从头下（本 mission 要消灭的形态；r2 实测
//! 同源换表示那条是 2.775×，比不续传的 1.775× 还费）。所以沿用旧校验器的条件只有一处判定
//! （`transfer_chunk` 的 `carry` = 「同源 **且** `done` 非空」），写元数据与发 `If-Range` 都走它。
//!
//! # 已知代价（复审 r1 p3-3）
//!
//! * **每次续传都要重读「即将复用」的那些字节**（不是整份文件：复用 3/4 才读 3/4，1 段就只读
//!   1 段）。这是本模块存在的意义本身——不复读就无法证明盘上旧字节还是当初那一段。188MB 文件
//!   落完 3/4 时约 141MB 顺序读，本地 SSD 是毫秒级，网络盘上可能到几秒；它发生在
//!   「进度从复用字节开始」之前，所以界面会看起来停在原地（未做专门进度事件，见 mission 报告
//!   的跟进项）。要省掉这次读只有一条路：相信元数据里的摘要而不复核——那等于把「旧字节可信」
//!   当前提，与上面的自证要求直接冲突，**明确不做**。
//! * **请求没带期望摘要时，末尾那道整文件摘要空转**（`verify_part` 在 `integrity: None` 时直接
//!   `Ok`）。此时「续传后重新校验」只剩两道：旧段自证（只能证明「这些字节是我们写下去的」）与
//!   `If-Range`（只能证明「服务器那份表示没变」，而服务器忽略该头时就失效）。边界很清楚：
//!   **长度相同、内容换了一份**的文件在这种配置下无法察觉（末尾没有摘要可比）。这不是本 mission
//!   引入的（单连接路径一直如此，`transfer.rs` 零改动），但分块路径继承了它。现实入口是
//!   `ModFile` 上游没给合法 sha1 的时候（`addon/online/download.rs` 只在 sha1 合法时才设
//!   `integrity`，而 mod / 整合包正会走分块）；`Library` 元数据、`download.bin` 缓存也有无摘要
//!   调用点。要收紧就得要求「分块路径必须带 integrity」，那会让这类下载退回单连接或直接失败——
//!   属于产品决策，不在这里单方面改。
//! * **超时后的读盘线程不可取消**：[`verify_records`] 跑在 `spawn_blocking` 上，调用方到达
//!   `verify_timeout` 后会丢弃中间文件并重写同一路径，而那个线程可能还在读、还持着句柄
//!   （与 `transfer_chunk::verify_part` 里记的那条同族）。假设未验：Windows 上可能让随后同名
//!   重建拿到 `Access is denied`，表现为「预分配中间文件失败」而不是干净重试。有界、有分类，
//!   但诊断会拐一道。

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::cache::now_ms;
use super::chunked::{self, ByteRange, ChunkFetch};
use super::resume::{ChunkRecord, RestartReason, ResumeMeta};
use super::types::{HashAlgo, ResumePolicy};
use super::verify;

/// 复用旧分块的判定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkDecision {
    /// 这些段可以复用——**但调用方仍须先跑 [`verify_records`]**，本判定只看不碰盘。
    Reuse {
        /// 已落盘且对得上本次规划的分块记录。
        done: Vec<ChunkRecord>,
    },
    /// 从头下载（调用方要丢掉整份中间文件）。
    Restart {
        /// 原因（诊断用；与单连接路径共用同一个枚举）。
        reason: RestartReason,
    },
}

/// 判定能否复用旧分块（纯函数：不读盘、不看时钟）。
///
/// `part_len` 为 0 表示没有中间文件（或它是 0 字节）。`planned` 是本次要用的分块规划，
/// 记录必须**逐一精确命中**其中的段：连接数变了、总长变了、或记录本身畸形，都判从头再来。
#[must_use]
pub fn decide(
    policy: ResumePolicy,
    part_len: u64,
    meta: Option<&ResumeMeta>,
    url: &str,
    integrity: Option<(HashAlgo, &str)>,
    total: u64,
    planned: &[ByteRange],
) -> ChunkDecision {
    let restart = |reason| ChunkDecision::Restart { reason };
    if policy == ResumePolicy::Never {
        return restart(RestartReason::PolicyNever);
    }
    if part_len == 0 {
        return restart(RestartReason::NoPart);
    }
    let Some(meta) = meta else {
        return restart(RestartReason::NoMeta);
    };
    // 没有分块清单的元数据（单连接路径写的）证明不了「乱序段各自到哪」，一律从头来。
    let Some(records) = meta.chunks.as_ref() else {
        return restart(RestartReason::NoMeta);
    };
    if meta.url != url {
        return restart(RestartReason::UrlChanged);
    }
    if let Some((algo, hash)) = integrity {
        if meta.algo != algo || !meta.hash.eq_ignore_ascii_case(hash) {
            return restart(RestartReason::HashChanged);
        }
    }
    // 总长变了就是另一个文件：旧段的偏移全部作废（只比 URL 抓不住这个）。
    if meta.total != Some(total) {
        return restart(RestartReason::SizeChanged);
    }
    // 没有校验器就无法让服务器回答「内容变了没有」，只能从头下（与单连接同一立场）。
    if meta.validator().if_range_value().is_none() {
        return restart(RestartReason::NoValidator);
    }
    // 每条记录必须是规划里**原样**的一段，且不许重复：段边界一旦漂移，
    // 「已落盘多少」就说不清了（长度恰好但位置错的文件会被整文件校验兜住，但那已经是白下）。
    let mut done: Vec<ChunkRecord> = Vec::with_capacity(records.len());
    for record in records {
        if record.is_empty() || record.end >= total {
            return restart(RestartReason::ChunkLayout);
        }
        if !planned
            .iter()
            .any(|range| range.start == record.start && range.end == record.end)
        {
            return restart(RestartReason::ChunkLayout);
        }
        if done
            .iter()
            .any(|seen| seen.start == record.start && seen.end == record.end)
        {
            return restart(RestartReason::ChunkLayout);
        }
        done.push(record.clone());
    }
    ChunkDecision::Reuse { done }
}

/// 本次还要抓的段 = 规划 − 已落盘（[`decide`] 已保证 `done ⊆ planned` 且边界精确）。
#[must_use]
pub fn remaining(planned: &[ByteRange], done: &[ChunkRecord]) -> Vec<ByteRange> {
    planned
        .iter()
        .copied()
        .filter(|range| {
            !done
                .iter()
                .any(|record| record.start == range.start && record.end == range.end)
        })
        .collect()
}

/// 按记录逐段重读、重算摘要：返回**可复用的字节数**。
///
/// 任一记录对不上（摘要不符 / 读不满一段 / 读盘失败）就返回 `Err`，调用方必须整份丢掉。
/// 同步阻塞 IO：异步路径要把它放进 `spawn_blocking` 并套上 `verify_timeout`
/// （与 `transfer_chunk::verify_part` 同一套做法）。
pub fn verify_records(path: &Path, records: &[ChunkRecord]) -> io::Result<u64> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut verified = 0_u64;
    for record in records {
        let mut hasher = verify::Hasher::new(chunked::CHUNK_DIGEST);
        file.seek(SeekFrom::Start(record.start))?;
        let mut left = record.len();
        while left > 0 {
            let want = usize::try_from(left.min(buffer.len() as u64)).unwrap_or(buffer.len());
            let read = file.read(&mut buffer[..want])?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!(
                        "分块 [{}..={}] 在盘上不够长（缺 {left} 字节）",
                        record.start, record.end
                    ),
                ));
            }
            hasher.update(&buffer[..read]);
            left -= read as u64;
        }
        let actual = hasher.finish();
        if !verify::matches(&record.sha1, &actual) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "分块 [{}..={}] 摘要不符：记录 {} 实得 {actual}",
                    record.start, record.end, record.sha1
                ),
            ));
        }
        verified += record.len();
    }
    Ok(verified)
}

/// 续传元数据的增量写手：每落完一段就把「这一段已落盘、摘要是它」写进元数据。
///
/// 为什么不收工再写：进程被强杀 / 断电时收工那次写根本不会发生，而已经落盘的段是好几分钟到
/// 几十分钟的字节。代价是每段一次小文件原子写（`plan` 最多切 `MAX_CONNECTIONS` = 16 段，
/// 单个作业最多 16 次），而且**只有成功回收的段才记**——写到一半被 abort 的段不会被记，
/// 下次整段重抓（于是「记录里的段」不会出现半截字节）。
pub struct ChunkJournal {
    /// 元数据本体（`chunks` 随进度增长）。
    meta: ResumeMeta,
    /// 落盘路径。
    path: PathBuf,
}

impl ChunkJournal {
    /// 建账并立刻落一次盘。
    ///
    /// 这一次写还担着第二个作用：元数据里 `chunks: Some(..)` 是「这个中间文件属于分块路径」的
    /// 标记，下次尝试靠它躲开单连接路径「长度 = 已下满」的误判（预分配文件长度恒等于总长）。
    pub fn begin(meta: ResumeMeta, path: PathBuf) -> Self {
        let mut journal = Self { meta, path };
        journal.save();
        journal
    }

    /// 已落盘的段。
    pub fn done(&self) -> &[ChunkRecord] {
        self.meta.chunks.as_deref().unwrap_or_default()
    }

    /// 记一个已落盘段（顺带补上服务器校验器）。
    ///
    /// 校验器**只在还没有值时写入**，这是刻意的（复审 r1 问过）：同一个 URL 的续传里如果服务器
    /// 换了 ETag，保留旧值反而是保守选择——下一次带着旧 ETag 发 `If-Range` 会被服务器判「变了」
    /// 而回 200，于是干净重下；若改成覆盖成新值，下一次会把「旧字节 + 新表示」当成同一份续下去，
    /// 只能等末尾整文件摘要才发现（那已经是白下）。跨源、跨表示的情况根本不走到这里：
    /// `transfer_chunk` 的 `carry`（同源 **且** `done` 非空）已经让 `etag` 从 `None` 起步，
    /// 这里填的只可能是**本源、本轮响应**带来的校验器。
    pub fn record(&mut self, fetched: ChunkFetch) {
        if self.meta.etag.is_none() {
            self.meta.etag = fetched.etag;
        }
        if self.meta.last_modified.is_none() {
            self.meta.last_modified = fetched.last_modified;
        }
        self.meta
            .chunks
            .get_or_insert_with(Vec::new)
            .push(ChunkRecord {
                start: fetched.range.start,
                end: fetched.range.end,
                sha1: fetched.sha1,
            });
        self.save();
    }

    /// 落盘（元数据写失败不影响下载本身：续传只是加速，不是正确性前提）。
    ///
    /// **同步写**（`write_atomic`：建 tmp → 写 → `sync_all` → rename）跑在 async 任务里。判断是
    /// 「可以接受」，量级与理由（复审 r2 p3-2）：
    /// * 一个作业最多 `MAX_CONNECTIONS`(16) 段 + 建账那次 = **≤17 次**，每次 1–3 KiB（一条记录
    ///   约 80 字节）。本地 SSD 每次约 1–2 ms ⇒ 整作业 ≤ ~30 ms；网络盘上一次 rename 可能几十
    ///   ms ⇒ ≤ ~1 s，摊在「几分钟到几小时」的作业上（落完一段本来就要几十秒到几分钟），占比
    ///   1e-4 量级；
    /// * 顺序有意义：必须**先**把「这一段已落盘 + 摘要」写到盘上，才谈得上「记账可信」
    ///   （record-then-trust）。挪进 `spawn_blocking` 就得让 journal 跨 await 被拥有或加锁，
    ///   为几毫秒的 executor 占用引入一处顺序风险，不划算；
    /// * 与单连接路径同口径：那边也是在 async 里同步 `meta.save`（`transfer.rs`）。只改分块侧会
    ///   让两条路径的行为分叉。
    ///
    /// 退路：真出现 executor 饥饿（并发下载很多 + 网络盘）时，把 `record()` 整体挪进
    /// `spawn_blocking`、journal 变成 `Arc<Mutex<_>>` 是可行方向，那时再收敛。
    pub fn save(&mut self) {
        self.meta.saved_at_ms = now_ms();
        let _ = self.meta.save(&self.path);
    }
}
