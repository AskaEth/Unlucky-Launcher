//! 引擎端到端（mock 源）：成功 / 退避重试 / 换源 / 校验失败 / 缓存命中 / 断点续传。
//!
//! 用 `tests/download/support` 里的进程内 HTTP 服务器，不发真实网络请求。

mod support;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use unlucky_core::download::backoff::BackoffPolicy;
use unlucky_core::download::cache::ContentCache;
use unlucky_core::download::chunked::ChunkedConfig;
use unlucky_core::download::config::EngineConfig;
use unlucky_core::download::engine::DownloadEngine;
use unlucky_core::download::error::DownloadErrorKind;
use unlucky_core::download::health::HealthRegistry;
use unlucky_core::download::mirrors::{MirrorTable, PrefixRule};
use unlucky_core::download::progress::{ProgressEmitter, ProgressEvent};
use unlucky_core::download::region::RegionProfile;
use unlucky_core::download::resume::ResumeMeta;
use unlucky_core::download::types::{DownloadRequest, HashAlgo, ResourceKind};

use support::{sha1_hex, temp_dir, MockResponse, MockServer};

fn fast_config(attempts: u32) -> EngineConfig {
    EngineConfig {
        max_attempts_per_source: attempts,
        backoff: BackoffPolicy {
            base_ms: 1,
            max_ms: 2,
            jitter_ratio: 0.0,
            max_attempts: attempts,
        },
        connect_timeout: Duration::from_secs(5),
        read_timeout: Duration::from_secs(5),
        chunked: ChunkedConfig::disabled(),
        ..EngineConfig::default()
    }
}

struct Harness {
    engine: DownloadEngine,
    dir: PathBuf,
}

fn harness(cfg: EngineConfig, table: MirrorTable, region: RegionProfile, tag: &str) -> Harness {
    let dir = temp_dir(tag);
    let cache = Arc::new(ContentCache::open(dir.join("cache")));
    let health = Arc::new(HealthRegistry::in_memory());
    let engine = DownloadEngine::new(cfg, Arc::new(table), Arc::new(region), health, cache);
    Harness { engine, dir }
}

fn direct_region() -> RegionProfile {
    RegionProfile::direct(0)
}

#[tokio::test]
async fn downloads_verifies_and_leaves_no_part_file() {
    let payload = b"unlucky launcher payload".to_vec();
    let server = MockServer::start(vec![MockResponse::ok(payload.clone())]).await;
    let url = server.url("/client.jar");
    let harness = harness(
        fast_config(2),
        MirrorTable::builtin(),
        direct_region(),
        "engine-ok",
    );
    let dest = harness.dir.join("versions/1.20.1/client.jar");

    let request = DownloadRequest::new(ResourceKind::ClientJar, url.as_str(), &dest)
        .with_sha1(sha1_hex(&payload))
        .with_size(payload.len() as u64);
    let outcome = harness.engine.download(request).await.expect("下载应成功");

    assert_eq!(outcome.bytes, payload.len() as u64);
    assert!(!outcome.from_cache);
    assert_eq!(outcome.resumed_from, 0);
    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    let version_dir = harness.dir.join("versions/1.20.1");
    assert!(
        support::part_files(&version_dir).is_empty(),
        "中间文件必须被提交掉：{:?}",
        support::part_files(&version_dir)
    );
    server.stop();
}

#[tokio::test]
async fn retries_same_source_after_server_error() {
    let payload = b"retry me".to_vec();
    let server = MockServer::start(vec![
        MockResponse::status(503),
        MockResponse::ok(payload.clone()),
    ])
    .await;
    let url = server.url("/lib.jar");
    let harness = harness(
        fast_config(3),
        MirrorTable::builtin(),
        direct_region(),
        "engine-retry",
    );
    let dest = harness.dir.join("lib.jar");

    let outcome = harness
        .engine
        .download(
            DownloadRequest::new(ResourceKind::Library, url.as_str(), &dest)
                .with_sha1(sha1_hex(&payload)),
        )
        .await
        .expect("退避后应成功");
    assert_eq!(outcome.bytes, payload.len() as u64);
    assert_eq!(server.hits(), 2, "第一次 503 之后应重试一次");
    server.stop();
}

#[tokio::test]
async fn switches_to_official_when_mirror_keeps_failing() {
    let payload = b"official wins".to_vec();
    let official = MockServer::start(vec![MockResponse::ok(payload.clone())]).await;
    let mirror = MockServer::start(vec![
        MockResponse::status(500),
        MockResponse::status(500),
        MockResponse::status(500),
    ])
    .await;

    let mut table = MirrorTable::builtin();
    let official_prefix = format!("http://127.0.0.1:{}", official.port());
    let mirror_prefix = format!("http://localhost:{}", mirror.port());
    table.add_custom_rule(PrefixRule::new(
        "mock-mirror",
        official_prefix,
        mirror_prefix,
    ));

    let harness = harness(
        fast_config(2),
        table,
        RegionProfile::mirror_first(0),
        "engine-switch",
    );
    let dest = harness.dir.join("client.jar");
    let url = official.url("/client.jar");

    let outcome = harness
        .engine
        .download(
            DownloadRequest::new(ResourceKind::ClientJar, url.as_str(), &dest)
                .with_sha1(sha1_hex(&payload)),
        )
        .await
        .expect("镜像全挂后应切到官方");
    assert_eq!(outcome.source, "official");
    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    assert_eq!(mirror.hits(), 2, "每个源最多尝试 max_attempts 次");
    official.stop();
    mirror.stop();
}

#[tokio::test]
async fn rejects_wrong_checksum_and_falls_back() {
    let payload = b"the real jar".to_vec();
    let official = MockServer::start(vec![MockResponse::ok(payload.clone())]).await;
    let mirror = MockServer::start(vec![MockResponse::ok(b"tampered by mirror".to_vec())]).await;

    let mut table = MirrorTable::builtin();
    table.add_custom_rule(PrefixRule::new(
        "mock-mirror",
        format!("http://127.0.0.1:{}", official.port()),
        format!("http://localhost:{}", mirror.port()),
    ));

    let harness = harness(
        fast_config(1),
        table,
        RegionProfile::mirror_first(0),
        "engine-checksum",
    );
    let dest = harness.dir.join("client.jar");
    let url = official.url("/client.jar");

    let outcome = harness
        .engine
        .download(
            DownloadRequest::new(ResourceKind::ClientJar, url.as_str(), &dest)
                .with_sha1(sha1_hex(&payload))
                .with_size(payload.len() as u64),
        )
        .await
        .expect("校验失败应换源而不是交付坏文件");
    assert_eq!(outcome.source, "official");
    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    assert!(mirror.hits() >= 1);
    official.stop();
    mirror.stop();
}

#[tokio::test]
async fn second_download_hits_content_cache_without_network() {
    let payload = b"cached payload".to_vec();
    let server = MockServer::start(vec![
        MockResponse::ok(payload.clone()),
        MockResponse::ok(payload.clone()),
    ])
    .await;
    let url = server.url("/lib.jar");
    let harness = harness(
        fast_config(2),
        MirrorTable::builtin(),
        direct_region(),
        "engine-cache",
    );
    let integrity = sha1_hex(&payload);

    let first = harness
        .engine
        .download(
            DownloadRequest::new(
                ResourceKind::Library,
                url.as_str(),
                harness.dir.join("a/lib.jar"),
            )
            .with_sha1(integrity.as_str()),
        )
        .await
        .expect("首次下载失败");
    assert!(!first.from_cache);

    let hits_after_first = server.hits();
    let second = harness
        .engine
        .download(
            DownloadRequest::new(
                ResourceKind::Library,
                url.as_str(),
                harness.dir.join("b/lib.jar"),
            )
            .with_sha1(integrity.as_str()),
        )
        .await
        .expect("缓存命中失败");
    assert!(second.from_cache);
    assert_eq!(second.bytes, payload.len() as u64);
    assert_eq!(server.hits(), hits_after_first, "缓存命中不能再发请求");
    assert_eq!(
        std::fs::read(harness.dir.join("b/lib.jar")).unwrap(),
        payload
    );
    server.stop();
}

#[tokio::test]
async fn resumes_from_existing_part_file() {
    let payload = b"0123456789abcdefghij".to_vec();
    let split = 8_usize;
    let server = MockServer::start(vec![MockResponse::ok(payload.clone())]).await;
    let url = server.url("/client.jar");
    let harness = harness(
        fast_config(2),
        MirrorTable::builtin(),
        direct_region(),
        "engine-resume",
    );
    let dest = harness.dir.join("client.jar");
    let part = harness.dir.join("client.jar.ullpart");
    let meta_path = harness.dir.join("client.jar.ullpart.ullmeta.json");

    std::fs::write(&part, &payload[..split]).unwrap();
    ResumeMeta {
        url: url.clone(),
        etag: Some("\"v1\"".to_string()),
        last_modified: None,
        total: Some(payload.len() as u64),
        algo: HashAlgo::Sha1,
        hash: sha1_hex(&payload),
        saved_at_ms: 1,
    }
    .save(&meta_path)
    .expect("写续传元数据失败");

    let outcome = harness
        .engine
        .download(
            DownloadRequest::new(ResourceKind::ClientJar, url.as_str(), &dest)
                .with_sha1(sha1_hex(&payload))
                .with_size(payload.len() as u64),
        )
        .await
        .expect("续传失败");

    assert_eq!(outcome.resumed_from, split as u64, "应从已有偏移续传");
    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    assert_eq!(server.hits(), 1);
    server.stop();
}

#[tokio::test]
async fn reports_progress_events_and_errors_when_all_sources_fail() {
    let server = MockServer::start(vec![
        MockResponse::status(500),
        MockResponse::status(500),
        MockResponse::status(500),
        MockResponse::status(500),
    ])
    .await;
    let url = server.url("/dead.jar");
    let dir = temp_dir("engine-fail");
    let cache = Arc::new(ContentCache::open(dir.join("cache")));
    let (emitter, mut events) = ProgressEmitter::channel();
    let engine = DownloadEngine::new(
        fast_config(1),
        Arc::new(MirrorTable::builtin()),
        Arc::new(direct_region()),
        Arc::new(HealthRegistry::in_memory()),
        cache,
    )
    .with_progress(emitter);

    let error = engine
        .download(DownloadRequest::new(
            ResourceKind::Library,
            url.as_str(),
            dir.join("dead.jar"),
        ))
        .await
        .expect_err("所有源都 500 时必须失败");
    assert_eq!(error.kind, DownloadErrorKind::AllSourcesFailed);

    let mut started = false;
    let mut failed_source = false;
    while let Ok(event) = events.try_recv() {
        match event {
            ProgressEvent::Started { candidates, .. } => {
                started = true;
                assert!(candidates >= 1);
            }
            ProgressEvent::SourceFailed {
                will_retry, kind, ..
            } => {
                failed_source = true;
                assert!(!will_retry, "单次尝试用尽后不应再声明重试");
                // M197：`kind` 是诊断字段，必须来自**真实**的失败分类（HTTP 500 → ServerError）。
                // 变异证明：把 `engine.rs` 的 `kind: err.kind` 写成常量
                // `DownloadErrorKind::Network` ⇒ 本断言红。
                assert_eq!(
                    kind,
                    DownloadErrorKind::ServerError,
                    "HTTP 500 必须被分类成 ServerError（诊断字段不许是常量）"
                );
            }
            _ => {}
        }
    }
    assert!(started && failed_source, "进度事件缺失");
    server.stop();
}

/// 「404 不是源坏了」的守卫（M197 P2-2 的第二档）：拆到子模块守 400 行硬线。
#[path = "engine_mock/not_found.rs"]
mod not_found;
