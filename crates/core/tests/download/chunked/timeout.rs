//! 分块作业的两道失速判据（M289 / 台账 P19 的「188MB 落完 + 永久挂住」现场）。
//!
//! `JoinSet::join_next` 此前**没有总期限**：一个分块卡在 #1，`while let Some(...)` 就永远
//! 不结束——文件已经 `set_len(total)` 预分配（NTFS 上真占空间），复审看到的 `.ullpart`
//! 是满大小，看起来像「下完了」。这里钉住两件事，且两者必须**分得开**（错误里靠
//! `StallReason` 区分，而不是一律「下载失败」）：
//!
//! * 无进度：`counter` 连续 `chunk_stall_timeout` 没有新字节 ⇒ `reason = no_progress`；
//! * 总期限：`chunk_job_budget`（按平均吞吐下限反推）用完 ⇒ `reason = deadline`。
//!
//! 变异：把 `await_chunks` 换回 `while let Some(joined) = set.join_next().await`（无期限）
//! ⇒ 两条都靠外层 `GATE` 挂断而红；把 `StallReason` 的分支写成常量 ⇒ 第二条红
//! （它断言的是 `deadline`，而无进度那条断言 `no_progress`）。
//!
//! 拆成子模块：`chunked_test.rs` 已 379 行。

use std::sync::Arc;
use std::time::{Duration, Instant};

use unlucky_core::download::backoff::BackoffPolicy;
use unlucky_core::download::cache::ContentCache;
use unlucky_core::download::chunked::ChunkedConfig;
use unlucky_core::download::config::EngineConfig;
use unlucky_core::download::engine::DownloadEngine;
use unlucky_core::download::error::DownloadErrorKind;
use unlucky_core::download::health::HealthRegistry;
use unlucky_core::download::mirrors::MirrorTable;
use unlucky_core::download::region::RegionProfile;
use unlucky_core::download::stall::{StallReason, StallStage};
use unlucky_core::download::types::{DownloadRequest, ResourceKind};

use super::support::{temp_dir, MockResponse, MockServer};

/// 外层保险：分块作业若真的无期限，用例在这里挂断并说明「这就是 bug」。
const GATE: Duration = Duration::from_secs(5);

fn config() -> EngineConfig {
    EngineConfig {
        max_attempts_per_source: 1,
        backoff: BackoffPolicy::none(),
        connect_timeout: Duration::from_secs(2),
        response_timeout: Duration::from_secs(2),
        // 读空闲给到 10s：让**整体**判据先响，证明它自己有效（不是靠单块读超时兜底）。
        read_timeout: Duration::from_secs(10),
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

fn harness(tag: &str, cfg: EngineConfig) -> (DownloadEngine, std::path::PathBuf) {
    let dir = temp_dir(tag);
    let cache = Arc::new(ContentCache::open(dir.join("cache")));
    let engine = DownloadEngine::new(
        cfg,
        Arc::new(MirrorTable::builtin()),
        Arc::new(RegionProfile::direct(0)),
        Arc::new(HealthRegistry::in_memory()),
        cache,
    );
    (engine, dir)
}

fn request(url: &str, dest: std::path::PathBuf, size: u64) -> DownloadRequest {
    DownloadRequest::new(ResourceKind::JavaRuntime, url, &dest).with_size(size)
}

/// 4 条分块连接全部「回了头、不给字节」：整个作业两分钟不动 ⇒ 「等分块回收 + 没有新字节」。
#[tokio::test]
async fn a_chunked_job_with_no_progress_is_named_await_chunk_join() {
    let payload = vec![1_u8; 16 * 1024];
    let size = payload.len() as u64;
    let stalled = |_| MockResponse::ok(payload.clone()).stalling_after(0);
    let server = MockServer::start((0..4).map(stalled).collect()).await;
    let (engine, dir) = harness(
        "chunked-timeout-stall",
        EngineConfig {
            chunk_stall_timeout: Duration::from_millis(200),
            ..config()
        },
    );
    let url = server.url("/jdk.zip");

    let started = Instant::now();
    let error = tokio::time::timeout(
        GATE,
        engine.download(request(&url, dir.join("jdk.zip"), size)),
    )
    .await
    .expect("分块回收必须有期限：外层 5s 挂断正说明 `join_next` 仍无界")
    .expect_err("4 条连接都不给字节时必须失败");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(2),
        "应在无进度窗口附近返回：{elapsed:?}"
    );
    assert_eq!(error.kind, DownloadErrorKind::AllSourcesFailed);
    let stall = error.stall.as_deref().expect("分块失速必须带现场");
    assert_eq!(stall.stage, StallStage::AwaitChunkJoin, "{stall}");
    assert_eq!(stall.reason, StallReason::NoProgress, "{stall}");
    assert_eq!(
        stall.received, 0,
        "一个字节都没到（口径是整个作业）：{stall}"
    );
    assert_eq!(stall.expected, Some(size), "{stall}");
    assert!(
        stall.idle >= Duration::from_millis(200),
        "无进展时长必须 ≥ 判定窗口：{stall}"
    );
    assert!(
        error.to_string().contains("等分块回收"),
        "用户那行字要说清卡在哪：{error}"
    );
    server.stop();
}

/// 分块作业的**总期限**：一直在动，但平均吞吐低到没意义 ⇒ 「等分块回收 + 期限用完」。
///
/// 与上一条的区别只有 `reason`：这条的 `received > 0`（一直在动），上一条是 0。
/// 另外钉住 r1 p2-2 的第一条要求：预算用完**不重试**（`max_attempts_per_source = 3` 之下
/// 仍然只发一轮分块请求，`hits` 停在 4）。变异：让 `DownloadError::is_retryable()` 忽略
/// `job_budget_exhausted()` ⇒ `hits` 变成 8 / 12，本条红。
#[tokio::test]
async fn a_chunked_job_that_is_too_slow_hits_the_total_budget_without_retrying() {
    let payload = vec![2_u8; 16 * 1024];
    let size = payload.len() as u64;
    // 每条连接每 20ms 滴 1 字节：一直在动（不会触发无进度），但远低于平均吞吐下限。
    let dripping = |_| MockResponse::ok(payload.clone()).dripping(1, Duration::from_millis(20));
    let server = MockServer::start((0..12).map(dripping).collect()).await;
    let (engine, dir) = harness(
        "chunked-timeout-budget",
        EngineConfig {
            chunk_job_floor: Duration::from_millis(300),
            max_attempts_per_source: 3,
            ..config()
        },
    );
    let url = server.url("/jdk.zip");

    let started = Instant::now();
    let error = tokio::time::timeout(
        GATE,
        engine.download(request(&url, dir.join("jdk.zip"), size)),
    )
    .await
    .expect("分块作业总期限必须有界（外层挂断 = 无界）")
    .expect_err("慢到没意义的作业必须在总期限处失败");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(2),
        "总期限 300ms，实际 {elapsed:?}（太长说明期限没被用上）"
    );
    let stall = error.stall.as_deref().expect("总期限失败也要带现场");
    assert_eq!(stall.stage, StallStage::AwaitChunkJoin, "{stall}");
    assert_eq!(
        stall.reason,
        StallReason::Deadline,
        "总期限要用 deadline，不能与「无进度」混成一句：{stall}"
    );
    assert!(
        stall.received > 0,
        "这条一直在动，已收字节必须 > 0（0 就说明它其实是卡住而不是慢）：{stall}"
    );
    assert_eq!(stall.expected, Some(size), "{stall}");
    assert!(
        !error.is_retryable(),
        "整作业总期限用完是链路级判决，不许重试：{error}"
    );
    assert_eq!(
        server.hits(),
        4,
        "预算用完只许发一轮分块请求（重试会再烧满一整个预算）"
    );
    server.stop();
}

/// 预算耗尽**连候选源都不再换**（r1 p2-2 的「必然失败的形态」）：镜像慢到撞预算，
/// 官方源一次都不该被碰——同一份字节、同一条链路，换源只会再烧一轮。
///
/// 变异：去掉 `engine.rs` 里的 `if exhausted { break; }` ⇒ `official.hits()` 变成 1，红。
#[tokio::test]
async fn a_budget_exhaustion_does_not_try_the_next_source() {
    use unlucky_core::download::mirrors::PrefixRule;

    let payload = vec![8_u8; 16 * 1024];
    let size = payload.len() as u64;
    let official = MockServer::start(vec![MockResponse::ok(payload.clone())]).await;
    let mirror = MockServer::start(
        (0..12)
            .map(|_| MockResponse::ok(payload.clone()).dripping(1, Duration::from_millis(20)))
            .collect(),
    )
    .await;
    let mut table = MirrorTable::builtin();
    table.add_custom_rule(PrefixRule::new(
        "mock-mirror",
        format!("http://127.0.0.1:{}", official.port()),
        format!("http://localhost:{}", mirror.port()),
    ));
    let dir = temp_dir("chunked-timeout-no-failover");
    let engine = DownloadEngine::new(
        EngineConfig {
            chunk_job_floor: Duration::from_millis(300),
            max_attempts_per_source: 3,
            ..config()
        },
        Arc::new(table),
        Arc::new(RegionProfile::mirror_first(0)),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(dir.join("cache"))),
    );
    let url = official.url("/jdk.zip");

    let error = tokio::time::timeout(
        GATE,
        engine.download(request(&url, dir.join("jdk.zip"), size)),
    )
    .await
    .expect("预算耗尽必须有界返回")
    .expect_err("慢到没意义的镜像必须让整次下载失败");
    assert_eq!(
        official.hits(),
        0,
        "预算用完不该再换源（同一链路再烧一轮 100 分钟没有意义）"
    );
    assert!(error.job_budget_exhausted(), "{error}");
    official.stop();
    mirror.stop();
}
