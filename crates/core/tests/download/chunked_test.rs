//! 分块规划与 Range 语义的单测（纯函数）+ 真跑 4 连接的端到端用例（mock 源）。

mod support;

use std::sync::Arc;
use std::time::Duration;

use unlucky_core::download::backoff::BackoffPolicy;
use unlucky_core::download::cache::ContentCache;
use unlucky_core::download::chunked::{
    content_range_matches, parse_content_range, plan, range_support, ByteRange, ChunkedConfig,
    ContentRange, RangeSupport, MAX_CONNECTIONS,
};
use unlucky_core::download::config::EngineConfig;
use unlucky_core::download::engine::DownloadEngine;
use unlucky_core::download::health::HealthRegistry;
use unlucky_core::download::mirrors::MirrorTable;
use unlucky_core::download::region::RegionProfile;
use unlucky_core::download::types::{DownloadRequest, ResourceKind};

use support::{sha1_hex, temp_dir, MockResponse, MockServer};

#[test]
fn plan_partitions_whole_file_without_gaps() {
    let cases = [
        (1_u64, 4_usize),
        (3, 4),
        (100, 4),
        (1000, 7),
        (123_456_789, 4),
    ];
    for (total, connections) in cases {
        let ranges = plan(total, connections);
        assert!(!ranges.is_empty(), "total={total} 不能切出空集");
        assert_eq!(ranges[0].start, 0, "必须从 0 开始");
        assert_eq!(ranges.last().unwrap().end, total - 1, "必须覆盖到末尾");
        let mut next = 0_u64;
        for range in &ranges {
            assert_eq!(range.start, next, "段之间不能有空洞或重叠");
            assert!(!range.is_empty());
            next = range.end + 1;
        }
        assert_eq!(next, total);
        assert_eq!(ranges.iter().map(ByteRange::len).sum::<u64>(), total);
    }
}

#[test]
fn plan_caps_connections_and_handles_edge_cases() {
    assert!(plan(0, 4).is_empty());
    // 文件比连接数还小：每段 1 字节。
    let tiny = plan(3, 4);
    assert_eq!(tiny.len(), 3);
    assert!(tiny.iter().all(|range| range.len() == 1));
    // 连接数被夹到上限。
    let many = plan(1_000_000, 100);
    assert_eq!(many.len(), MAX_CONNECTIONS);
    // 单连接。
    assert_eq!(plan(120, 1), vec![ByteRange::new(0, 119)]);
}

#[test]
fn byte_range_headers_are_closed_intervals() {
    let range = ByteRange::new(100, 199);
    assert_eq!(range.len(), 100);
    assert_eq!(range.header_value(), "bytes=100-199");
    assert!(ByteRange::new(10, 9).is_empty());
    assert_eq!(ByteRange::new(10, 9).len(), 0);
}

#[test]
fn parse_content_range_accepts_servers_forms() {
    assert_eq!(
        parse_content_range("bytes 0-99/1000"),
        Some(ContentRange {
            start: 0,
            end: 99,
            total: Some(1000)
        })
    );
    assert_eq!(
        parse_content_range("  bytes 100-199/*  "),
        Some(ContentRange {
            start: 100,
            end: 199,
            total: None
        })
    );
    // 畸形一律 None：调用方据此退化。
    assert_eq!(parse_content_range("bytes 100-99/1000"), None);
    assert_eq!(parse_content_range("items 0-9/10"), None);
    assert_eq!(parse_content_range("bytes 0-9"), None);
    assert_eq!(parse_content_range(""), None);
}

#[test]
fn range_support_matrix() {
    assert_eq!(range_support(Some("bytes"), 200), RangeSupport::Supported);
    assert_eq!(range_support(None, 206), RangeSupport::Supported);
    assert_eq!(range_support(Some("none"), 200), RangeSupport::Unsupported);
    assert_eq!(
        range_support(Some("crumbs"), 200),
        RangeSupport::Unsupported
    );
    assert_eq!(range_support(None, 200), RangeSupport::Unknown);
}

#[test]
fn chunked_config_gate() {
    let config = ChunkedConfig::default();
    assert!(config.enabled);
    assert_eq!(config.connections, 4);
    // 只有大文件 + 大资源类型才分块。
    assert!(config.applies_to(ResourceKind::JavaRuntime, Some(200 * 1024 * 1024)));
    assert!(!config.applies_to(ResourceKind::JavaRuntime, Some(1024)));
    assert!(!config.applies_to(ResourceKind::ClientJar, Some(200 * 1024 * 1024)));
    assert!(!config.applies_to(ResourceKind::JavaRuntime, None));
    // 设置项关掉后一律不分块。
    assert!(!ChunkedConfig::disabled().applies_to(ResourceKind::JavaRuntime, Some(u64::MAX)));
    assert_eq!(
        ChunkedConfig {
            connections: 999,
            ..ChunkedConfig::default()
        }
        .effective_connections(),
        MAX_CONNECTIONS
    );
}

/// 端到端：`min_size` 调到 1KiB 后，20KiB 的 JDK 类资源真的被切成 4 段并行取。
///
/// 断言三件事：①mock 源收到 4 次带 `Range` 的请求（不是 1 次）②拼装后的文件逐字节正确
/// ③流式 sha1 校验通过（证明分块乱序写没有错位）。
#[tokio::test]
async fn downloads_large_file_over_four_parallel_ranges() {
    let payload: Vec<u8> = (0..20 * 1024).map(|index| (index % 251) as u8).collect();
    let server = MockServer::start(vec![
        MockResponse::ok(payload.clone()),
        MockResponse::ok(payload.clone()),
        MockResponse::ok(payload.clone()),
        MockResponse::ok(payload.clone()),
    ])
    .await;
    let url = server.url("/jdk.zip");
    let dir = temp_dir("chunked-e2e");
    let engine = DownloadEngine::new(
        EngineConfig {
            max_attempts_per_source: 1,
            backoff: BackoffPolicy {
                base_ms: 1,
                max_ms: 2,
                jitter_ratio: 0.0,
                max_attempts: 1,
            },
            read_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(5),
            // 真开分块：4 条连接，1KiB 以上就切。
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
    );

    let dest = dir.join("jdk.zip");
    let outcome = engine
        .download(
            DownloadRequest::new(ResourceKind::JavaRuntime, url.as_str(), &dest)
                .with_sha1(sha1_hex(&payload))
                .with_size(payload.len() as u64),
        )
        .await
        .expect("分块下载失败");

    assert_eq!(outcome.bytes, payload.len() as u64);
    assert_eq!(outcome.source, "official");
    assert_eq!(
        server.hits(),
        4,
        "4 条连接应各发一次带 Range 的请求（退化单连接会只有 1 次）"
    );
    assert_eq!(std::fs::read(&dest).expect("读回落盘文件失败"), payload);
    assert!(
        support::part_files(&dir).is_empty(),
        "中间文件必须在提交后消失：{:?}",
        support::part_files(&dir)
    );
    server.stop();
}

/// 端到端：目标目录**还不存在**时分块下载也要能落盘。
///
/// 回归用例。分块路径先 `OpenOptions` 预分配整文件（不走 `PartFile::open_append`
/// 那层建目录），所以「下到新建目录」曾让所有候选源瞬间全部失败
/// （现场：`data/java-managed/.cache/` 未创建时装 JDK 六个源全挂）。
#[tokio::test]
async fn chunked_download_creates_the_destination_directory() {
    let payload: Vec<u8> = (0..8 * 1024).map(|index| (index % 31) as u8).collect();
    let responses = vec![
        MockResponse::ok(payload.clone()),
        MockResponse::ok(payload.clone()),
        MockResponse::ok(payload.clone()),
        MockResponse::ok(payload.clone()),
    ];
    let server = MockServer::start(responses).await;
    let url = server.url("/jdk.zip");
    let dir = temp_dir("chunked-mkdir");
    let engine = DownloadEngine::new(
        EngineConfig {
            max_attempts_per_source: 1,
            backoff: BackoffPolicy {
                base_ms: 1,
                max_ms: 2,
                jitter_ratio: 0.0,
                max_attempts: 1,
            },
            read_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(5),
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
    );

    let dest = dir.join("java-managed/.cache/jdk.zip");
    assert!(
        !dest.parent().expect("父目录").exists(),
        "前置条件：目录不存在"
    );
    let outcome = engine
        .download(
            DownloadRequest::new(ResourceKind::JavaRuntime, url.as_str(), &dest)
                .with_sha1(sha1_hex(&payload))
                .with_size(payload.len() as u64),
        )
        .await
        .expect("分块下载应成功");
    assert_eq!(outcome.bytes, payload.len() as u64);
    assert_eq!(std::fs::read(&dest).expect("读回落盘文件失败"), payload);
    server.stop();
}

/// 端到端：服务器声明支持 Range 但实际无视（返回 200）→ 自动退化单连接，结果仍然正确。
#[tokio::test]
async fn falls_back_to_single_connection_when_server_ignores_range() {
    let payload: Vec<u8> = (0..8 * 1024).map(|index| (index % 97) as u8).collect();
    // 每个请求都会拿到 200 + 整份内容（range_support = false）。
    // 响应条数给足：4 个分块请求可能都发出去才被 abort，退化后的整份请求是第 5 条。
    let no_range = || MockResponse::ok(payload.clone()).without_range();
    let server = MockServer::start(vec![
        no_range(),
        no_range(),
        no_range(),
        no_range(),
        no_range(),
        no_range(),
    ])
    .await;
    let url = server.url("/client.jar");
    let dir = temp_dir("chunked-fallback");
    let engine = DownloadEngine::new(
        EngineConfig {
            max_attempts_per_source: 1,
            backoff: BackoffPolicy::none(),
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
    );

    let dest = dir.join("client.jar");
    let outcome = engine
        .download(
            DownloadRequest::new(ResourceKind::JavaRuntime, url.as_str(), &dest)
                .with_sha1(sha1_hex(&payload))
                .with_size(payload.len() as u64),
        )
        .await
        .expect("退化后应仍能下载成功");

    assert_eq!(outcome.bytes, payload.len() as u64);
    assert_eq!(std::fs::read(&dest).expect("读回落盘文件失败"), payload);
    server.stop();
}

#[test]
fn content_range_must_match_requested_segment() {
    let requested = ByteRange::new(5120, 10239);
    // 严格一致才放行。
    assert!(content_range_matches(
        requested,
        Some("bytes 5120-10239/20480")
    ));
    // 起止任一不符 → 判失败（否则「长度恰好但位置错」的响应会拼出坏文件）。
    assert!(!content_range_matches(
        requested,
        Some("bytes 0-5119/20480")
    ));
    assert!(!content_range_matches(
        requested,
        Some("bytes 5120-20479/20480")
    ));
    // 畸形头判失败。
    assert!(!content_range_matches(requested, Some("garbage")));
    // 缺头从宽（长度校验仍兜底）。
    assert!(content_range_matches(requested, None));
}

/// 端到端：服务器回 206 但 `Content-Range` 谎报段位置 → 分块路径判不符、丢弃中间文件、
/// 退化单连接重下，最终文件仍逐字节正确（不会交付拼错的文件）。
#[tokio::test]
async fn rejects_lying_content_range_and_falls_back() {
    let payload: Vec<u8> = (0..8 * 1024).map(|index| (index % 89) as u8).collect();
    let lying = || MockResponse::ok(payload.clone()).with_wrong_content_range();
    let server = MockServer::start(vec![
        lying(),
        lying(),
        lying(),
        lying(),
        MockResponse::ok(payload.clone()),
    ])
    .await;
    let url = server.url("/jdk.zip");
    let dir = temp_dir("chunked-lying-range");
    let engine = DownloadEngine::new(
        EngineConfig {
            max_attempts_per_source: 1,
            backoff: BackoffPolicy::none(),
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
    );

    let dest = dir.join("jdk.zip");
    let outcome = engine
        .download(
            DownloadRequest::new(ResourceKind::JavaRuntime, url.as_str(), &dest)
                .with_sha1(sha1_hex(&payload))
                .with_size(payload.len() as u64),
        )
        .await
        .expect("谎报 Content-Range 后应退化单连接成功");

    assert_eq!(outcome.bytes, payload.len() as u64);
    assert_eq!(std::fs::read(&dest).expect("读回落盘文件失败"), payload);
    server.stop();
}
