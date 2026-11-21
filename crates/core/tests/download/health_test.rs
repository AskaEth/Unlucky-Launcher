//! 健康度记忆的单测：冷却、恢复探测、排序、持久化 + 引擎驱动的落盘接线。

mod support;

use std::sync::Arc;
use std::time::Duration;

use unlucky_core::download::backoff::BackoffPolicy;
use unlucky_core::download::cache::{now_ms, ContentCache};
use unlucky_core::download::config::EngineConfig;
use unlucky_core::download::engine::DownloadEngine;
use unlucky_core::download::error::DownloadErrorKind;
use unlucky_core::download::health::{
    HealthRegistry, SourceHealth, COOLDOWN_MS, FAILURE_THRESHOLD,
};
use unlucky_core::download::mirrors::MirrorTable;
use unlucky_core::download::region::RegionProfile;
use unlucky_core::download::types::{Candidate, DownloadRequest, ResourceKind, SOURCE_OFFICIAL};

use support::{MockResponse, MockServer};

const NOW: u64 = 1_000_000;

fn registry() -> HealthRegistry {
    HealthRegistry::in_memory()
}

#[test]
fn fresh_source_is_neutral_not_penalized() {
    let health = SourceHealth::default();
    assert!(!health.is_cooling_down(NOW));
    assert_eq!(health.failure_rate(), 0.0);
    // 未知源必须是中间分，不能比刚成功的源更优先。
    let healthy = SourceHealth {
        samples: 3,
        successes: 3,
        ema_rtt_ms: 120.0,
        ..SourceHealth::default()
    };
    assert!(healthy.score(NOW) < health.score(NOW));
}

#[test]
fn three_consecutive_failures_trigger_cooldown() {
    let mut health = SourceHealth::default();
    for _ in 0..FAILURE_THRESHOLD {
        health.record_failure("HTTP 502", NOW);
    }
    assert!(health.is_cooling_down(NOW));
    assert!(health.is_cooling_down(NOW + COOLDOWN_MS - 1));
    // 冷却到期后不再是冷却态，而是「可以探测」。
    assert!(!health.is_cooling_down(NOW + COOLDOWN_MS));
    assert!(health.is_probe(NOW + COOLDOWN_MS));
    // 冷却中的源排序惩罚要明显高于健康源。
    assert!(health.score(NOW) >= 1_000.0);
}

#[test]
fn missing_artifact_is_recorded_without_cooling_down() {
    // 院校镜像只留最新补丁：请求旧补丁必然 404，不能把源判成「坏」。
    let mut health = SourceHealth::default();
    for _ in 0..(FAILURE_THRESHOLD * 2) {
        health.record_missing("HTTP 404", NOW);
    }
    assert!(!health.is_cooling_down(NOW), "4xx 不进冷却");
    assert!(!health.is_probe(NOW), "4xx 不该产生恢复探测状态");
    assert_eq!(health.failures, 0, "4xx 不计失败次数");
    assert_eq!(health.not_found, u64::from(FAILURE_THRESHOLD * 2));
    assert_eq!(health.failure_rate(), 0.0);
    assert!(health.last_error.is_some(), "诊断仍要留痕");
    // 打分不应被 404 拉高（仍是未知源的中位分）。
    assert_eq!(health.score(NOW), SourceHealth::default().score(NOW));
    assert_eq!(
        health.score(NOW),
        unlucky_core::download::health::SCORE_UNKNOWN
    );
}

#[test]
fn not_found_ageing_does_not_persist_a_cooldown() {
    let registry = HealthRegistry::in_memory();
    for _ in 0..10 {
        registry.record_missing("tuna", ResourceKind::JavaRuntime, "HTTP 404", NOW);
    }
    let health = registry.health_of("tuna", ResourceKind::JavaRuntime);
    assert!(!health.is_cooling_down(NOW));
    assert_eq!(health.not_found, 10);
    assert_eq!(health.consecutive_failures, 0);
}

#[test]
fn probe_failure_re_arms_cooldown_and_success_clears_it() {
    let mut health = SourceHealth::default();
    for _ in 0..FAILURE_THRESHOLD {
        health.record_failure("timeout", NOW);
    }
    let after_cooldown = NOW + COOLDOWN_MS + 1;
    assert!(!health.is_cooling_down(after_cooldown));
    // 探测再一次失败 → 立即重新冷却。
    health.record_failure("timeout again", after_cooldown);
    assert!(health.is_cooling_down(after_cooldown));

    // 成功则全部清零。
    health.record_success(90, after_cooldown + 1);
    assert!(!health.is_cooling_down(after_cooldown + 1));
    assert!(!health.is_probe(after_cooldown + 1));
    assert_eq!(health.consecutive_failures, 0);
    assert_eq!(health.cooldown_until_ms, 0);
    assert_eq!(health.last_error, None);
}

#[test]
fn ema_smooths_latency_and_counts_samples() {
    let mut health = SourceHealth::default();
    health.record_success(100, NOW);
    assert_eq!(health.ema_rtt_ms, 100.0);
    health.record_success(200, NOW);
    // 0.3 * 200 + 0.7 * 100 = 130
    assert!((health.ema_rtt_ms - 130.0).abs() < 1e-9);
    assert_eq!(health.samples, 2);
    assert_eq!(health.successes, 2);
}

#[test]
fn failure_rate_counts_both_sides() {
    let mut health = SourceHealth::default();
    health.record_success(50, NOW);
    health.record_failure("boom", NOW);
    assert!((health.failure_rate() - 0.5).abs() < 1e-9);
}

#[test]
fn registry_ranks_healthy_first_and_cooling_last() {
    let registry = registry();
    for _ in 0..FAILURE_THRESHOLD {
        registry.record_failure("mirror-b", ResourceKind::ClientJar, "HTTP 500", NOW);
    }
    registry.record_success(
        "mirror-a",
        ResourceKind::ClientJar,
        Duration::from_millis(80),
        NOW,
    );

    let candidates = vec![
        Candidate::new("mirror-b", "https://b.example/file"),
        Candidate::new("unknown", "https://u.example/file"),
        Candidate::new("mirror-a", "https://a.example/file"),
    ];
    let ranked = registry.rank(&candidates, ResourceKind::ClientJar, NOW);
    assert_eq!(ranked[0].source, "mirror-a");
    assert_eq!(ranked[1].source, "unknown");
    assert_eq!(
        ranked[2].source, "mirror-b",
        "冷却中的源排最后但仍保留为兜底"
    );
}

#[test]
fn health_is_tracked_per_resource_kind() {
    let registry = registry();
    for _ in 0..FAILURE_THRESHOLD {
        registry.record_failure("bmclapi", ResourceKind::Asset, "404", NOW);
    }
    assert!(registry.is_cooling_down("bmclapi", ResourceKind::Asset, NOW));
    // 另一种资源类型不受影响（资产挂了不代表 JDK 也挂）。
    assert!(!registry.is_cooling_down("bmclapi", ResourceKind::JavaRuntime, NOW));
}

#[test]
fn persistence_roundtrip_and_age_pruning() {
    let dir = std::env::temp_dir().join(format!("ull-health-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("health.json");

    let registry = HealthRegistry::open(&path, 7 * 24 * 3600 * 1000, NOW);
    registry.record_failure("bmclapi", ResourceKind::Library, "HTTP 503", NOW);
    registry.record_success(
        "official",
        ResourceKind::Library,
        Duration::from_millis(55),
        NOW,
    );
    registry.save().expect("持久化失败");

    let reloaded = HealthRegistry::open(&path, 7 * 24 * 3600 * 1000, NOW + 1_000);
    assert_eq!(reloaded.snapshot().len(), 2);
    assert_eq!(
        reloaded
            .health_of("bmclapi", ResourceKind::Library)
            .failures,
        1
    );
    assert!(
        reloaded
            .health_of("official", ResourceKind::Library)
            .ema_rtt_ms
            > 0.0
    );

    // 超出最大保留期 → 丢弃旧记录（缓存目录被长时间放着的情况）。
    let pruned = HealthRegistry::open(&path, 10, NOW + 60_000);
    assert!(pruned.snapshot().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

/// P1 回归：健康度必须由**引擎的生产路径**落盘——下载→（模拟进程退出）→重开 registry，
/// 冷却状态与失败计数都要还在，否则「连续 3 败冷却 10 分钟」每轮启动从零学起。
#[tokio::test]
async fn engine_persists_health_so_a_restart_keeps_the_cooldown() {
    let failed = || MockResponse::status(500);
    let server = MockServer::start(vec![failed(), failed(), failed(), failed()]).await;
    let url = server.url("/lib.jar");
    let dir = support::temp_dir("health-engine-persist");
    let path = dir.join("cache/health.json");
    let now = now_ms();
    let max_age_ms = 7 * 24 * 3600 * 1000;

    let registry = Arc::new(HealthRegistry::open(&path, max_age_ms, now));
    let engine = DownloadEngine::new(
        EngineConfig {
            max_attempts_per_source: FAILURE_THRESHOLD,
            backoff: BackoffPolicy {
                base_ms: 1,
                max_ms: 2,
                jitter_ratio: 0.0,
                max_attempts: FAILURE_THRESHOLD,
            },
            ..EngineConfig::default()
        },
        Arc::new(MirrorTable::builtin()),
        Arc::new(RegionProfile::direct(0)),
        Arc::clone(&registry),
        Arc::new(ContentCache::open(dir.join("cache"))),
    );

    let error = engine
        .download(DownloadRequest::new(
            ResourceKind::Library,
            url.as_str(),
            dir.join("lib.jar"),
        ))
        .await
        .expect_err("全 500 必须失败");
    assert_eq!(error.kind, DownloadErrorKind::AllSourcesFailed);
    server.stop();

    // 第 3 次连续失败进入冷却时就该落盘（不等收尾 flush）。
    assert!(path.is_file(), "健康度必须已经落盘到 cache/health.json");
    let before = registry.health_of(SOURCE_OFFICIAL, ResourceKind::Library);
    assert!(before.consecutive_failures >= FAILURE_THRESHOLD);
    assert!(before.is_cooling_down(now_ms()));

    // 「进程退出后重开」：从同一个文件恢复，冷却状态与失败计数都还在。
    let reopened = HealthRegistry::open(&path, max_age_ms, now_ms());
    let restored = reopened.health_of(SOURCE_OFFICIAL, ResourceKind::Library);
    assert_eq!(restored.consecutive_failures, before.consecutive_failures);
    assert_eq!(restored.failures, before.failures);
    assert!(
        restored.is_cooling_down(now_ms()),
        "重开后仍应处于冷却：{restored:?}"
    );
    assert!(restored.last_error.is_some(), "失败原因也要持久化");

    let _ = std::fs::remove_dir_all(&dir);
}
