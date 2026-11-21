//! 弱校验器（`Last-Modified`）驱动 `If-Range` 的那条支路（复审 r2 p3-1）。
//!
//! 分块路径此前只在 **ETag** 上端到端覆盖过条件请求；`Validator::if_range_value()` 的兜底
//! （ETag 缺失 ⇒ 用 Last-Modified）与 `ChunkJournal::record` 写 `last_modified` 这两条路，只有
//! 单连接的纯函数用例覆盖（`resume_test.rs` 的 `missing_validator_restarts`），分块侧零覆盖。
//!
//! 这里补上：服务器**只给** `Last-Modified`（不给 ETag），3/4 段失败后重试必须带这个 HTTP-date
//! 发 `If-Range`，夹具按弱校验器比对相符 ⇒ 206 ⇒ 只补 1 段（41984 = 1.025×），而不是整份重下。
//!
//! 抓得住：分块路径「只有弱校验器时也能续传」这条端到端行为（含 `record` 写 `last_modified`、
//! `validator()` 兜底、夹具的 HTTP-date 比对）。
//! 抓不住：真实服务器对 HTTP-date 型 `If-Range` 的**强/弱比较语义**（RFC 7233 §3.2 规定只能用
//! 强校验器做 If-Range，HTTP-date 允许以 60s 粒度做「没变过」判定）——夹具只做字符串相等，这里
//! 证明的是「我们自己这条链路会把日期原样送进 If-Range 并接受 206」，不是「所有真实服务器都会」。
//! 若服务器判「不符」，按既有路径会回 200 ⇒ `RangeUnsupported` ⇒ 干净重下（正确性不受影响）。

use std::sync::Arc;
use std::time::Duration;

use unlucky_core::download::backoff::BackoffPolicy;
use unlucky_core::download::cache::ContentCache;
use unlucky_core::download::chunked::ChunkedConfig;
use unlucky_core::download::config::EngineConfig;
use unlucky_core::download::engine::DownloadEngine;
use unlucky_core::download::health::HealthRegistry;
use unlucky_core::download::mirrors::MirrorTable;
use unlucky_core::download::region::RegionProfile;
use unlucky_core::download::types::{DownloadRequest, ResourceKind};

use super::support::{sha1_hex, temp_dir, MockResponse, MockServer};

/// 4 段 × 10 KiB。
const SIZE: usize = 40 * 1024;
/// 卡住的那一段吐这么多字节就停。
const STALL_TAIL: usize = 1024;
/// 服务器唯一的校验器（HTTP-date 原文，客户端会原样放进 `If-Range`）。
const LAST_MODIFIED: &str = "Wed, 21 Oct 2015 07:28:00 GMT";

fn payload() -> Vec<u8> {
    (0..SIZE).map(|index| (index % 251) as u8).collect()
}

fn engine(dir: &std::path::Path) -> DownloadEngine {
    DownloadEngine::new(
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
        },
        Arc::new(MirrorTable::builtin()),
        Arc::new(RegionProfile::direct(0)),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(dir.join("cache"))),
    )
}

#[tokio::test]
async fn last_modified_alone_can_drive_if_range_on_the_chunked_path() {
    let payload = payload();
    let size = payload.len() as u64;
    // 只给弱校验器：没有 ETag，只有 Last-Modified。
    let response = |body: Vec<u8>| {
        MockResponse::ok(body)
            .without_etag()
            .with_last_modified(LAST_MODIFIED)
    };
    // 4 段里 3 段成功、1 段卡住；重试只该补 1 段；后面 4 条给「退化成整份重下」兜底。
    let mut script: Vec<MockResponse> = (0..3).map(|_| response(payload.clone())).collect();
    script.push(response(payload.clone()).stalling_after(STALL_TAIL));
    script.push(response(payload.clone()));
    script.extend((0..4).map(|_| response(payload.clone())));
    let server = MockServer::start(script).await;
    let url = server.url("/jdk.zip");
    let dir = temp_dir("chunked-last-modified");
    let engine = engine(&dir);
    let dest = dir.join("jdk.zip");

    let outcome = engine
        .download(
            DownloadRequest::new(ResourceKind::JavaRuntime, url.as_str(), &dest)
                .with_sha1(sha1_hex(&payload))
                .with_size(size),
        )
        .await
        .expect("只有弱校验器时也要能续传完成");
    assert_eq!(std::fs::read(&dest).expect("读回落盘文件失败"), payload);

    // 主判据：重试那一次带的是 **Last-Modified**（HTTP-date），而且只有那一次有条件请求。
    let seen = server.if_range_values();
    assert_eq!(
        seen.len(),
        1,
        "只该在重试那一次发条件请求（首轮没有可续的前缀）：{seen:?}"
    );
    assert_eq!(
        seen[0].to_ascii_lowercase(),
        LAST_MODIFIED.to_ascii_lowercase(),
        "ETag 缺失时 `If-Range` 必须兜底用 Last-Modified，实得 {:?}",
        seen[0]
    );
    // 3 段 + 卡住的 1KiB + 补 1 段 = 整份 + 1KiB = 41984（1.025×）。
    assert_eq!(
        server.bytes_sent(),
        size + STALL_TAIL as u64,
        "弱校验器那一路也该只补 1 段（1.025×），不是整份重下"
    );
    assert_eq!(server.hits(), 5, "第一次 4 段 + 第二次补 1 段");
    assert_eq!(outcome.resumed_from, size * 3 / 4, "复用了 3 段");
    server.stop();
}
