//! [`EngineTransport`] 的**真出口**用例：本机回环假服务端 + 真 `reqwest::Client`，一条都不出网（M423）。
//!
//! # 为什么这一组不能复用 `FixtureTransport`
//!
//! 那个替身是 `async` 直接 return 的：它**根本走不到 `reqwest`**，于是 `send()` / `text()` 有没有
//! 上界在它面前等价。M423 修的正是这两处（`transport.rs` 曾经的裸 `.send()` 与 `Response::text()`），
//! 所以守卫只能用真的 socket：上界在不在，由**墙钟**说话。
//!
//! 三个假服务端各钉一件事：
//!
//! | 假服务端 | 复现的坏法 | 钉住的界 |
//! |----------|------------|----------|
//! | [`serve_silent`] | 接了连接、一个字节不回 | 等响应头（`await_headers`） |
//! | [`serve_headers_only`] | 回了头、正文一个字节不发 | 等字节（`await_bytes`） |
//! | [`serve_once`] | 正常回一条响应 | 改造不许改掉成功路径（状态码 / `Retry-After` / 请求头透传） |
//!
//! 每个失败用例都套一层 [`tokio::time::timeout`]：上界若被拆掉，用例应该**变红**（5s 到点报
//! 「无界」），而不是把整个测试二进制挂在那里——挂死的用例在 CI 里是最难认的一种红。

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::addon::curseforge::{CfErrorKind, CfTransport, EngineTransport};
use crate::download::{
    ContentCache, DownloadEngine, EngineConfig, HealthRegistry, MirrorTable, ProxySetting,
    RegionProfile,
};

/// 用例自己的「无界」判据：真的无界时，这个时限会先到。
const HANG_GUARD: Duration = Duration::from_secs(5);

/// 一台**只连回环**的测试引擎。
///
/// 两处必须改掉出厂值，否则用例证明的东西会跑偏：
/// * 两个界压到几百毫秒——用例要跑得动；
/// * 代理强制 [`ProxySetting::Direct`]——`System` 会读进程环境变量，本机有代理时那些请求会被
///   拐出去，「本机用例」就不再是本机用例。
fn engine(response: Duration, read_idle: Duration) -> DownloadEngine {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let unique = SEQ.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("ull-cf-transport-{}-{unique}", std::process::id()));
    DownloadEngine::new(
        EngineConfig {
            response_timeout: response,
            read_timeout: read_idle,
            proxy: ProxySetting::Direct,
            ..EngineConfig::default()
        },
        Arc::new(MirrorTable::builtin()),
        Arc::new(RegionProfile::direct(0)),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(root)),
    )
}

/// 绑一个回环端口并返回它的 URL（路径无所谓，服务端不看）。
fn listener_url(listener: &TcpListener) -> String {
    let addr = listener.local_addr().expect("取端口");
    format!("http://{addr}/v1/mods/search")
}

/// 把请求头读到 `\r\n\r\n` 为止（读不到就返回已读到的部分——用例不靠它判定成败）。
fn read_head(socket: &mut std::net::TcpStream) -> Vec<u8> {
    let mut head = Vec::new();
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        let mut chunk = [0u8; 512];
        match socket.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => head.extend_from_slice(&chunk[..read]),
        }
    }
    head
}

/// **静默服务端**：accept 之后一个字节都不写。
///
/// 只读不写，客户端放弃（`reqwest` 的 future 被丢弃 ⇒ 对端 `read` 返回 0）线程才退出，
/// 不在用例进程里留一条永久阻塞的线程。
fn serve_silent() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("绑定回环端口");
    let url = listener_url(&listener);
    std::thread::spawn(move || {
        if let Ok((mut socket, _)) = listener.accept() {
            let mut sink = [0u8; 512];
            while matches!(socket.read(&mut sink), Ok(read) if read > 0) {}
        }
    });
    url
}

/// **只回头不回正文**的服务端：声明 `Content-Length: 64`，然后一个正文字节都不发。
///
/// 这条钉的是 `Response::text()` 那一半：头一到，`send()` 就有界地成功了，卡住的是正文读。
fn serve_headers_only() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("绑定回环端口");
    let url = listener_url(&listener);
    std::thread::spawn(move || {
        let Ok((mut socket, _)) = listener.accept() else {
            return;
        };
        let _head = read_head(&mut socket);
        let _ = socket.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 64\r\n\r\n",
        );
        let mut sink = [0u8; 512];
        while matches!(socket.read(&mut sink), Ok(read) if read > 0) {}
    });
    url
}

/// **正常服务端**：回一条 200 + 正文 + `Retry-After`，并把收到的请求头带出去。
fn serve_once(body: &'static str) -> (String, Arc<Mutex<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("绑定回环端口");
    let url = listener_url(&listener);
    let seen = Arc::new(Mutex::new(String::new()));
    let record = Arc::clone(&seen);
    std::thread::spawn(move || {
        let Ok((mut socket, _)) = listener.accept() else {
            return;
        };
        let head = read_head(&mut socket);
        *record.lock().expect("锁没中毒") = String::from_utf8_lossy(&head).into_owned();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nRetry-After: 7\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let _ = socket.write_all(response.as_bytes());
    });
    (url, seen)
}

/// ① 静默服务端：**在上界内回 `Err`**，不是永远转圈。
///
/// 改前这里会挂死（裸 `send().await`，引擎 client 只有 `connect_timeout`，而连接是通的）——
/// 于是外层 [`HANG_GUARD`] 到点就是红。
#[tokio::test]
async fn a_server_that_never_answers_returns_an_error_instead_of_hanging() {
    let bound = Duration::from_millis(200);
    let engine = engine(bound, bound);
    let url = serve_silent();

    let started = Instant::now();
    let attempt =
        tokio::time::timeout(HANG_GUARD, EngineTransport::new(&engine).get(&url, &[])).await;
    let elapsed = started.elapsed();

    let error = attempt
        .expect("静默服务端 ⇒ 请求没有上界（这条用例的变异证明：拆掉 send_bounded 就红在这里）")
        .expect_err("静默服务端必须回 Err");
    assert!(elapsed < HANG_GUARD, "界是 {bound:?}，实际等了 {elapsed:?}");
    assert_eq!(
        error.kind(),
        &CfErrorKind::Network,
        "超时仍归 cf_network（用户动作是「检查网络」）：{}",
        error.message()
    );
    let message = error.message();
    assert!(
        message.contains("await_headers"),
        "错误串必须带失速阶段（不是一句「超时了」）：{message}"
    );
    assert!(
        message.contains("等响应头") && message.contains("已收 0 字节"),
        "失速现场要带阶段 + 已收字节：{message}"
    );
}

/// ② 回了头但正文不动：`Response::text()` 那一半同样有界，且带「等字节」现场。
#[tokio::test]
async fn a_body_that_never_arrives_returns_an_error_instead_of_hanging() {
    let bound = Duration::from_millis(300);
    let engine = engine(bound, bound);
    let url = serve_headers_only();

    let started = Instant::now();
    let attempt =
        tokio::time::timeout(HANG_GUARD, EngineTransport::new(&engine).get(&url, &[])).await;
    let elapsed = started.elapsed();

    let error = attempt
        .expect("正文无限等待 ⇒ text_bounded 没接上（拆掉它这里会红）")
        .expect_err("正文读不出来必须是 Err");
    assert!(elapsed < HANG_GUARD, "界是 {bound:?}，实际等了 {elapsed:?}");
    assert_eq!(error.kind(), &CfErrorKind::Network);
    let message = error.message();
    assert!(
        message.contains("await_bytes"),
        "阶段应是「等字节」：{message}"
    );
    assert!(
        message.contains("预期 64 字节"),
        "已收/预期两个数要从 Content-Length 推出来：{message}"
    );
}

/// ③ 正常响应仍然成功：状态码、`Retry-After`、正文、请求头透传与 UA 一个都没被改造动过。
#[tokio::test]
async fn a_normal_response_still_parses_and_carries_the_engine_headers() {
    let body = r#"{"data":[]}"#;
    let (url, seen) = serve_once(body);
    let engine = engine(Duration::from_secs(30), Duration::from_secs(30));

    let response = EngineTransport::new(&engine)
        .get(
            &url,
            &[(
                "x-api-key".to_string(),
                "test-key-not-a-real-credential".to_string(),
            )],
        )
        .await
        .expect("正常响应必须成功");

    assert_eq!(response.status, 200);
    assert!(response.is_success());
    assert_eq!(response.body, body);
    assert_eq!(
        response.retry_after,
        Some(7),
        "Retry-After 的提取逻辑不许在改造中变掉"
    );

    let head = seen.lock().expect("锁没中毒").clone();
    let lowered = head.to_ascii_lowercase();
    assert!(
        lowered.contains("x-api-key: test-key-not-a-real-credential"),
        "请求头必须逐条透传：{head}"
    );
    assert!(
        head.contains(engine.config().user_agent.as_str()),
        "UA 必须仍取引擎配置：{head}"
    );
}
