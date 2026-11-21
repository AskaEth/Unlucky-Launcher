//! 内容库的 HTTP 出口**守卫**（M317 之后本文件只剩用例）。
//!
//! ## 为什么生产代码搬空了
//!
//! M109 时本文件是内容库唯一的网络出口，有两条路径：无 header 的（走引擎
//! [`DownloadEngine::download_bytes`]）与带 header 的（CurseForge 的 `X-API-KEY`，`engine.client()`
//! 直发 + 自己一套 429/5xx 退避）。M317 把 CurseForge 的检索交给 core 的
//! `addon::curseforge::CurseForge`：URL 拼装、`x-api-key` 请求头、429 退避与 JSON 解析全在那边，
//! 于是「presenter 自己发带 header 的请求」这条路径**没有调用者了**——连同它的手写退避
//! （`retryable` / `retry_after`）与百分号编码（`encode`，core 的 `online::modrinth::encode`
//! 早就是唯一实现）一起退役。
//!
//! 留下的不是「空文件」，而是两条**仍然成立**的判断，它们必须有个地方可跑：
//!
//! | 用例 | 守的是什么 |
//! |------|------------|
//! | [`tests::both_download_paths_send_the_same_user_agent`] | 内容库实际在用的两条 HTTP 机制（引擎的 `download_bytes` 与 CF 客户端的传输层）发出去的 `User-Agent` 逐字节相同，且等于引擎配置里那一个（M207：UA 定义只有 core 一处） |
//! | [`tests::three_host_request_paths_share_one_engine`] | 连走三条宿主请求路径只构造一个引擎（M165：谁绕过 `host::engine::engine` 另造一个 client，这条就红） |
//!
//! 两条都用 127.0.0.1 回环（**不出公网**）：要证明「站点收到的字节是什么」，只有抓包算数；
//! 断言两个常量相等只能说明「借了同一个名字」。M317 新增的 CurseForge 接线用例
//! （`tests/retired_curseforge.rs`）反过来一个 socket 都不开——它们喂的是录制的夹具。

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::time::Duration;

    use unlucky_core::addon::curseforge::{CfTransport, EngineTransport};
    use unlucky_core::download::blocking::block_on;
    use unlucky_core::download::shared;
    use unlucky_core::download::{DownloadRequest, ResourceKind};

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

    /// 内容库在用的两条 HTTP 机制各发一条请求，回两份响应正文（路径 A / 路径 B）。
    ///
    /// 路径 A = 引擎的 `download_bytes`（Modrinth 元数据那条，M162 起走 core 的
    /// `online::EngineTransport`，与这里同一条 `download_bytes`）；
    /// 路径 B = CurseForge 客户端的传输层（M317 起检索走它）。
    fn two_paths(env: &HostEnv, url: &str) -> (String, String) {
        let engine = crate::host::engine::engine(env);
        let dir = env.layout.cache().join("content");
        std::fs::create_dir_all(&dir).expect("建缓存目录");
        let request = DownloadRequest::new(ResourceKind::Other, url, dir.join("response.json"));
        let without_headers = block_on(engine.download_bytes(request)).expect("路径 A：引擎");
        let transport = EngineTransport::new(&engine);
        let response = block_on(transport.get(url, &[])).expect("路径 B：CF 传输层");
        assert_eq!(response.status, 200, "回环应答 200");
        (
            String::from_utf8(without_headers).expect("路径 A 是 UTF-8"),
            response.body,
        )
    }

    /// 两条路径发出去的 `User-Agent` 必须是**同一个字符串**，且就是引擎配置里那一个（M207）。
    ///
    /// 为什么读回环上的字节、而不是断言常量相等：`header("User-Agent", …)` 到底传了什么，只有抓包才算
    /// 「站点看到的事实」；断言两个常量相等只说明「借了同一个名字」，谁在发送处又拼了一份照旧全绿。
    ///
    /// 三段内容的正面断言在 core（`download::http` 的用例
    /// `the_user_agent_carries_the_package_version_and_the_repo_url`：版本 == `CARGO_PKG_VERSION`、
    /// 联系方式段 == 仓库 URL、不含邮箱）。这里钉的是**两条路径同源**。
    ///
    /// 抓得住：CF 传输层又自己拼一份 UA（哪怕只差产品名的连字符）⇒ 红；发送处把 header 删掉或写成
    /// 空串 ⇒ 红；客户端的默认 UA 与配置字段分叉（`build_client` 不再读 `cfg.user_agent`）⇒ 红。
    /// 抓不住：真实 Modrinth / CurseForge 收到这条 UA 之后怎么限流（单测不出网）。
    #[test]
    fn both_download_paths_send_the_same_user_agent() {
        let dir = data_root("ua");
        let env = HostEnv::at_root(&dir);
        let (url, seen) = loopback(2);

        let _ = two_paths(&env, &url);

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
    /// 1. 引擎的 `download_bytes`（Modrinth 元数据那条）；
    /// 2. CurseForge 客户端的传输层（M317 起检索走它，走 `engine.client()`）；
    /// 3. `host::java::vendors` = Java 页渲染（走 `domestic()`，非 HTTP 的漏斗消费者）。
    #[test]
    fn three_host_request_paths_share_one_engine() {
        let dir = data_root("paths");
        let env = HostEnv::at_root(&dir);
        let cache_root = env.layout.cache();
        let (url, seen) = loopback(2);

        let (without_headers, with_headers) = two_paths(&env, &url);
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
