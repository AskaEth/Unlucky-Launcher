//! 分块路径的续传（M293 / 台账 P21 第 2 条 + P19(b)）：真跑两次尝试，**数服务端吐了多少字节**。
//!
//! 判据刻意不用「最终 hash 对」：hash 对只说明拼出来的文件没错，抓不住「其实整份又下了一遍」——
//! 那正是 P21 那条债的形态（188MB 的 JDK 一次失败就从 0 重下）。所以每个用例都用夹具统计
//! 服务端实际发出的正文字节数（[`MockServer::bytes_sent`]）：
//!
//! * `resumes_the_finished_chunks_after_a_partial_failure`：第一次在 3/4 处失败、第二次续传完成
//!   ⇒ 两次总字节必须 **< 1.5×文件大小**。变异：把 `attempt_chunked` 改回「进来先 `discard()`、
//!   失败也 `discard()`」⇒ 第二次必然整份重下，该断言红（1.75×）。
//! * `without_resume_the_same_failure_downloads_again`：同一份夹具、同一个失败点，只把续传策略
//!   换成 `Never` ⇒ 服务端确实吐出 ~1.75×。这条是上一条的**可执行变异证明**：`< 1.5×` 这个阈值
//!   真的把「续传」和「全重下」分开了，不是随便定的一个宽阈值。
//! * `corrupted_old_chunks_are_never_stitched_in`：盘上的旧段被改过一个字节 ⇒ 一段都不许复用
//!   （摘要自证不过就整份丢掉从头再来），服务端要重新吐全部字节。
//! * `changed_size_invalidates_old_chunk_layout`：总长换了（同一个目标文件，大小变了）⇒ 旧段一段
//!   都不许用。这条钉的是「旧元数据不得被误用」。

use std::sync::Arc;
use std::time::Duration;

use unlucky_core::download::backoff::BackoffPolicy;
use unlucky_core::download::cache::ContentCache;
use unlucky_core::download::chunked::{plan, ByteRange, ChunkedConfig};
use unlucky_core::download::config::EngineConfig;
use unlucky_core::download::engine::DownloadEngine;
use unlucky_core::download::health::HealthRegistry;
use unlucky_core::download::mirrors::MirrorTable;
use unlucky_core::download::region::RegionProfile;
use unlucky_core::download::resume::{ChunkRecord, ResumeMeta};
use unlucky_core::download::types::{DownloadRequest, HashAlgo, ResourceKind, ResumePolicy};
use unlucky_core::download::verify::digest_bytes;

use super::support::{sha1_hex, temp_dir, MockResponse, MockServer};

/// 4 段 × 10 KiB。
const SIZE: usize = 40 * 1024;
/// 卡住的那一段吐这么多字节就停（这部分字节落盘了但不会被记账）。
const STALL_TAIL: usize = 1024;

fn payload() -> Vec<u8> {
    (0..SIZE).map(|index| (index % 251) as u8).collect()
}

/// 一次尝试允许 2 次（第一次故意失败、第二次续传）；读空闲 300ms：卡住的那一段自己报错，
/// 那时其余三段早就落完并被回收了（作业级的无进度窗口是 10s，不会抢在它前面）。
fn config() -> EngineConfig {
    EngineConfig {
        max_attempts_per_source: 2,
        backoff: BackoffPolicy::none(),
        connect_timeout: Duration::from_secs(2),
        response_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_millis(300),
        chunk_stall_timeout: Duration::from_secs(10),
        chunk_job_floor: Duration::from_secs(10),
        verify_timeout: Duration::from_secs(10),
        chunked: ChunkedConfig {
            enabled: true,
            connections: 4,
            min_size: 1024,
        },
        ..EngineConfig::default()
    }
}

fn engine(dir: &std::path::Path) -> DownloadEngine {
    DownloadEngine::new(
        config(),
        Arc::new(MirrorTable::builtin()),
        Arc::new(RegionProfile::direct(0)),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(dir.join("cache"))),
    )
}

fn request(url: &str, dest: &std::path::Path, payload: &[u8]) -> DownloadRequest {
    DownloadRequest::new(ResourceKind::JavaRuntime, url, dest)
        .with_sha1(sha1_hex(payload))
        .with_size(payload.len() as u64)
}

/// 第一次尝试的脚本：4 段里 3 段正常落完，第 4 个连接拿到「吐一点就卡住」的那份。
fn partial_script(payload: &[u8]) -> Vec<MockResponse> {
    let mut script: Vec<MockResponse> =
        (0..3).map(|_| MockResponse::ok(payload.to_vec())).collect();
    script.push(MockResponse::ok(payload.to_vec()).stalling_after(STALL_TAIL));
    script
}

fn record_of(payload: &[u8], range: ByteRange) -> ChunkRecord {
    ChunkRecord {
        start: range.start,
        end: range.end,
        sha1: digest_bytes(
            HashAlgo::Sha1,
            &payload[range.start as usize..=range.end as usize],
        ),
    }
}

/// 分块路径的元数据（`chunks: Some(..)` 是「这个中间文件属于分块路径」的标记）。
fn chunk_meta(url: &str, total: u64, hash: &str, chunks: Vec<ChunkRecord>) -> ResumeMeta {
    ResumeMeta {
        url: url.to_string(),
        etag: Some("\"v1\"".to_string()),
        last_modified: None,
        total: Some(total),
        algo: HashAlgo::Sha1,
        hash: hash.to_string(),
        saved_at_ms: 1,
        chunks: Some(chunks),
    }
}

/// 3/4 段落完就失败，第二次续传完成 ⇒ 服务端总共只该吐 ≈1.0×，而不是 ≈1.75×。
#[tokio::test]
async fn resumes_the_finished_chunks_after_a_partial_failure() {
    let payload = payload();
    let size = payload.len() as u64;
    let mut script = partial_script(&payload);
    // 第二次尝试只该有 1 段要补。脚本仍给足 4 条：万一实现退化成「从头下」，
    // 那一次也要能下完——这样红的就是**字节判据**本身，而不是「脚本用尽 ⇒ 500」。
    script.extend((0..4).map(|_| MockResponse::ok(payload.clone())));
    let server = MockServer::start(script).await;
    let url = server.url("/jdk.zip");
    let dir = temp_dir("chunked-resume");
    let engine = engine(&dir);
    let dest = dir.join("jdk.zip");

    let outcome = engine
        .download(request(&url, &dest, &payload))
        .await
        .expect("续传后应当成功");
    assert_eq!(std::fs::read(&dest).expect("读回落盘文件失败"), payload);

    // 主判据：服务端总共吐了多少字节（「最终 hash 对」抓不到「其实全重下了」）。
    let sent = server.bytes_sent();
    assert!(
        sent < size * 3 / 2,
        "两次尝试总共只该吐 ≈1.0× 字节，实得 {sent}（1.5×{size} = {}）：\
         超过它说明第二次又把整份下了一遍",
        size * 3 / 2
    );
    assert_eq!(server.hits(), 5, "第一次 4 段 + 第二次补 1 段");
    assert_eq!(
        outcome.resumed_from,
        size * 3 / 4,
        "3/4 的段应当从磁盘复用（剩下 1 段重抓）"
    );
    server.stop();
}

/// 上一条的变异证明：同一个失败点，只把续传关掉 ⇒ 两次尝试一共吐 ~1.75×。
///
/// 把实现改回「失败就 discard、下次从头下」时，上一条的 `< 1.5×` 会落到本用例的区间 ⇒ 必红。
#[tokio::test]
async fn without_resume_the_same_failure_downloads_again() {
    let payload = payload();
    let size = payload.len() as u64;
    let mut script = partial_script(&payload);
    // 第二次尝试要是从头下：4 段全部重抓。
    script.extend((0..4).map(|_| MockResponse::ok(payload.clone())));
    let server = MockServer::start(script).await;
    let url = server.url("/jdk.zip");
    let dir = temp_dir("chunked-noresume");
    let engine = engine(&dir);
    let dest = dir.join("jdk.zip");

    let outcome = engine
        .download(request(&url, &dest, &payload).with_resume(ResumePolicy::Never))
        .await
        .expect("不续传也要能下完");
    assert_eq!(std::fs::read(&dest).expect("读回落盘文件失败"), payload);
    assert_eq!(outcome.resumed_from, 0, "策略 Never ⇒ 一个字节都不复用");

    let sent = server.bytes_sent();
    assert!(
        sent > size * 3 / 2,
        "不续传时服务端要吐出两遍的字节（实测 {sent}，应当 > {}）——\
         这正是 P21 那条债的形态",
        size * 3 / 2
    );
    assert_eq!(server.hits(), 8, "第一次 4 段 + 第二次整份 4 段");
    server.stop();
}

/// 盘上的旧段被改过：不许把「记录里说好的字节」当成「盘上现在就是好的」直接拼进结果。
#[tokio::test]
async fn corrupted_old_chunks_are_never_stitched_in() {
    let payload = payload();
    let size = payload.len() as u64;
    let mut script: Vec<MockResponse> = (0..4).map(|_| MockResponse::ok(payload.clone())).collect();
    // 万一实现真去复用坏段，它还要再整份重下一遍才有机会成功（8 条够两种世界都下完）。
    script.extend((0..4).map(|_| MockResponse::ok(payload.clone())));
    let server = MockServer::start(script).await;
    let url = server.url("/jdk.zip");
    let dir = temp_dir("chunked-broken-part");
    let engine = engine(&dir);
    let dest = dir.join("jdk.zip");
    let ranges = plan(size, 4);

    // 旧版无主命名的中间文件（`PartFile::for_dest` 会收养它）。
    let part = dir.join("jdk.zip.ullpart");
    let meta_path = dir.join("jdk.zip.ullpart.ullmeta.json");
    let mut on_disk = payload.clone();
    on_disk[ranges[1].start as usize + 5] ^= 0xff;
    std::fs::write(&part, &on_disk).expect("写旧中间文件失败");
    // 元数据声称前 3 段都落完了（记的是**原始**字节的摘要），只有第 4 段没记。
    chunk_meta(
        &url,
        size,
        &sha1_hex(&payload),
        ranges[..3]
            .iter()
            .map(|range| record_of(&payload, *range))
            .collect(),
    )
    .save(&meta_path)
    .expect("写旧元数据失败");

    let outcome = engine
        .download(request(&url, &dest, &payload))
        .await
        .expect("坏旧段被丢掉后应当重下成功");
    assert_eq!(std::fs::read(&dest).expect("读回落盘文件失败"), payload);
    // 主判据：坏段自证不过 ⇒ 整份丢掉从头来，服务端只该吐这一份文件的字节。
    assert_eq!(
        server.bytes_sent(),
        size,
        "全部字节都要重新抓一遍（复用坏字节 = 交付坏文件）"
    );
    assert_eq!(server.hits(), 4, "从头下 = 4 个分块请求");
    assert_eq!(
        outcome.resumed_from, 0,
        "任何一段对不上就整份丢掉，一段都不复用"
    );
    server.stop();
}

/// 同一个目标文件换了大小：旧的 2 段元数据**恰好**对得上新规划的前两段，只有「总长变了」能拦住它。
#[tokio::test]
async fn changed_size_invalidates_old_chunk_layout() {
    let payload = payload();
    let size = payload.len() as u64;
    let mut script: Vec<MockResponse> = (0..4).map(|_| MockResponse::ok(payload.clone())).collect();
    // 真去复用旧段时，它还要再整份重下一遍才有机会成功（8 条够两种世界都下完）。
    script.extend((0..4).map(|_| MockResponse::ok(payload.clone())));
    let server = MockServer::start(script).await;
    let url = server.url("/jdk.zip");
    let dir = temp_dir("chunked-size-change");
    let engine = engine(&dir);
    let dest = dir.join("jdk.zip");

    // 旧中间文件 = 上一次「同一个目标、一半大小」的下载残留（内容是**另一个版本**的字节）。
    let old_size = SIZE / 2;
    let old_bytes = vec![0xEE_u8; old_size];
    let part = dir.join("jdk.zip.ullpart");
    let meta_path = dir.join("jdk.zip.ullpart.ullmeta.json");
    std::fs::write(&part, &old_bytes).expect("写旧中间文件失败");
    // 2 段 × 10KiB：段区间与新规划的 [0-10239] / [10240-20479] 完全重合，所以
    // 「段对不上号」这条拦不住它，只有总长比较能拦（变异：删掉总长比较 ⇒ 本用例红：
    // 复用 20480 字节 + 整文件校验失败 ⇒ 白下一次 20480 字节）。
    chunk_meta(
        &url,
        old_size as u64,
        &sha1_hex(&payload),
        plan(old_size as u64, 2)
            .iter()
            .map(|range| record_of(&old_bytes, *range))
            .collect(),
    )
    .save(&meta_path)
    .expect("写旧元数据失败");

    let outcome = engine
        .download(request(&url, &dest, &payload))
        .await
        .expect("大小变了以后必须从头重下成功");
    assert_eq!(std::fs::read(&dest).expect("读回落盘文件失败"), payload);
    // 主判据：旧文件的那 20480 字节一个都不许用（误用 ⇒ 这次会吐出 1.5×）。
    assert_eq!(
        server.bytes_sent(),
        size,
        "旧的那 20480 字节一个都不许用：只允许下这次的 {size} 字节"
    );
    assert_eq!(outcome.resumed_from, 0, "总长变了 ⇒ 旧段偏移全部作废");
    server.stop();
}

/// 同一个 URL、**换了一份表示**（复审 r2 p2-1）：上一份表示的校验器不许搬进这一轮。
///
/// 链条（全走既有代码）：上一轮留下 `url=U, 旧总长/旧段区间, etag="v1"`（`chunk_meta` 固定写
/// `"v1"`）⇒ 本轮因 `HashChanged` 判 `Restart`、`part.discard()`，但 `previous` 还在内存里 ⇒
/// 若把 `"v1"` 写进**新一轮**元数据，而 `record` 又只在缺值时写，那本轮落的 3 段（**新**内容）
/// 配的是上一份表示的校验器 ⇒ 重试拿 `"v1"` 发 `If-Range` ⇒ 此刻服务器是 `"v2"`，按 RFC 7233
/// 回 200 + 完整正文 ⇒ `RangeUnsupported` ⇒ 丢整份 ⇒ 单连接从头下。
/// 实测那是 113664 字节（2.775×），比「干脆不续传」的 72704（1.775×）还多一整份。
///
/// 修法：`carry = if done.is_empty() { None } else { same_source }` —— `done` 为空就没有任何
/// 字节要接着用，旧校验器描述的不是这次要下的东西；本轮响应里的真实校验器由 `record` 填进来。
///
/// 断言：重试带的 `If-Range` 必须是本轮响应给的 `"v2"`（出现 `"v1"` 即串台），且字节数落回
/// 41984（1.025×）而不是 113664。
#[tokio::test]
async fn stale_validator_from_the_previous_representation_is_not_carried() {
    /// 服务器此刻那份表示的 ETag（旧元数据里的 `"v1"` 是上一份表示的）。
    const ETAG_NEW: &str = "\"v2\"";

    let payload = payload();
    let size = payload.len() as u64;
    // 4 段：3 段成功 + 1 段卡住；重试只该补 1 段；后面 4 条给「退化成单连接整份重下」兜底
    // （那种坏结局也要能下完，红的是判据不是脚本用尽）。
    let fresh = |body: Vec<u8>| MockResponse::ok(body).with_etag(ETAG_NEW);
    let mut script: Vec<MockResponse> = (0..3).map(|_| fresh(payload.clone())).collect();
    script.push(fresh(payload.clone()).stalling_after(STALL_TAIL));
    script.push(fresh(payload.clone()));
    script.extend((0..4).map(|_| fresh(payload.clone())));
    let server = MockServer::start(script).await;
    let url = server.url("/jdk.zip");
    let dir = temp_dir("chunked-stale-validator");
    let engine = engine(&dir);
    let dest = dir.join("jdk.zip");

    // 上一轮留下的中间文件 + 元数据：同一个 URL，但描述的是另一份表示（旧总长 / 旧段区间 /
    // 旧 ETag / 旧摘要）。
    let old_size = SIZE / 2;
    let old_bytes = vec![0xEE_u8; old_size];
    let part = dir.join("jdk.zip.ullpart");
    let meta_path = dir.join("jdk.zip.ullpart.ullmeta.json");
    std::fs::write(&part, &old_bytes).expect("写旧中间文件失败");
    chunk_meta(
        &url,
        old_size as u64,
        &sha1_hex(&old_bytes),
        plan(old_size as u64, 2)
            .iter()
            .map(|range| record_of(&old_bytes, *range))
            .collect(),
    )
    .save(&meta_path)
    .expect("写旧元数据失败");

    let outcome = engine
        .download(request(&url, &dest, &payload))
        .await
        .expect("换了表示之后仍应能下完");
    assert_eq!(std::fs::read(&dest).expect("读回落盘文件失败"), payload);
    assert_eq!(
        server.if_range_values(),
        vec![ETAG_NEW.to_string()],
        "重试只许带本轮响应给的校验器；出现 \"v1\" 说明上一份表示的校验器被搬进了这一轮"
    );
    // 3 段 + 卡住的 1KiB + 补 1 段 = 整份文件 + 1KiB = 41984（1.025×）。
    assert_eq!(
        server.bytes_sent(),
        size + STALL_TAIL as u64,
        "应当是 41984（1.025×）；113664（2.775×）说明陈旧校验器把整份顶回来又重下了一遍"
    );
    assert_eq!(server.hits(), 5, "第一次 4 段 + 第二次补 1 段");
    assert_eq!(outcome.resumed_from, size * 3 / 4, "复用了本轮的 3 段");
    server.stop();
}
