//! **代理真的接上了吗**（M295）：本地假代理 + 引擎的**生产装配路径**。
//!
//! 判据是「请求真的经由代理」而不是「配置对象里存了个地址」——后者正是这条债原本的形态
//! （`settings` 里存着 `manual` + 地址，`build_client` 从不 `.proxy()`，全绿）。
//!
//! 三档各一条：
//!
//! | 用例 | 环境 | 期望 |
//! |---|---|---|
//! | [`manual_mode_really_sends_the_request_through_the_proxy`] | 无 | 目标域名**不可能解析**，仍拿到正文 ⇒ 只可能来自假代理；且代理记到的请求行是**绝对 URI** |
//! | [`direct_ignores_the_env_proxy_and_system_follows_it`] | `HTTP_PROXY` 指向假代理 | `system` 拿到正文（经由代理）；**同一个环境**下 `direct` 拿不到（`.no_proxy()`），且代理一条新请求都没收到 |
//! | [`an_unparseable_proxy_url_falls_back_to_direct_and_says_why`] | 无 | 不 panic；`proxy_problem()` 有可读原因；配置字段仍是用户写的 `manual`（不撒谎） |
//!
//! 变异（只改实现侧，跑完即还原）：
//! * 删掉 `ProxySetting::apply` 里的 `.proxy(proxy)` ⇒ 第 1 条红（其余两条绿）；
//! * 删掉 `ProxySetting::apply` 里的 `.no_proxy()` ⇒ 第 2 条红（`direct` 也会经代理成功）；
//! * 把 `ProxySetting::System` 改成 `.no_proxy()` ⇒ 第 2 条红（`system` 拿不到正文）。
//!
//! 抓不住：真机上的「系统代理」（Windows 注册表 / macOS 偏好设置，本仓 reqwest 没开
//! `client-proxy-system`，`system` 只认环境变量）、PAC 脚本、需要认证的代理、HTTPS 目标的
//! `CONNECT` 隧道（本夹具只讲明文 HTTP）。

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use unlucky_core::download::proxy::ProxySetting;

use super::*;

/// 本文件的用例**串行**跑：`direct_ignores_the_env_proxy_and_system_follows_it` 会改进程环境变量
/// （全局状态），并发跑时邻座的客户端会跟着读到它——那会让失败原因变成「别人的环境」，而不是
/// 自己被变异的那一行。三个用例都先拿这把锁（同 `core::update` 的环境变量用例手法）。
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 用**异步**互斥锁而不是 `std::sync::Mutex`：三个用例都是 `#[tokio::test]`，持着标准锁跨越
/// `await` 会被 `clippy::await_holding_lock` 拦下（本仓 clippy 是 `-D warnings`）。
async fn serial() -> tokio::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().await
}

/// 假代理：只回一份固定正文，并把每条请求的**第一行**记下来。
///
/// 为什么记请求行就够：明文 HTTP 走代理时请求行是**绝对 URI**（`GET http://host/path HTTP/1.1`），
/// 直连永远只有 `/path`。所以「记到绝对 URI」等价于「这条请求是代理发出的」。
struct FakeProxy {
    addr: std::net::SocketAddr,
    lines: Arc<Mutex<Vec<String>>>,
    handle: tokio::task::JoinHandle<()>,
}

impl FakeProxy {
    async fn start(body: &'static [u8]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("绑定假代理端口");
        let addr = listener.local_addr().expect("取假代理地址");
        let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&lines);
        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let seen = Arc::clone(&seen);
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut chunk = [0u8; 1024];
                    loop {
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(read) => head.extend_from_slice(&chunk[..read]),
                        }
                        if head.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let text = String::from_utf8_lossy(&head);
                    let first = text.lines().next().unwrap_or_default().to_string();
                    seen.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .push(first);
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(header.as_bytes()).await;
                    let _ = socket.write_all(body).await;
                });
            }
        });
        Self {
            addr,
            lines,
            handle,
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn request_lines(&self) -> Vec<String> {
        self.lines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl Drop for FakeProxy {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// 一个引擎 + 一份缓存根（与 `engine_mock_test.rs` 的 `harness` 同一形状，只多了一个代理档）。
fn engine_with_proxy(proxy: ProxySetting, tag: &str) -> Harness {
    harness(
        EngineConfig {
            proxy,
            ..fast_config(1)
        },
        MirrorTable::builtin(),
        direct_region(),
        tag,
    )
}

/// 目标 URL 用 RFC 2606 的保留域名 `.invalid`：**不可能解析**，所以「拿到正文」只可能是因为
/// 请求经由假代理（DNS 不会骗人，除非真走了代理）。
const TARGET: &str = "http://proxywire.invalid/meta.json";
const BODY: &[u8] = b"proxied payload";

fn request_for(harness: &Harness) -> DownloadRequest {
    DownloadRequest::new(ResourceKind::Other, TARGET, harness.dir.join("meta.json"))
}

/// **manual 档真的走代理**（任务书的头号判据）。
///
/// 变异：删掉 `ProxySetting::apply` 里那一行 `.proxy(proxy)` ⇒ 目标域名解析失败，`expect` 当场红。
#[tokio::test]
async fn manual_mode_really_sends_the_request_through_the_proxy() {
    let _serial = serial().await;
    let proxy = FakeProxy::start(BODY).await;
    let harness = engine_with_proxy(
        ProxySetting::Manual { url: proxy.url() },
        "engine-proxy-manual",
    );

    let bytes = harness
        .engine
        .download_bytes(request_for(&harness))
        .await
        .expect("manual 档必须能经由代理拿到正文（否则 .proxy() 那行没生效）");
    assert_eq!(bytes, BODY, "拿到的必须是假代理回的那份字节");

    let lines = proxy.request_lines();
    assert_eq!(lines.len(), 1, "应该恰好一次经由代理的请求：{lines:?}");
    assert!(
        lines[0].starts_with("GET http://proxywire.invalid/meta.json"),
        "走代理的请求行必须是**绝对 URI**（这是「经由代理」的直接证据）：{lines:?}"
    );
    assert!(
        harness.engine.proxy_problem().is_none(),
        "地址合法时不该有代理问题：{:?}",
        harness.engine.proxy_problem()
    );
    std::fs::remove_dir_all(&harness.dir).ok();
}

/// **direct 不读环境变量、system 跟随环境变量**（同一份环境、同一个目标，只有设置不同）。
///
/// 这里动的是**进程环境变量**（全局状态），所以用一把进程内锁串起来、退出时还原原值；同时把
/// `NO_PROXY` 钉成回环，让同进程里其它打回环 HTTP 的用例（`engine_mock` 的兄弟文件）绕过假代理，
/// 不被这一条污染。
///
/// 变异：`Direct` 少了 `.no_proxy()` ⇒ 第一段红（`.invalid` 会经由假代理「成功」，代理多记一行）；
/// `System` 被写成 `.no_proxy()` ⇒ 第二段红（拿不到正文）。
#[tokio::test]
async fn direct_ignores_the_env_proxy_and_system_follows_it() {
    let _serial = serial().await;

    /// 记住两个变量的原值，`Drop` 时还原（用例 panic 也还原）。
    struct Restore(Option<std::ffi::OsString>, Option<std::ffi::OsString>);
    impl Drop for Restore {
        fn drop(&mut self) {
            for (key, value) in [("HTTP_PROXY", self.0.clone()), ("NO_PROXY", self.1.clone())] {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    let proxy = FakeProxy::start(BODY).await;
    let _restore = Restore(std::env::var_os("HTTP_PROXY"), std::env::var_os("NO_PROXY"));
    std::env::set_var("HTTP_PROXY", proxy.url());
    std::env::set_var("NO_PROXY", "127.0.0.1,localhost");

    // ① system：跟随环境变量里的 HTTP_PROXY ⇒ 经由假代理拿到正文。
    let system = engine_with_proxy(ProxySetting::System, "engine-proxy-system");
    let bytes = system
        .engine
        .download_bytes(request_for(&system))
        .await
        .expect("system 档必须跟随环境变量里的代理");
    assert_eq!(bytes, BODY);
    let lines = proxy.request_lines();
    assert_eq!(lines.len(), 1, "system 档应该经由代理一次：{lines:?}");
    std::fs::remove_dir_all(&system.dir).ok();

    // ② direct：**同一个环境变量、同一个目标** ⇒ 一个字节都不许经代理。
    let direct = engine_with_proxy(ProxySetting::Direct, "engine-proxy-direct");
    let error = direct
        .engine
        .download_bytes(request_for(&direct))
        .await
        .expect_err("direct 档不许经由环境变量里的代理拿到 .invalid 的正文");
    assert_eq!(
        proxy.request_lines().len(),
        lines.len(),
        "direct 档一条请求都不许发给代理（`.no_proxy()` 没生效）：{:?}",
        proxy.request_lines()
    );
    assert_eq!(
        error.kind,
        DownloadErrorKind::AllSourcesFailed,
        "回落成「这个域名连不上」的有界失败，不是别的：{}",
        error.message
    );
    std::fs::remove_dir_all(&direct.dir).ok();
}

/// **非法地址不得穿透**（任务书第 4 条）：不 panic、有可读原因、配置不撒谎、回落是直连。
///
/// 变异：把 `build_client` 里 `cfg.proxy.apply(builder)?` 的 `?` 改成 `expect` ⇒ 本用例 panic（红）；
/// 把引擎的回落改成 `reqwest::Client::new()`（跟随环境变量）⇒ `proxy_problem()` 仍在但回落不确定
/// （本机没有代理时这条断言照样绿——所以「回落是直连」由 `direct` 用例的 `.no_proxy()` 那半边兜，
/// 这里只说清「配置没被偷偷改掉」这一条）。
#[tokio::test]
async fn an_unparseable_proxy_url_falls_back_to_direct_and_says_why() {
    let _serial = serial().await;
    let broken = ProxySetting::Manual {
        url: "http://127.0.0.1:99999".to_string(),
    };
    let harness = engine_with_proxy(broken.clone(), "engine-proxy-broken");

    let problem = harness
        .engine
        .proxy_problem()
        .expect("地址解析不了必须留下原因（宿主靠它告诉用户）");
    assert!(problem.contains("http://127.0.0.1:99999"), "{problem}");
    assert!(problem.contains("代理地址用不了"), "{problem}");
    assert!(problem.contains("端口 99999"), "原因要能读：{problem}");
    assert!(!problem.contains("builder error"), "{problem}");
    assert_eq!(
        harness.engine.config().proxy,
        broken,
        "配置字段仍是用户写的 manual（回落是运行时行为，不是偷偷改配置）"
    );

    // 回落直连 ⇒ 请求打到一个不可能解析的域名上是有界失败，既不是 panic 也不是「用了某个代理」。
    let error = harness
        .engine
        .download_bytes(request_for(&harness))
        .await
        .expect_err("直连打 .invalid 必须失败");
    assert_eq!(error.kind, DownloadErrorKind::AllSourcesFailed);
    std::fs::remove_dir_all(&harness.dir).ok();
}

/// **`socks5://` 这类「合法但本构建用不上」的地址**（r1 复审 p3-1）。
///
/// 以前 `Proxy::all("socks5://…")` 返回 `Ok` ⇒ `proxy_problem()` 是 `None` ⇒ 用户只看到「所有候选源
/// 都失败了」，把原因推给源（socks5 在国内很常见）。现在它是**有界错误**：走同一条「回落直连 +
/// 原因留在引擎上」的路，且文案点名 scheme、不含口令。
///
/// 变异：删掉 `manual_proxy` 里的 scheme 白名单 ⇒ 本用例第一句红（`proxy_problem()` 变 `None`）。
#[tokio::test]
async fn an_unsupported_proxy_scheme_is_refused_with_a_readable_reason() {
    let _serial = serial().await;
    const SECRET: &str = "PWMUSTNOTLEAK";
    let socks = ProxySetting::Manual {
        url: format!("socks5://user:{SECRET}@127.0.0.1:1080"),
    };
    let harness = engine_with_proxy(socks.clone(), "engine-proxy-socks");

    let problem = harness
        .engine
        .proxy_problem()
        .expect("本构建用不了的 scheme 必须留下原因");
    assert!(problem.contains("不支持 socks5"), "{problem}");
    assert!(
        problem.contains("只支持 http / https"),
        "文案要说清能用什么：{problem}"
    );
    assert!(!problem.contains(SECRET), "回落提示泄漏口令：{problem}");
    assert_eq!(
        harness.engine.config().proxy,
        socks,
        "配置字段仍是用户写的值"
    );

    // 回落直连 ⇒ 有界失败，不是 panic。
    let error = harness
        .engine
        .download_bytes(request_for(&harness))
        .await
        .expect_err("回落直连打 .invalid 必须失败");
    assert_eq!(error.kind, DownloadErrorKind::AllSourcesFailed);
    std::fs::remove_dir_all(&harness.dir).ok();
}
