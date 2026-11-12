//! `SourceFailed.kind` 的另一半（M197 复审 P2-2）：**404 是「该源没有这个文件」**，不是「源坏了」。
//!
//! 两档在引擎里的后果不同：`NotFound` 走 `record_missing`（不冷却源），`ServerError` 走
//! `record_failure`。院校镜像只留最新补丁，请求旧补丁必然 404——判成「源坏了」会把唯一可用镜像
//! 冷却掉（M33/M34 实测）。
//!
//! 变异证明：把 `engine.rs` / `bytes.rs` 的 `kind: err.kind` 改成常量 `Network` ⇒ 本条与
//! `reports_progress_events_and_errors_when_all_sources_fail` 一起红。
//!
//! 拆成子模块：`engine_mock_test.rs` 加了这条之后 407 行，越过 400 行硬线（`guards.yml` 的
//! `file-size`），于是把整条用例搬出来。

use std::sync::Arc;

use unlucky_core::download::cache::ContentCache;
use unlucky_core::download::engine::DownloadEngine;
use unlucky_core::download::error::DownloadErrorKind;
use unlucky_core::download::health::HealthRegistry;
use unlucky_core::download::mirrors::MirrorTable;
use unlucky_core::download::progress::{ProgressEmitter, ProgressEvent};
use unlucky_core::download::types::{DownloadRequest, ResourceKind};

use super::support::{temp_dir, MockResponse, MockServer};
use super::{direct_region, fast_config};

#[tokio::test]
async fn a_404_is_classified_as_not_found() {
    let server = MockServer::start(vec![
        MockResponse::status(404),
        MockResponse::status(404),
        MockResponse::status(404),
        MockResponse::status(404),
    ])
    .await;
    let url = server.url("/gone.jar");
    let dir = temp_dir("engine-404");
    let (emitter, mut events) = ProgressEmitter::channel();
    let engine = DownloadEngine::new(
        fast_config(1),
        Arc::new(MirrorTable::builtin()),
        Arc::new(direct_region()),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(dir.join("cache"))),
    )
    .with_progress(emitter);
    engine
        .download(DownloadRequest::new(
            ResourceKind::Library,
            url.as_str(),
            dir.join("gone.jar"),
        ))
        .await
        .expect_err("只有 404 的源必须失败");

    let mut seen: Vec<DownloadErrorKind> = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let ProgressEvent::SourceFailed { kind, .. } = event {
            seen.push(kind);
        }
    }
    assert!(!seen.is_empty(), "至少要有一条 SourceFailed");
    assert!(
        seen.iter().all(|kind| *kind == DownloadErrorKind::NotFound),
        "404 必须分类成 NotFound，实际：{seen:?}"
    );
    server.stop();
}
