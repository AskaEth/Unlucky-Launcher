//! 版本查询用的极小 HTTP 取文本工具（四家发行版的「一锤子 GET」共用）。
//!
//! 只有一件事：GET → 状态分类 → 整段文本。**下载不走这里**（那归 `core::download` 引擎），
//! 所以没有重试、没有镜像展开、没有进度事件——查清单失败就直接把错误抛给发行版回退链，
//! 让它换下一家（这是 D23「vendor 级失败自动降下一 vendor」的最短路径）。
//!
//! 两处 `await` 都有界（M290）：等响应头与等正文都走 `core::download::http` 的既有构造点
//! （`http::send_bounded` / `http::text_bounded`），与引擎里 `download_bytes` 同一套口径。

use crate::download::http;
use crate::download::stall::Deadlines;

use super::error::{JavaError, JavaErrorKind};

/// 取一段文本（`what` 是给用户看的名词，例如「华为云 openjdk 目录」）。
///
/// # Errors
///
/// 连不上 / 超时 → [`JavaErrorKind::Network`]；404 或 410 → [`JavaErrorKind::Metadata`]
/// （「这个东西上游没有」比「网络坏了」更准确，回退链据此换 vendor 而不是重试同一条）。
pub async fn get_text(
    client: &reqwest::Client,
    url: &str,
    what: &str,
) -> Result<String, JavaError> {
    get_text_with(client, url, what, http::default_deadlines()).await
}

/// 真正干活的那一层：两段界是参数（生产只用上面的出厂值，用例要把它压到毫秒级）。
///
/// 失败时 `message` 里带**阶段**：等响应头（`await_headers`）/ 等字节（`await_bytes`），
/// 外加上游那句错。这不是「请求失败」四个字——用户要能分辨「对面不说话」与「说到一半停住」。
async fn get_text_with(
    client: &reqwest::Client,
    url: &str,
    what: &str,
    deadlines: Deadlines,
) -> Result<String, JavaError> {
    let response = http::send_bounded(client.get(url), url, deadlines, None)
        .await
        .map_err(|error| {
            JavaError::new(JavaErrorKind::Network, format!("取 {what} 失败：{error}"))
        })?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
        return Err(JavaError::new(
            JavaErrorKind::Metadata,
            format!("{what} 不存在（HTTP {status}）：{url}"),
        ));
    }
    if !status.is_success() {
        return Err(JavaError::new(
            JavaErrorKind::Network,
            format!("{what} 返回 HTTP {status}：{url}"),
        ));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = http::text_bounded(response, url, deadlines)
        .await
        .map_err(|error| {
            JavaError::new(
                JavaErrorKind::Network,
                format!("读 {what} 响应失败：{error}"),
            )
        })?;
    // 门户兜底页会把「路径不存在」伪装成 200 + HTML；版本清单页面本身可能也是 HTML，
    // 所以这里不做内容判定，只看调用方能不能解析出来（解析失败会给出更准的原因）。
    if body.trim().is_empty() {
        return Err(JavaError::new(
            JavaErrorKind::Metadata,
            format!("{what} 返回空响应（Content-Type: {content_type}）：{url}"),
        ));
    }
    Ok(body)
}

/// 「接了连接不回话 / 停住 / 谎报长度 / 灌洪流」的回环服务端：M290 给**模块外**那几条一锤子
/// GET 配的定点探针（`game/java/api.rs`、`api/version.rs`、本模块、`update/github.rs`、以及
/// `download/http.rs` 的正文上界各用一次）。住在这里只因为 scope 里的文件只有本模块有余量。
///
/// 与 `tests/download/support` 的 `MockServer::silent` 是**两种夹具**：那一份服务集成测试里的
/// 引擎公开 API，这一份只能服务 `cfg(test)` 下的库里私有路径（`get_text_with` 等）。
#[cfg(test)]
pub(crate) mod probe {
    use std::io::{Read as _, Write as _};
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use crate::download::stall::Deadlines;

    /// 服务端的坏法（外加一种正常应答，用来证明这几条路径的正常语义没被改坏）。
    #[derive(Debug, Clone, Copy)]
    pub(crate) enum Mode {
        /// 读完请求头、**一个字节都不写**（对面接了连接就装死）。
        Silent,
        /// 头里声明 `declared` 字节、只发 `sent` 字节，然后停住（连接不断）。
        Declared { declared: u64, sent: usize },
        /// 不发 `Content-Length`，灌 `Flood` 那么多字节后关连接（超长流：只能靠读方自己卡）。
        Flood(u64),
        /// 正常应答：200 + 这段正文（`Content-Length` 如实声明）。
        Answer(&'static str),
    }

    /// 挂住的时长：比用例的外层护栏（[`GATE`]）长，读方不设界就只能被外层打断。
    const HOLD: Duration = Duration::from_secs(30);

    /// 外层护栏：用例调用外面再套这一层，**只在读方自己无界时才触发**。
    pub(crate) const GATE: Duration = Duration::from_secs(5);

    /// 用例用的两段界（200ms 量级）：与 [`GATE`] 差一个数量级，「引擎自己返回」与
    /// 「被外层打断」就分得开——断言里还要求耗时 < 2s，落在两者之间。
    pub(crate) fn tight() -> Deadlines {
        let short = Duration::from_millis(200);
        Deadlines {
            response: short,
            read_idle: short,
        }
    }

    /// 跑一次「被测调用」，外面套 [`GATE`] 护栏并带回耗时：**护栏只会在读方自己无界时触发**，
    /// 所以耗时远小于 `GATE` 就等于「它自己返回的」。`what` 是失败时那句 bug 说明。
    pub(crate) async fn bounded<T>(
        what: &str,
        step: impl std::future::Future<Output = T>,
    ) -> (T, Duration) {
        let started = std::time::Instant::now();
        let value = tokio::time::timeout(GATE, step)
            .await
            .unwrap_or_else(|_| panic!("{what}：外层 {GATE:?} 挂断（M290 修的就是这条）"));
        (value, started.elapsed())
    }

    /// 只在 `127.0.0.1` 上说坏话的服务端（线程泄漏到进程结束，用例之间互不影响）。
    pub(crate) struct Hanging {
        addr: SocketAddr,
        hits: Arc<AtomicUsize>,
    }

    impl Hanging {
        /// 起服务（每个连接一个线程：reqwest 可能重连，脚本不是一次性的）。
        pub(crate) fn start(mode: Mode) -> Self {
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("绑定回环端口");
            let addr = listener.local_addr().expect("取本地地址");
            let hits = Arc::new(AtomicUsize::new(0));
            let counter = Arc::clone(&hits);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut socket) = stream else { break };
                    counter.fetch_add(1, Ordering::SeqCst);
                    std::thread::spawn(move || {
                        let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
                        let mut seen = Vec::new();
                        loop {
                            let mut chunk = [0_u8; 1024];
                            match socket.read(&mut chunk) {
                                Ok(0) | Err(_) => return,
                                Ok(read) => seen.extend_from_slice(&chunk[..read]),
                            }
                            if seen.windows(4).any(|window| window == b"\r\n\r\n") {
                                break;
                            }
                        }
                        match mode {
                            Mode::Declared { declared, sent } => {
                                // 头里说 `declared`、正文只给 `sent` 字节：读方按空闲判定就该收手；
                                // `declared` 也可以离谱到远超上限（那种情况读方必须连正文都不读）。
                                let head = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                                     Content-Length: {declared}\r\n\r\n"
                                );
                                socket.write_all(head.as_bytes()).expect("写响应头失败");
                                let _ = socket.write_all(&vec![b'{'; sent]);
                                let _ = socket.flush();
                            }
                            Mode::Flood(total) => {
                                // 不声明长度（靠关连接分帧）：读方只能自己卡上限。
                                let chunk = vec![b'x'; 64 * 1024];
                                socket
                                    .write_all(
                                        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\n",
                                    )
                                    .expect("写响应头失败");
                                for _ in 0..total.div_ceil(chunk.len() as u64) {
                                    if socket.write_all(&chunk).is_err() {
                                        return;
                                    }
                                }
                                return;
                            }
                            Mode::Answer(body) => {
                                let head = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\n\
                                     Content-Length: {}\r\nConnection: close\r\n\r\n",
                                    body.len()
                                );
                                socket.write_all(head.as_bytes()).expect("写响应头失败");
                                let _ = socket.write_all(body.as_bytes());
                                let _ = socket.flush();
                                return;
                            }
                            Mode::Silent => {}
                        }
                        // 无论回没回字节，都把连接攥住：这就是「无界 `await`」在真网络上的形态。
                        std::thread::sleep(HOLD);
                    });
                }
            });
            Self { addr, hits }
        }

        /// 指到这个服务端上的一个地址。
        pub(crate) fn url(&self, path: &str) -> String {
            format!("http://{}{path}", self.addr)
        }

        /// 已经收到几个请求。
        pub(crate) fn hits(&self) -> usize {
            self.hits.load(Ordering::SeqCst)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::probe::{bounded, tight, Hanging, Mode};
    use super::{get_text, get_text_with};
    use crate::game::java::error::JavaErrorKind;

    /// 服务端接了连接、一个字不回：`get_text` 必须自己返回，且说清卡在**等响应头**。
    ///
    /// 变异证明：把 `get_text_with` 的 `send_bounded` 退回裸 `client.get(url).send()`
    /// ⇒ 护栏在外层 5s 处 panic（文案就是那句 bug 说明），红。
    #[tokio::test]
    async fn a_silent_server_is_bounded_and_the_stage_is_named() {
        let server = Hanging::start(Mode::Silent);
        let (result, elapsed) = bounded(
            "get_text 的 send() 又无界了",
            get_text_with(
                &reqwest::Client::new(),
                &server.url("/list.json"),
                "华为云 openjdk 目录",
                tight(),
            ),
        )
        .await;
        assert!(
            elapsed < Duration::from_secs(2),
            "必须自己返回，实际 {elapsed:?}"
        );
        let error = result.expect_err("silent 服务端必须失败");
        assert_eq!(error.kind(), JavaErrorKind::Network);
        assert!(
            error.message().contains("等响应头") && error.message().contains("await_headers"),
            "要说清卡在哪一段：{}",
            error.message()
        );
        assert_eq!(server.hits(), 1, "只该问一次（这里没有重试）");
    }

    /// 头到手、正文挤出几个字节就停住：正文那一段同样有界，阶段是**等字节**。
    ///
    /// 变异证明：把 `text_bounded` 换回 `response.text()` ⇒ 护栏在外层 5s 处 panic，红。
    #[tokio::test]
    async fn a_stalled_body_is_bounded_and_says_await_bytes() {
        let server = Hanging::start(Mode::Declared {
            declared: 4096,
            sent: 16,
        });
        let (result, elapsed) = bounded(
            "正文读取又无界了",
            get_text_with(
                &reqwest::Client::new(),
                &server.url("/list.json"),
                "Zulu 包列表",
                tight(),
            ),
        )
        .await;
        assert!(
            elapsed < Duration::from_secs(2),
            "应在读空闲附近返回，实际 {elapsed:?}"
        );
        let error = result.expect_err("说到一半停住必须失败");
        assert!(
            error.message().contains("等字节") && error.message().contains("await_bytes"),
            "要说清卡在哪一段：{}",
            error.message()
        );
    }

    /// 正常路径不动：真回了一段 JSON 就原样交出来（含中文，证明解码没被打坏）。
    ///
    /// 变异证明：把 `text_bounded` 的解码换成按字节硬塞 ⇒ 中文那条断言红。
    #[tokio::test]
    async fn a_normal_response_is_handed_over_unchanged() {
        let body = r#"{"major": 21, "note": "中文也要活"}"#;
        let server = Hanging::start(Mode::Answer(body));
        let text = get_text(&reqwest::Client::new(), &server.url("/ok.json"), "正常清单")
            .await
            .expect("正常响应不该失败");
        assert_eq!(text, body, "正文必须逐字节交出来");
        assert!(text.contains("中文也要活"));
    }

    /// `game/java/api.rs` 那条 latest 查询的 HTTP 跳（`fetch_latest` → `fetch_body`）：两处
    /// `await` 同样有界、阶段同样说得出。**用例住在这里**而不是 `api.rs`：那份文件贴着 400 行
    /// 红线，而它测的正是本模块这几行同款 GET（同一套构造点）。
    ///
    /// 变异证明：把 `fetch_body` 的 `send_bounded` 退回裸 `send()` ⇒ 护栏在外层 5s 处 panic，红。
    #[tokio::test]
    async fn the_adoptium_latest_hop_is_bounded_and_the_stage_is_named() {
        let server = Hanging::start(Mode::Silent);
        let (result, elapsed) = bounded(
            "fetch_latest 的 send() 又无界了",
            crate::game::java::api::fetch_body(
                &reqwest::Client::new(),
                21,
                &server.url("/assets/latest/21/hotspot"),
                tight(),
            ),
        )
        .await;
        assert!(
            elapsed < Duration::from_secs(2),
            "必须自己返回，实际 {elapsed:?}"
        );
        let error = result.expect_err("silent 服务端必须失败");
        assert_eq!(error.kind(), JavaErrorKind::Network);
        assert!(
            error.message().contains("等响应头") && error.message().contains("await_headers"),
            "要说清卡在哪一段：{}",
            error.message()
        );
    }

    /// 正常路径不变：200 + 真 JSON 时 `fetch_body` 逐字节交出正文（`fetch_latest` 就靠它解析）。
    #[tokio::test]
    async fn the_adoptium_latest_hop_still_returns_the_body_on_200() {
        let body = r#"[{"release_name":"jdk-21.0.13+9"}]"#;
        let server = Hanging::start(Mode::Answer(body));
        let text = crate::game::java::api::fetch_body(
            &reqwest::Client::new(),
            21,
            &server.url("/assets/latest/21/hotspot"),
            tight(),
        )
        .await
        .expect("200 不该失败");
        assert_eq!(text, body);
    }

    /// `update/github.rs` 取更新清单那一跳（`fetch_manifest_documents` → `candidate_text`）：
    /// 同样是「一锤子 GET 拿一段 JSON」。**用例住在这里**而不是 `github.rs`：它内部是 `block_on`
    /// 的同步签名——在 tokio 用例里直接调它会嵌套 `block_on` panic，只有这一层能直接测到。
    ///
    /// 变异证明：把 `candidate_text` 的 `send_bounded` 退回裸 `send()` ⇒ 护栏在外层 5s 处 panic，红。
    #[tokio::test]
    async fn the_manifest_fetch_is_bounded_and_the_stage_is_named() {
        let server = Hanging::start(Mode::Silent);
        let (result, elapsed) = bounded(
            "取更新清单的 send() 又无界了",
            crate::update::github::candidate_text(
                &reqwest::Client::new(),
                &server.url("/manifest.json"),
                tight(),
            ),
        )
        .await;
        assert!(
            elapsed < Duration::from_secs(2),
            "必须自己返回，实际 {elapsed:?}"
        );
        let error = result.expect_err("silent 服务端必须失败");
        assert_eq!(error.kind, crate::update::UpdateErrorKind::Network);
        assert!(
            error.message.contains("等响应头") && error.message.contains("await_headers"),
            "要说清卡在哪一段：{}",
            error.message
        );
    }
}
