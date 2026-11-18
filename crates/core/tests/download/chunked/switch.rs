//! 换源（复审 r1 **p2-1**）：源 A 的校验器**不许**被带到源 B 上。
//!
//! 复审发现的链条：A 落 3/4 段后失败 ⇒ 元数据 = `url=A` + A 的 ETag；引擎换到 B ⇒ `decide` 判
//! `UrlChanged`、删盘重来，但 `ChunkJournal::begin` 把 **A 的校验器**原样写进 `url=B` 的新元数据；
//! `record` 又只在 `etag.is_none()` 时写入 ⇒ B 的真实校验器永远盖不上。于是 B 的重试拿 A 的 ETag
//! 发 `If-Range`：按 RFC 7233 §3.2，服务器对不上就回 200 + 完整正文 ⇒ 分块路径判 `RangeUnsupported`
//! ⇒ 丢掉整份 ⇒ 单连接从头下。债务以另一条路径复现，正是本 mission 要消灭的「从 0 重下」。
//!
//! 修法 = **校验器与记下它的 URL 绑定**（复审给的选项 A）：只有 `previous.url == candidate.url`
//! 才沿用，否则当「没有校验器」，让新源自己的响应去填（`ChunkJournal::record`）。
//!
//! 判据都看**线上行为**，不看实现细节：
//! * 源 B 收到的每一个 `If-Range` 都必须是 B 自己的 ETag（A 的一个都不许出现）；
//! * 源 B 自己的字节数仍是 1.025×（换源之后 B 的同源续传照走，没被这一修打回「整份重下」）；
//! * 最终文件逐字节正确。
//!
//! 变异（把 URL 过滤去掉）⇒ 第一条红：`if_range_values` 变成 A 的 `"src-a"`，且夹具按 RFC 7233
//! 回 200 ⇒ 分块判 `RangeUnsupported` ⇒ 退化单连接 ⇒ 字节数也涨（红原文见 mission 报告）。

use std::sync::Arc;
use std::time::Duration;

use unlucky_core::download::backoff::BackoffPolicy;
use unlucky_core::download::cache::ContentCache;
use unlucky_core::download::chunked::ChunkedConfig;
use unlucky_core::download::config::EngineConfig;
use unlucky_core::download::engine::DownloadEngine;
use unlucky_core::download::health::HealthRegistry;
use unlucky_core::download::mirrors::{MirrorTable, PrefixRule};
use unlucky_core::download::region::RegionProfile;
use unlucky_core::download::types::{DownloadRequest, ResourceKind};

use super::support::{sha1_hex, temp_dir, MockResponse, MockServer};

/// 4 段 × 10 KiB。
const SIZE: usize = 40 * 1024;
const CHUNK: u64 = 10 * 1024;
/// 卡住的那一段吐这么多字节就停（落盘但不会被记账）。
const STALL_TAIL: usize = 1024;
/// 两台「服务器」各自的校验器：故意取不同的值，任何串台都能一眼看出来。
const ETAG_A: &str = "\"src-a\"";
const ETAG_B: &str = "\"src-b\"";

fn payload() -> Vec<u8> {
    (0..SIZE).map(|index| (index % 251) as u8).collect()
}

/// 一次尝试允许 2 次：源 A 用完 2 次出局 → 换源 B；源 B 也是 2 次（失败一次 + 续传一次）。
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

/// 「3 段成功 + 1 段卡住」的 4 条脚本（等于一次部分失败的分块尝试）。
fn partial(etag: &str, payload: &[u8]) -> Vec<MockResponse> {
    let mut script: Vec<MockResponse> = (0..3)
        .map(|_| MockResponse::ok(payload.to_vec()).with_etag(etag))
        .collect();
    script.push(
        MockResponse::ok(payload.to_vec())
            .with_etag(etag)
            .stalling_after(STALL_TAIL),
    );
    script
}

#[tokio::test]
async fn switching_source_never_reuses_the_previous_source_validator() {
    let payload = payload();
    let size = payload.len() as u64;

    // 源 A：部分失败一次，同源续传那一次直接 500 ⇒ 源 A 出局（引擎按候选顺序换到 B）。
    let mut script_a = partial(ETAG_A, &payload);
    script_a.push(MockResponse::status(500));
    let server_a = MockServer::start(script_a).await;

    // 源 B：同样部分失败一次（于是 B 有自己的已落盘分块），续传那一段就该带 **B 自己的** 校验器。
    // 后面多给 4 条：万一实现退化成「整份重下」，那次也要能下完——红的是判据，而不是脚本用尽。
    let mut script_b = partial(ETAG_B, &payload);
    script_b.push(MockResponse::ok(payload.clone()).with_etag(ETAG_B));
    script_b.extend((0..4).map(|_| MockResponse::ok(payload.clone()).with_etag(ETAG_B)));
    let server_b = MockServer::start(script_b).await;

    // 两台服务器当两个候选源：官方 = A，镜像规则把前缀改写成 B。
    let url = server_a.url("/jdk.zip");
    let mut table = MirrorTable::builtin();
    table.add_custom_rule(PrefixRule::new(
        "test-mirror",
        format!("{}/", server_a.url("")),
        format!("{}/", server_b.url("")),
    ));
    let dir = temp_dir("chunked-switch");
    let engine = DownloadEngine::new(
        config(),
        Arc::new(table),
        Arc::new(RegionProfile::direct(0)),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(dir.join("cache"))),
    );
    let dest = dir.join("jdk.zip");

    let outcome = engine
        .download(
            DownloadRequest::new(ResourceKind::JavaRuntime, url.as_str(), &dest)
                .with_sha1(sha1_hex(&payload))
                .with_size(size),
        )
        .await
        .expect("换源后应当能下完");

    assert_eq!(
        std::fs::read(&dest).expect("读回落盘文件失败"),
        payload,
        "换源 + 续传之后文件必须逐字节正确"
    );
    // 主判据：源 B 只许看到**它自己**的校验器。若这里出现 `"src-a"`，就是复审 r1 p2-1 的债
    // 从另一条路回来了（真实服务器会对不上回 200 ⇒ 丢掉整份 ⇒ 单连接从头下）。
    assert_eq!(
        server_b.if_range_values(),
        vec![ETAG_B.to_string()],
        "源 B 收到了不属于它的校验器：换源后旧源的 ETag 被带过来了"
    );
    // 对照：源 A 的同源续传本来就该带自己的校验器（证明这个观测点不是恒空）。
    assert_eq!(
        server_a.if_range_values(),
        vec![ETAG_A.to_string()],
        "源 A 的同源续传应当带 A 自己的校验器"
    );
    // 换源之后 B 自己的那一轮仍是「同源续传」：3 段 + 1KiB + 补 1 段 = 1.025×。
    assert_eq!(
        server_b.bytes_sent(),
        3 * CHUNK + STALL_TAIL as u64 + CHUNK,
        "源 B 的字节数应当是 1.025×（3 段 + 卡住的 1KiB + 补 1 段），不是整份重下"
    );
    assert_eq!(server_b.hits(), 5, "B：一次部分失败（4 请求）+ 一次补 1 段");
    assert_eq!(
        outcome.resumed_from,
        3 * CHUNK,
        "最终那一次复用了 3 段（补抓 1 段），不是从头下"
    );

    server_a.stop();
    server_b.stop();
}

/// 同一个换源场景，但**新源自己没有任何校验器**（复审 r1 建议的那条用例）。
///
/// 语义是刻意的保守：换了源、新源又不给 ETag/Last-Modified ⇒ 我们无法让服务器回答「内容变过
/// 没有」，`chunk_resume::decide` 判 `NoValidator` ⇒ 新源那次重试**整份重下**，不复用自己刚
/// 落的 3 段。代价（多下一份）换的是「绝不拿无法证明的旧字节去拼文件」，与本模块的立场一致。
///
/// 这条同时是「修法不要退化成沿用旧校验器」的守卫：只要有人把 `same_source` 过滤去掉、或把
/// `record` 改成无条件覆盖，B 的元数据就会带上 A 的 `"src-a"` ⇒ 这里会出现 `If-Range` ⇒ 红。
#[tokio::test]
async fn switched_source_without_its_own_validator_restarts_from_zero() {
    let payload = payload();
    let size = payload.len() as u64;

    // 源 A：有 ETag，部分失败后同源续传吃 500 ⇒ 出局。
    let mut script_a = partial(ETAG_A, &payload);
    script_a.push(MockResponse::status(500));
    let server_a = MockServer::start(script_a).await;

    // 源 B：**完全不发 ETag**。部分失败一次（于是 B 有 3 段已落盘），重试时不许复用。
    let mut script_b: Vec<MockResponse> = (0..3)
        .map(|_| MockResponse::ok(payload.clone()).without_etag())
        .collect();
    script_b.push(
        MockResponse::ok(payload.clone())
            .without_etag()
            .stalling_after(STALL_TAIL),
    );
    script_b.extend((0..4).map(|_| MockResponse::ok(payload.clone()).without_etag()));
    let server_b = MockServer::start(script_b).await;

    let url = server_a.url("/jdk.zip");
    let mut table = MirrorTable::builtin();
    table.add_custom_rule(PrefixRule::new(
        "test-mirror",
        format!("{}/", server_a.url("")),
        format!("{}/", server_b.url("")),
    ));
    let dir = temp_dir("chunked-switch-novalidator");
    let engine = DownloadEngine::new(
        config(),
        Arc::new(table),
        Arc::new(RegionProfile::direct(0)),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(dir.join("cache"))),
    );
    let dest = dir.join("jdk.zip");

    let outcome = engine
        .download(
            DownloadRequest::new(ResourceKind::JavaRuntime, url.as_str(), &dest)
                .with_sha1(sha1_hex(&payload))
                .with_size(size),
        )
        .await
        .expect("新源没有校验器时也要能下完（宁可重下）");

    assert_eq!(std::fs::read(&dest).expect("读回落盘文件失败"), payload);
    assert!(
        server_b.if_range_values().is_empty(),
        "新源没有任何校验器，就不许发条件请求（带 A 的值更是串源）：{:?}",
        server_b.if_range_values()
    );
    assert_eq!(
        server_b.bytes_sent(),
        3 * CHUNK + STALL_TAIL as u64 + 4 * CHUNK,
        "B 的第二次尝试是整份重下（无法证明旧字节 ⇒ 不复用）"
    );
    assert_eq!(server_b.hits(), 8, "B：部分失败 4 请求 + 整份重下 4 请求");
    assert_eq!(outcome.resumed_from, 0, "最终那一次从头下");

    server_a.stop();
    server_b.stop();
}
