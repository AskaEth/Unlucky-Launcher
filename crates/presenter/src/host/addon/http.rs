//! HTTP 取文本（内容库唯一的网络出口）。
//!
//! ## 两条路径，为什么不是一条
//!
//! | 请求 | 走哪 | 拿到什么 |
//! |---|---|---|
//! | 无自定义 header（[`get_via_engine`]，**生产零调用者**：内容库的 Modrinth 走 core 的 `EngineTransport`，同一条 `download_bytes`） | [`DownloadEngine::download_bytes`] | **信号量限流 + 源健康度 + 指数退避 + 分类错误**，全是 core 现成的 |
//! | 带 header（只有 CurseForge 的 `X-API-KEY`） | `engine.client()` 直发 | 引擎的请求体没有 header 字段，只能自己发；代价是这条路径自己带一个 429/5xx 退避 |
//!
//! 两条路径用的是**同一个引擎**，而引擎是进程级共享的（M165：`host::engine::engine` →
//! `core::download::shared::engine_for`，一个数据根一个）。所以 Modrinth 与 CurseForge 的请求、
//! 以及它们与真正的文件下载，共用同一条连接池与同一套限流；`block_on` 也跑在同一个常驻运行时上，
//! 池里的空闲连接不会随着某次调用结束而变成死连接。
//!
//! ## UA 只有一份定义（M207）
//!
//! `User-Agent` 定义在 core 的 `download::http::USER_AGENT`：`Unlucky-Launcher/<CARGO_PKG_VERSION>
//! (+https://github.com/AskaEth/Unlucky-Launcher)`。本文件**不拼** UA——带 header 的路径发的是
//! `engine.config().user_agent`，无 header 的路径发的是同一个客户端的默认 UA，所以两条路径、以及真正的
//! 文件下载，站点看到的都是同一个字符串。
//!
//! M205 复审核实过的旧状：`api.modrinth.com` 收到的是引擎那份 `UnluckyLauncher/<ver>`（无连字符、
//! 无联系方式），带联系方式的 `Unlucky-Launcher/<ver> (+repo)` 只发给 CurseForge 搜索——而 Modrinth 的
//! UA 政策正是「带联系方式，否则限流」，也就是说那条政策在**真正会被限流的那条路上**没被满足。两份 UA
//! 同时活着、改一处不带另一处，所以 M207 把定义收进 core 一处。
//!
//! 钉住它的是回环上的字节级用例 `both_download_paths_send_the_same_user_agent`：两条路径发出去的
//! `User-Agent` 必须逐字节相同，且等于 `EngineConfig::default().user_agent`。
//!
//! ## 密钥纪律
//!
//! 带 header 的那条路径**只把 header 值喂给请求**：不进日志、不进错误串。错误串里只有 URL
//! （CF 的 URL 不含密钥）与状态码。

use std::time::Duration;

use unlucky_core::download::blocking::block_on;
use unlucky_core::download::config::DEFAULT_MAX_ATTEMPTS_PER_SOURCE;
use unlucky_core::download::{DownloadRequest, ResourceKind};

use super::super::env::HostEnv;

/// 最多重试次数（含首次）。
///
/// **不在这里写数字**：取值就是下载引擎的出厂默认
/// （`core::download::config::DEFAULT_MAX_ATTEMPTS_PER_SOURCE`，`download.retry_limit` 的 schema
/// 默认值同源），改工厂默认时这里跟着走。
///
/// 边界说清：这条路径**没读**宿主设置（次数是编译期常量），所以固定吃出厂默认——**不是拿不到**。
/// `env` 就在手上（`get_with_headers` 第一句 `engine(env)` 已经灌过调优），用户当前值就在
/// `engine.config().max_attempts_per_source`；要让它跟随 `download.retry_limit`，读那个字段即可，
/// 不需要新拉一条参数链。另一条无 header 的路径（[`get_via_engine`]）走引擎，本来就跟随设置
/// （源冷却中时引擎只给 1 次，那是恢复探测，见 `DownloadEngine::attempts_for`）。
const ATTEMPTS: u32 = DEFAULT_MAX_ATTEMPTS_PER_SOURCE;

/// 取一段文本（JSON 原文）。
pub(super) fn get(env: &HostEnv, url: &str, headers: &[(&str, &str)]) -> Result<String, String> {
    if headers.is_empty() {
        get_via_engine(env, url)
    } else {
        get_with_headers(env, url, headers)
    }
}

/// core 的下载引擎：限流与退避都在里面，这里只负责「URL → 文本」。
fn get_via_engine(env: &HostEnv, url: &str) -> Result<String, String> {
    let engine = super::super::engine::engine(env);
    let dir = env.layout.cache().join("content");
    std::fs::create_dir_all(&dir).map_err(|error| format!("建缓存目录失败：{error}"))?;
    let request = DownloadRequest::new(ResourceKind::Other, url, dir.join("response.json"));
    let bytes = block_on(engine.download_bytes(request))
        .map_err(|error| format!("请求 {url} 失败：{error}"))?;
    String::from_utf8(bytes).map_err(|error| format!("响应不是 UTF-8：{error}"))
}

/// 带 header 的请求（只有 CF 走这条）：429 / 5xx / 连接类失败按退避重试。
fn get_with_headers(env: &HostEnv, url: &str, headers: &[(&str, &str)]) -> Result<String, String> {
    let engine = super::super::engine::engine(env);
    // UA 取自引擎配置（**不在这里拼**）：定义只有 core 那一份，这条路径与引擎路径发的必须是同一个字符串。
    let user_agent = engine.config().user_agent.clone();
    let owned: Vec<(String, String)> = headers
        .iter()
        .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
        .collect();

    for attempt in 0..ATTEMPTS {
        let result = block_on(async {
            let mut builder = engine
                .client()
                .get(url)
                .header("User-Agent", user_agent.as_str());
            for (name, value) in &owned {
                builder = builder.header(name.as_str(), value.as_str());
            }
            let response = builder.send().await.map_err(|error| error.to_string())?;
            let status = response.status();
            if status.is_success() {
                return response.text().await.map_err(|error| error.to_string());
            }
            let retry = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            Err(format!("HTTP {} (retry_after={retry:?})", status.as_u16()))
        });

        match result {
            Ok(body) => return Ok(body),
            Err(reason) if attempt + 1 < ATTEMPTS && retryable(&reason) => {
                std::thread::sleep(retry_after(&reason, attempt));
            }
            Err(reason) => return Err(format!("请求 {url} 失败：{reason}")),
        }
    }
    Err(format!("请求 {url} 失败：重试用尽"))
}

/// 这个失败值得再试一次吗（429 / 5xx / 连接类；**401/403/404 一律不重试**，重试只会更糟）。
pub(super) fn retryable(reason: &str) -> bool {
    reason.contains("HTTP 429")
        || reason.contains("HTTP 500")
        || reason.contains("HTTP 502")
        || reason.contains("HTTP 503")
        || reason.contains("HTTP 504")
        || reason.contains("error sending request")
        || reason.contains("timed out")
}

/// 退避时长：优先吃服务端给的 `Retry-After`（封顶 30s），否则指数退避（1s、2s、4s）。
pub(super) fn retry_after(reason: &str, attempt: u32) -> Duration {
    let parsed = reason
        .split("retry_after=Some(")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .and_then(|value| value.parse::<u64>().ok());
    match parsed {
        Some(secs) => Duration::from_secs(secs.min(30)),
        None => Duration::from_secs(1u64 << attempt.min(4)),
    }
}

/// 最小百分号编码（query 串用；不为一件小事引第三方 crate）。
pub(super) fn encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::time::Duration;

    use unlucky_core::download::shared;

    use super::get;
    use crate::host::env::HostEnv;

    /// 离线回环 HTTP 服务：只答固定 JSON，把每条请求的**原始请求头**送回主线程。
    ///
    /// 用 127.0.0.1 而不是真站点：**单测不许打公网**（CI 没网，也不该有）。回传原始头部是为了抓
    /// 「站点实际收到的 `User-Agent`」——那是唯一能证明两条路径发同一个字符串的地方。
    fn loopback(expected_requests: usize) -> (String, std::sync::mpsc::Receiver<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定回环端口");
        let addr = listener.local_addr().expect("取端口");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut heads: Vec<String> = Vec::new();
            while heads.len() < expected_requests {
                let Ok((mut socket, _)) = listener.accept() else {
                    break;
                };
                let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
                let mut seen = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    match socket.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => seen.extend_from_slice(&chunk[..read]),
                    }
                    if !seen.windows(4).any(|window| window == b"\r\n\r\n") {
                        continue;
                    }
                    heads.push(String::from_utf8_lossy(&seen).into_owned());
                    let body = br#"{"ok":true}"#;
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    );
                    if socket.write_all(reply.as_bytes()).is_err()
                        || socket.write_all(body).is_err()
                    {
                        break;
                    }
                    seen.clear();
                    if heads.len() >= expected_requests {
                        let _ = tx.send(heads);
                        return;
                    }
                }
            }
            let _ = tx.send(heads);
        });
        (format!("http://{addr}/v2/search"), rx)
    }

    /// 一条原始请求头里 `User-Agent` 的值（大小写不敏感：reqwest 发的是小写字段名）。
    fn user_agent_of(head: &str) -> Option<String> {
        head.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("user-agent")
                .then(|| value.trim().to_string())
        })
    }

    /// 一个只用来放缓存根的临时数据根（`<root>/.ULL/cache`）。
    fn data_root(tag: &str) -> std::path::PathBuf {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("ull-http-{tag}-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ULL").join("cache")).expect("建缓存根");
        dir
    }

    /// 两条下载路径发出去的 `User-Agent` 必须是**同一个字符串**，且就是引擎配置里那一个（M207）。
    ///
    /// 为什么读回环上的字节、而不是断言常量相等：`header("User-Agent", …)` 到底传了什么，只有抓包才算
    /// 「站点看到的事实」；断言两个常量相等只说明「借了同一个名字」，谁在发送处又拼了一份照旧全绿。
    ///
    /// 三段内容的正面断言在 core（`download::http` 的用例
    /// `the_user_agent_carries_the_package_version_and_the_repo_url`：版本 == `CARGO_PKG_VERSION`、
    /// 联系方式段 == 仓库 URL、不含邮箱）。这里钉的是**两条路径同源**：无 header 那条发引擎 reqwest
    /// 客户端的默认 UA，带 header 那条发 `engine.config().user_agent`，两者都必须对
    /// `EngineConfig::default()` 逐字节相等。
    ///
    /// 抓得住：presenter 又自己拼一份 UA（哪怕只差产品名的连字符）⇒ 红；发送处把 header 删掉或写成空串 ⇒ 红；
    /// 客户端的默认 UA 与配置字段分叉（`build_client` 不再读 `cfg.user_agent`）⇒ 红。
    /// 抓不住：真实 Modrinth / CurseForge 收到这条 UA 之后怎么限流（单测不出网）；也抓不住「联系方式
    /// 那一串是不是站点认可的格式」——那要真发一次才知道。
    #[test]
    fn both_download_paths_send_the_same_user_agent() {
        let dir = data_root("ua");
        let env = HostEnv::at_root(&dir);
        let (url, seen) = loopback(2);

        get(&env, &url, &[]).expect("无 header 路径（引擎 `download_bytes`）");
        get(&env, &url, &[("x-api-key", "test-key")])
            .expect("带 header 路径（`engine.client()` 直发）");

        let heads = seen
            .recv_timeout(Duration::from_secs(10))
            .expect("回环服务应回传请求头");
        let sent: Vec<String> = heads
            .iter()
            .filter_map(|head| user_agent_of(head))
            .collect();
        assert_eq!(sent.len(), 2, "两条请求都要带 User-Agent：{heads:?}");
        assert_eq!(sent[0], sent[1], "两条路径的 UA 必须是同一个字符串");

        let defined = unlucky_core::download::config::EngineConfig::default().user_agent;
        assert_eq!(
            sent[0], defined,
            "发出去的 UA 必须就是引擎配置里那一个（定义只有 core 一份）"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **端到端守卫（M165 复审备注 2）**：连走三条真实宿主请求路径，只该构造一个引擎。
    ///
    /// 为什么不满足于「再断言一次 `engine()` 复用」：那种断言只钉漏斗本身——将来谁在调用点
    /// 自己 `DownloadEngine::new`，它照旧全绿。这里数的是 `shared::constructions`，而记账卡在
    /// `DownloadEngine::new` 这个卡口上（与谁造的无关），所以只要有一条路径另造引擎就会红。
    ///
    /// 三条路径：
    /// 1. [`get`] 无 header = [`get_via_engine`]（走 `engine.download_bytes`）。**生产零调用者**：内容库的
    ///    Modrinth 元数据 GET 走 core 的 `EngineTransport`（同一条 `download_bytes`），不经本文件；
    /// 2. [`get`] 带 header = CurseForge 那条（走 `engine.client()`，另一条机制）；
    /// 3. `host::java::vendors` = Java 页渲染（走 `domestic()`，非 HTTP 的漏斗消费者）。
    ///
    /// 没走 `versions` 清单：它的地址是写死的 launchermeta，单测里要么打真网、要么吃满连接超时
    /// —— 那正是「测试里不该发生的事」，所以选了同样真实但完全离线的三条。
    #[test]
    fn three_host_request_paths_share_one_engine() {
        let dir = data_root("paths");
        let env = HostEnv::at_root(&dir);
        let cache_root = env.layout.cache();
        let (url, seen) = loopback(2);

        let without_headers = get(&env, &url, &[]).expect("无 header 路径（`get_via_engine`）");
        let with_headers =
            get(&env, &url, &[("x-api-key", "test-key")]).expect("带 header 路径（CF）");
        let vendors = crate::host::java::vendors(&env, &[]);

        assert_eq!(
            without_headers, with_headers,
            "两条路径读的是同一份回环响应"
        );
        assert!(!vendors.is_empty(), "Java 页应有发行版行");
        let heads = seen
            .recv_timeout(Duration::from_secs(10))
            .expect("回环服务应回传请求头");
        assert_eq!(heads.len(), 2, "两条 HTTP 路径各发一条请求");
        assert_eq!(
            shared::constructions(&cache_root),
            1,
            "三条宿主请求路径必须共用同一个引擎（谁绕过漏斗另造一个，这条就红）"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
