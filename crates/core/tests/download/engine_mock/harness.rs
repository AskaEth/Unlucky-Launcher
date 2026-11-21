//! `engine_mock/timeout*` 两条用例文件共用的夹具（引擎构造 / 外层保险 / 静默响应脚本）。
//!
//! 拆出来是因为 `bytes_timeout.rs` 与 `timeout.rs` 是兄弟模块，各写一份会变成「两份默认值」。

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

use super::support::{temp_dir, MockResponse};

/// 外层保险：引擎若真的无界，用例在这里挂断并给出「这就是 bug」的说明，而不是跑不完。
pub const GATE: Duration = Duration::from_secs(5);

/// 只为超时用例准备的引擎（单连接路径；分块用例自己拼配置）。
pub fn config(response_ms: u64, read_ms: u64) -> EngineConfig {
    EngineConfig {
        max_attempts_per_source: 1,
        backoff: BackoffPolicy::none(),
        connect_timeout: Duration::from_secs(2),
        response_timeout: Duration::from_millis(response_ms),
        read_timeout: Duration::from_millis(read_ms),
        chunked: ChunkedConfig::disabled(),
        ..EngineConfig::default()
    }
}

pub fn engine(cfg: EngineConfig, tag: &str) -> (DownloadEngine, std::path::PathBuf) {
    let dir = temp_dir(tag);
    let engine = DownloadEngine::new(
        cfg,
        Arc::new(MirrorTable::builtin()),
        Arc::new(RegionProfile::direct(0)),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(dir.join("cache"))),
    );
    (engine, dir)
}

/// `silent()` 服务的脚本（`count` 份：多候选/多重试时不会被脚本耗尽改成 500）。
pub fn silent_responses(count: usize) -> Vec<MockResponse> {
    (0..count).map(|_| MockResponse::silent()).collect()
}
