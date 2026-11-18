//! `download_bytes` 的两处 `await` 也必须自己超时（M289 r1 复审 p2-1）。
//!
//! 复审用一个 `silent()` 探针证明了同一个模块里**另一条下载入口**一模一样地还在挂：
//! `download_bytes` 的 `.send()` 是裸的、`response.bytes()` 是整包读（`MAX_IN_MEMORY_BYTES`
//! 只管大小不管时间）。生产调用点三个：`game/install/source.rs`（版本清单 / 版本 JSON）、
//! `addon/online/transport.rs`（Modrinth 元数据）。
//!
//! 现在两条都走与文件下载同一套构造点：`http::send_bounded` + `http::read_step`
//! （整包读换成流式累加，顺带卡住没有 `Content-Length` 的洪流）。
//!
//! 变异：把 `fetch_bytes` 改回裸 `send()` ⇒ 第一条在外层 5s 处挂断而红；
//! 把流式读换回 `response.bytes()` ⇒ 第二条在外层 5s 处挂断而红。

use std::time::{Duration, Instant};

use unlucky_core::download::error::DownloadErrorKind;
use unlucky_core::download::stall::{StallReason, StallStage};
use unlucky_core::download::types::{DownloadRequest, ResourceKind};

use super::harness::{config, engine, silent_responses, GATE};
use super::support::{MockResponse, MockServer};

fn request(url: &str, dest: std::path::PathBuf, size: Option<u64>) -> DownloadRequest {
    let request = DownloadRequest::new(ResourceKind::VersionManifest, url, &dest);
    match size {
        Some(size) => request.with_size(size),
        None => request,
    }
}

/// 服务端接了连接、一个字都不回：`download_bytes` 必须自己在 2s 内返回，阶段=等响应头。
#[tokio::test]
async fn download_bytes_against_a_silent_server_is_bounded_and_named() {
    let server = MockServer::start(silent_responses(2)).await;
    let (engine, dir) = engine(config(200, 200), "timeout-bytes-silent");
    let url = server.url("/manifest.json");

    let started = Instant::now();
    let error = tokio::time::timeout(
        GATE,
        engine.download_bytes(request(&url, dir.join("manifest.json"), Some(4096))),
    )
    .await
    .expect("download_bytes 的 send() 又无界了：外层 5s 挂断（与 `download()` 是同一类 bug）")
    .expect_err("silent 服务端必须失败");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(2),
        "超时必须在上界附近发生，实际 {elapsed:?}"
    );
    assert_eq!(error.kind, DownloadErrorKind::AllSourcesFailed);
    let stall = error
        .stall
        .as_deref()
        .expect("`download_bytes` 的整体失败也要带最后一次失速现场");
    assert_eq!(stall.stage, StallStage::AwaitHeaders, "{stall}");
    assert_eq!(stall.reason, StallReason::Deadline, "{stall}");
    assert_eq!(stall.received, 0, "{stall}");
    assert_eq!(stall.expected, Some(4096), "{stall}");
    assert!(
        error.to_string().contains("等响应头"),
        "用户那行字要说清卡在哪：{error}"
    );
    server.stop();
}

/// 头到手、正文写出一截就卡住：整包读也必须**空闲**有界（阶段=等字节），不是等它读完。
#[tokio::test]
async fn download_bytes_body_read_is_idle_bounded() {
    let payload = vec![6_u8; 32 * 1024];
    let server = MockServer::start(vec![MockResponse::ok(payload).stalling_after(16)]).await;
    let (engine, dir) = engine(config(2_000, 300), "timeout-bytes-body");
    let url = server.url("/version.json");

    let started = Instant::now();
    let error = tokio::time::timeout(
        GATE,
        engine.download_bytes(request(&url, dir.join("version.json"), None)),
    )
    .await
    .expect("正文读取又无界了：外层 5s 挂断（`response.bytes()` 是整包读，没有空闲判定）")
    .expect_err("中途卡住必须失败");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(2),
        "应在读空闲附近返回：{elapsed:?}"
    );
    let stall = error.stall.as_deref().expect("中途卡住必须带失速现场");
    assert_eq!(stall.stage, StallStage::AwaitBytes, "{stall}");
    assert_eq!(stall.reason, StallReason::NoProgress, "{stall}");
    assert_eq!(stall.received, 16, "已收字节是真实读到量：{stall}");
    // 夹具声明了 `Content-Length`（`ok()` 会带），所以预期字节是有的；
    // 没有 `Content-Length` 的洪流由「边读边卡 MAX_IN_MEMORY_BYTES」兜（本文件不重复）。
    assert_eq!(stall.expected, Some(32 * 1024), "{stall}");
}
