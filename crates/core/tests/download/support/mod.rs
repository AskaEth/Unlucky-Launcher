//! 测试支撑：临时目录、摘要工具、进程内极简 HTTP 服务器（零额外依赖）。
//!
//! 为什么自己写 mock server 而不用 wiremock：P0 阶段控制依赖面（Win7 Tier-3 构建 +
//! 单文件体积），一个 60 行的 `TcpListener` 足够表达「成功 / 500 / 校验不符 / 无视 Range」。

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// 断言用的小工具：唯一临时目录（进程内计数 + 时间戳，避免并行测试互相踩）。
pub fn temp_dir(tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let dir = std::env::temp_dir().join(format!("ull-download-test-{tag}-{stamp}-{unique}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建临时目录失败");
    dir
}

/// 算一段字节的 SHA-1（测试断言用）。
pub fn sha1_hex(bytes: &[u8]) -> String {
    use sha1::Digest;
    let digest = sha1::Sha1::digest(bytes);
    let mut out = String::with_capacity(40);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// 目录里（不含子目录）所有中间文件家族的名字（`.ullpart` 与 `.ullmeta.json`）。
pub fn part_files(dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(unlucky_core::download::PART_SUFFIX))
        .collect();
    names.sort();
    names
}

/// 把文件的修改时间挪到「`age` 之前」（清理/收养的年龄判定用）。
pub fn age_file(path: &std::path::Path, age: std::time::Duration) {
    let times = std::fs::FileTimes::new().set_modified(std::time::SystemTime::now() - age);
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("打开待改 mtime 的文件失败")
        .set_times(times)
        .expect("改 mtime 失败");
}

/// mock 服务器给一次请求的应答。
#[derive(Debug, Clone)]
pub struct MockResponse {
    /// HTTP 状态码。
    pub status: u16,
    /// 响应体。
    pub body: Vec<u8>,
    /// 是否声明 `Accept-Ranges: bytes` 并在收到 `Range` 时真的切段。
    pub range_support: bool,
    /// `ETag`（续传测试用）。
    pub etag: Option<String>,
    /// 返回 206 时故意把 `Content-Range` 写成从 0 开始（模拟段错位的坏服务器）。
    pub lie_content_range: bool,
}

impl MockResponse {
    /// 200 + 完整内容（支持 Range）。
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: body.into(),
            range_support: true,
            etag: Some("\"v1\"".to_string()),
            lie_content_range: false,
        }
    }

    /// 固定状态码 + 空体。
    pub fn status(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
            range_support: false,
            etag: None,
            lie_content_range: false,
        }
    }

    /// 声明不支持 Range（模拟部分镜像）。
    pub fn without_range(mut self) -> Self {
        self.range_support = false;
        self
    }

    /// 206 但 `Content-Range` 谎报（模拟段错位的坏 CDN）。
    pub fn with_wrong_content_range(mut self) -> Self {
        self.lie_content_range = true;
        self
    }
}

/// 进程内极简 HTTP/1.1 服务器：按脚本顺序返回响应，脚本用尽后一律 500。
pub struct MockServer {
    addr: SocketAddr,
    hits: Arc<AtomicUsize>,
    handle: tokio::task::JoinHandle<()>,
}

impl MockServer {
    /// 启动服务器（脚本是「每次请求一个响应」的队列）。
    pub async fn start(script: Vec<MockResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("绑定端口失败");
        let addr = listener.local_addr().expect("取本地地址失败");
        let hits = Arc::new(AtomicUsize::new(0));
        let script = Arc::new(std::sync::Mutex::new(script));
        let counter = Arc::clone(&hits);

        let handle = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let script = Arc::clone(&script);
                let counter = Arc::clone(&counter);
                tokio::spawn(async move {
                    let index = counter.fetch_add(1, Ordering::Relaxed);
                    let response = {
                        let script = script.lock().unwrap_or_else(|err| err.into_inner());
                        script.get(index).cloned()
                    };
                    let response = response.unwrap_or_else(|| MockResponse::status(500));
                    let _ = serve(stream, response).await;
                });
            }
        });

        Self { addr, hits, handle }
    }

    /// 第 `index` 个请求该用的 URL（换成不同路径可模拟「不同源」）。
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    /// 换 host 拼 URL（用 `localhost` vs `127.0.0.1` 造出两个不同源）。
    pub fn url_with(&self, host: &str, path: &str) -> String {
        format!("http://{host}:{}{}", self.addr.port(), path)
    }

    /// 端口号。
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// 已收到的请求数。
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }

    /// 关停。
    pub fn stop(self) {
        self.handle.abort();
    }
}

async fn serve(mut stream: TcpStream, response: MockResponse) -> std::io::Result<()> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let request = String::from_utf8_lossy(&buffer).to_ascii_lowercase();
    let range = request
        .lines()
        .find_map(|line| line.strip_prefix("range: bytes="))
        .map(str::trim)
        .and_then(parse_range_header);

    let total = response.body.len() as u64;
    let (status, headers, body) = match (range, response.range_support) {
        (Some((start, requested_end)), true) if start < total => {
            // 尊重请求里的结束偏移（分块下载必须逐段精确，否则组装后长度对不上）。
            let end = requested_end.unwrap_or(total - 1).min(total - 1);
            let slice = response.body[start as usize..=end as usize].to_vec();
            // 谎报模式：body 仍是正确的那一段，但头部声称从 0 开始（长度恰好 → 只有
            // 严格比对 Content-Range 才抓得住）。
            let (claimed_start, claimed_end) = if response.lie_content_range {
                (0, slice.len().saturating_sub(1) as u64)
            } else {
                (start, end)
            };
            (
                206,
                vec![
                    format!("Content-Range: bytes {claimed_start}-{claimed_end}/{total}"),
                    format!("Content-Length: {}", slice.len()),
                    "Accept-Ranges: bytes".to_string(),
                ],
                slice,
            )
        }
        (Some(_), true) => (
            416,
            vec![
                format!("Content-Range: bytes */{total}"),
                "Content-Length: 0".to_string(),
            ],
            Vec::new(),
        ),
        _ => {
            let mut headers = vec![format!("Content-Length: {total}")];
            if response.range_support {
                headers.push("Accept-Ranges: bytes".to_string());
            }
            (response.status, headers, response.body.clone())
        }
    };
    let mut head = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    if let Some(etag) = &response.etag {
        head.push_str(&format!("ETag: {etag}\r\n"));
    }
    for header in headers {
        head.push_str(&header);
        head.push_str("\r\n");
    }
    head.push_str("Connection: close\r\n\r\n");
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.flush().await?;
    Ok(())
}

/// 解析 `Range: bytes=<start>-<end>`（`end` 可缺省 = 到文件末尾）。
fn parse_range_header(value: &str) -> Option<(u64, Option<u64>)> {
    let (start, end) = value.split_once('-')?;
    let start = start.trim().parse::<u64>().ok()?;
    let end = end.trim();
    let end = if end.is_empty() {
        None
    } else {
        Some(end.parse::<u64>().ok()?)
    };
    Some((start, end))
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        206 => "Partial Content",
        404 => "Not Found",
        416 => "Range Not Satisfiable",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}
