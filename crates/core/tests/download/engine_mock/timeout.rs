//! 超时的**可诊断性**与**不误杀**（M289 / 台账 P19 的下半段）。
//!
//! 这条 mission 的价值不在「加了超时」，而在两件别的事：
//!
//! 1. **下次挂住时能一眼看出卡在哪**：每个超时错误都带阶段（连接 / 等响应头 / 等字节 /
//!    等分块回收 / 校验）+ 已收字节 / 预期字节 / 无进展时长三个数，且这几个数真的来自现场
//!    （不是常量）。变异：把 `StallReport` 的 `stage` 字段删掉、或把 `received` 写成 `0`
//!    常量 ⇒ `the_stage_numbers_are_real` 红（下面贴了红原文）。
//! 2. **慢但有效不许被误杀**：单次请求上界只覆盖「响应头到手」，正文靠空闲判定；
//!    所以一条 4 秒才下完 96 字节的滴流必须成功。变异：把等响应头的上界套到整个传输上
//!    ⇒ `a_slow_but_steady_stream_is_not_killed` 红。
//!
//! 还有一条来自复审的纠正：`engine.rs` 的 `_permit` 是 RAII 守卫（future 被 drop 时自己
//! 归还），所以这里**不写**「手动归还」补丁；用例断言的是**行为**——超时之后引擎还能
//! 正常完成一次新下载（`max_concurrency = 1` 时这条才是可观测的）。
//!
//! 拆成子模块：`engine_mock_test.rs` 已 364 行，再加这几条会越 400 行硬线。
//! 全离线（`127.0.0.1` 回环）、每条 <2s（含外层 `timeout` 保险）。

use std::time::{Duration, Instant};

use unlucky_core::download::config::EngineConfig;
use unlucky_core::download::error::DownloadErrorKind;
use unlucky_core::download::stall::{StallReason, StallStage};
use unlucky_core::download::types::{DownloadRequest, ResourceKind};

use super::harness::{config, engine, silent_responses, GATE};
use super::support::{sha1_hex, MockResponse, MockServer};

fn request(url: &str, dest: std::path::PathBuf, size: Option<u64>) -> DownloadRequest {
    let request = DownloadRequest::new(ResourceKind::ClientJar, url, &dest);
    match size {
        Some(size) => request.with_size(size),
        None => request,
    }
}

/// `silent()`：服务端接了连接、读完请求头、一个字都不回。
///
/// 这就是 P19 记的完全一致现场——`send()` 无界 ⇒ 这个 future 永不结束。
/// 断言四件事：①引擎**自己**在 2s 内返回（外层 5s 挂断 = 无界）②阶段是「等响应头」
/// ③三个数是真的（已收 0 / 预期 = 文件长度 / 无进展 ≈ 上界）④用户看得见的那行字里
/// 也有阶段与数字（`silent()` 引擎零事件、零日志，只有这一行）。
#[tokio::test]
async fn a_silent_server_times_out_and_names_the_stage_with_numbers() {
    let payload = vec![7_u8; 64 * 1024];
    let size = payload.len() as u64;
    let server = MockServer::start(silent_responses(2)).await;
    let (engine, dir) = engine(config(300, 300), "timeout-silent");
    let url = server.url("/silent.jar");

    let started = Instant::now();
    let error = tokio::time::timeout(
        GATE,
        engine.download(request(&url, dir.join("silent.jar"), Some(size))),
    )
    .await
    .expect("引擎必须自己超时返回：外层 5s 挂断正说明内部仍是**无界**的（这就是要修的 bug）")
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
        .expect("整体失败必须带最后一次失速现场（否则用户只看到「下载失败」）");
    assert_eq!(stall.stage, StallStage::AwaitHeaders, "{stall}");
    assert_eq!(stall.reason, StallReason::Deadline, "{stall}");
    assert_eq!(stall.received, 0, "一个字节都还没到：{stall}");
    assert_eq!(
        stall.expected,
        Some(size),
        "预期字节是这次请求的可诊断参数：{stall}"
    );
    assert!(
        stall.idle >= Duration::from_millis(300),
        "无进展时长不得小于上界：{stall}"
    );

    let text = error.to_string();
    for needle in ["等响应头", "await_headers", "已收 0 字节"] {
        assert!(text.contains(needle), "错误文案缺少「{needle}」：{text}");
    }
    assert!(
        text.contains("预期 65536 字节"),
        "错误文案缺少预期字节：{text}"
    );
    server.stop();
}

/// `stall_after`：响应头到手、正文写出一截、然后卡住。
///
/// 阶段必须是「等字节」而不是「等响应头」——两者同样是「没数据」，但修法不同
/// （前者等/换源，后者多半是源坏了）。三个数按**这条连接**的口径：已收 = 真读到的量。
#[tokio::test]
async fn a_stall_mid_body_is_named_await_bytes_with_the_real_byte_count() {
    let payload = vec![9_u8; 32 * 1024];
    let size = payload.len() as u64;
    let server =
        MockServer::start(vec![MockResponse::ok(payload.clone()).stalling_after(64)]).await;
    let (engine, dir) = engine(config(2_000, 300), "timeout-stall-body");
    let url = server.url("/stall.jar");

    let started = Instant::now();
    let error = tokio::time::timeout(
        GATE,
        engine.download(request(&url, dir.join("stall.jar"), Some(size))),
    )
    .await
    .expect("读取必须有界（外层挂断 = 读循环仍无界）")
    .expect_err("中途卡住必须失败");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(2),
        "应在上界附近返回：{elapsed:?}"
    );
    let stall = error.stall.as_deref().expect("中途卡住必须带失速现场");
    assert_eq!(stall.stage, StallStage::AwaitBytes, "{stall}");
    assert_eq!(stall.reason, StallReason::NoProgress, "{stall}");
    assert_eq!(
        stall.received, 64,
        "已收字节必须是**真实**读到的量（服务端只发了 64 字节）：{stall}"
    );
    assert_eq!(stall.expected, Some(size), "{stall}");
    assert!(stall.idle >= Duration::from_millis(300), "{stall}");
}

/// 阳性对照①：**短包**——头部声称全量、正文只到一半就关连接。
///
/// 必须快速、确定地失败（长度不符），不是挂住、也不是「成功但文件短了」。
#[tokio::test]
async fn a_short_body_fails_definitively_instead_of_hanging() {
    let payload = vec![3_u8; 8 * 1024];
    let size = payload.len() as u64;
    let server = MockServer::start(vec![MockResponse::ok(payload.clone()).short_body(4096)]).await;
    let (engine, dir) = engine(config(2_000, 2_000), "timeout-short-body");
    let url = server.url("/short.jar");

    let started = Instant::now();
    let error = tokio::time::timeout(
        GATE,
        engine.download(request(&url, dir.join("short.jar"), Some(size))),
    )
    .await
    .expect("短包必须快速失败，不许等读取上界")
    .expect_err("短包必须失败");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "短包应立即判失败"
    );
    assert_eq!(error.kind, DownloadErrorKind::AllSourcesFailed);
    // reqwest 对「Content-Length 没读满就被关掉」报的是 `error decoding response body`
    // （不是我们的「长度不符」），所以这里不钉它的文案：钉的是**有界 + 有成因 + 坏文件不落盘**。
    assert!(
        !error.suppressed.is_empty(),
        "短包必须留下一条逐源成因，而不是一句「下载失败」"
    );
    assert!(
        !dir.join("short.jar").exists(),
        "短包不许落盘（宁可失败也不能交付半个文件）"
    );
    assert!(error.stall.is_none(), "这不是失速（有明确终点）：{error}");
}

/// 阳性对照②：**超长包**——不发 `Content-Length`、正文之后再多吐一堆字节。
///
/// 客户端按期望长度收，多出来的字节不能悄悄算进文件：必须判长度不符。
#[tokio::test]
async fn an_oversize_body_is_rejected_by_length() {
    let payload = vec![5_u8; 8 * 1024];
    let size = payload.len() as u64;
    let server =
        MockServer::start(vec![MockResponse::ok(payload.clone()).oversize_body(4096)]).await;
    let (engine, dir) = engine(config(2_000, 2_000), "timeout-oversize-body");
    let url = server.url("/oversize.jar");

    let error = tokio::time::timeout(
        GATE,
        engine.download(request(&url, dir.join("oversize.jar"), Some(size))),
    )
    .await
    .expect("超长包必须有界地失败")
    .expect_err("超长包必须失败");
    assert!(
        error
            .suppressed
            .iter()
            .any(|item| item.contains("长度不符")),
        "超长包要报长度不符（多出来的字节不许算进文件）：{:?}",
        error.suppressed
    );
}

/// **慢但有效不许被误杀**（加超时最常见的自杀方式）。
///
/// 服务端每 40ms 滴 8 字节，96 字节要 ≈440ms 才下完；等响应头的上界只有 100ms。
/// 只要上界只覆盖「响应头到手」，这条就成功；一旦有人把它套到整个传输上就红。
#[tokio::test]
async fn a_slow_but_steady_stream_is_not_killed() {
    let payload: Vec<u8> = (0..96).map(|index| (index % 251) as u8).collect();
    let device = MockResponse::ok(payload.clone()).dripping(8, Duration::from_millis(40));
    let server = MockServer::start(vec![device]).await;
    let (engine, dir) = engine(config(100, 1_000), "timeout-slow-steady");
    let url = server.url("/slow.jar");
    let dest = dir.join("slow.jar");

    let outcome = tokio::time::timeout(
        GATE,
        engine.download(
            DownloadRequest::new(ResourceKind::ClientJar, url.as_str(), &dest)
                .with_sha1(sha1_hex(&payload))
                .with_size(payload.len() as u64),
        ),
    )
    .await
    .expect("慢但持续的流被误杀：时间上界被套到了整个传输上")
    .expect("慢但持续的流必须成功");

    assert_eq!(outcome.bytes, payload.len() as u64);
    assert_eq!(std::fs::read(&dest).expect("读回落盘文件失败"), payload);
    server.stop();
}

/// 复审纠正的判据：**超时之后引擎还能正常下载**（`_permit` 是 RAII，问题在无界，不在归还）。
///
/// `max_concurrency = 1` 让「许可有没有回到池里」可观测：第一条被超时打断后，
/// 第二条若拿不到许可就会挂住 ⇒ 外层 `GATE` 挂断并报「许可没还回来」。
#[tokio::test]
async fn after_a_timeout_the_engine_still_downloads_again() {
    let payload = b"second download works".to_vec();
    let silent = MockServer::start(silent_responses(2)).await;
    let good = MockServer::start(vec![MockResponse::ok(payload.clone())]).await;
    let (engine, dir) = engine(
        EngineConfig {
            max_concurrency: 1,
            ..config(200, 200)
        },
        "timeout-permit",
    );

    let first = tokio::time::timeout(
        GATE,
        engine.download(request(
            &silent.url("/dead.jar"),
            dir.join("dead.jar"),
            None,
        )),
    )
    .await
    .expect("第一条必须自己超时（外层挂断 = 无界）")
    .expect_err("silent 服务端必须失败");
    assert_eq!(first.kind, DownloadErrorKind::AllSourcesFailed);
    assert_eq!(
        engine.concurrency_probe(),
        1,
        "超时之后许可必须回到池里（RAII 自还，不许写手动归还补丁）"
    );

    let outcome = tokio::time::timeout(
        GATE,
        engine.download(
            DownloadRequest::new(
                ResourceKind::ClientJar,
                good.url("/ok.jar"),
                dir.join("ok.jar"),
            )
            .with_sha1(sha1_hex(&payload)),
        ),
    )
    .await
    .expect("许可没还回来：第二次下载挂住了")
    .expect("超时之后引擎必须还能完成一次新下载");
    assert_eq!(outcome.bytes, payload.len() as u64);
    silent.stop();
    good.stop();
}

/// 等许可是第五处 `await`（r1 p2-1 的连坐形态）：许可被占住时必须有界，且阶段是「等许可」。
///
/// `max_concurrency = 1` 让第一条下载攥住唯一许可；第二条堵在 `acquire()` 上。
/// 变异：把 `acquire()` 的 `timeout` 去掉 ⇒ 第二条在外层 `GATE` 处挂断而红。
#[tokio::test]
async fn waiting_for_a_permit_is_bounded_and_named_acquire_permit() {
    let held = vec![4_u8; 8 * 1024];
    let holder = MockServer::start(vec![MockResponse::ok(held.clone()).stalling_after(0)]).await;
    let good = MockServer::start(vec![MockResponse::ok(b"second".to_vec())]).await;
    let (engine, dir) = engine(
        EngineConfig {
            max_concurrency: 1,
            permit_timeout: Duration::from_millis(100),
            ..config(2_000, 300)
        },
        "timeout-permit-wait",
    );

    let (first, second) = tokio::time::timeout(GATE, async {
        tokio::join!(
            engine.download(request(
                &holder.url("/hold.jar"),
                dir.join("hold.jar"),
                None
            )),
            engine.download(request(&good.url("/ok.jar"), dir.join("ok.jar"), None)),
        )
    })
    .await
    .expect("等许可仍然无界：两条 select 在外层 5s 处挂断");

    assert!(first.is_err(), "第一条自己会读超时（占着许可 300ms）");
    let error = second.expect_err("许可被占住时第二条必须失败");
    let stall = error.stall.as_deref().expect("等许可失败要带现场");
    assert_eq!(stall.stage, StallStage::AcquirePermit, "{stall}");
    assert_eq!(stall.reason, StallReason::Deadline, "{stall}");
    assert!(
        stall.idle >= Duration::from_millis(100),
        "无进展时长不得小于上界：{stall}"
    );
    assert!(
        error.to_string().contains("等许可"),
        "用户那行字要说清卡在哪（不许冒充「服务器不给数据」）：{error}"
    );
    assert_eq!(good.hits(), 0, "没拿到许可就不该发请求");
    holder.stop();
    good.stop();
}
