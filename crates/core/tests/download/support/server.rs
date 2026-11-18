//! 进程内极简 HTTP/1.1 夹具服务器（零额外依赖）。
//!
//! 为什么自己写而不用 wiremock：P0 阶段控制依赖面（Win7 Tier-3 构建 + 单文件体积），一个
//! `TcpListener` + 脚本队列足够表达「成功 / 500 / 校验不符 / 无视 Range / 中途卡住」。
//!
//! 正文写出的字节数记在 [`MockServer::bytes_sent`] 里：续传用例的判据是「服务端到底吐了
//! 多少字节」，只看最终 hash 抓不到「其实整份又下了一遍」（见 `chunked/resume.rs`）。
//!
//! 从 `support/mod.rs` 拆出只为守 400 行硬线（M293）。

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub use super::response::MockResponse;

/// 夹具「挂住」的时长：远大于用例自己的上界，保证读方只能靠超时脱身。
const HOLD: Duration = Duration::from_secs(30);

/// 进程内极简 HTTP/1.1 服务器：按脚本顺序返回响应，脚本用尽后一律 500。
pub struct MockServer {
    addr: SocketAddr,
    hits: Arc<AtomicUsize>,
    bytes: Arc<std::sync::atomic::AtomicU64>,
    if_ranges: Arc<std::sync::Mutex<Vec<String>>>,
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
        let bytes = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let if_ranges = Arc::new(std::sync::Mutex::new(Vec::new()));
        let script = Arc::new(std::sync::Mutex::new(script));
        let counter = Arc::clone(&hits);
        let body_bytes = Arc::clone(&bytes);
        let seen_if_ranges = Arc::clone(&if_ranges);

        let handle = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let script = Arc::clone(&script);
                let counter = Arc::clone(&counter);
                let bytes = Arc::clone(&body_bytes);
                let seen = Arc::clone(&seen_if_ranges);
                tokio::spawn(async move {
                    let index = counter.fetch_add(1, Ordering::Relaxed);
                    let response = {
                        let script = script.lock().unwrap_or_else(|err| err.into_inner());
                        script.get(index).cloned()
                    };
                    let response = response.unwrap_or_else(|| MockResponse::status(500));
                    let _ = serve(stream, response, &bytes, &seen).await;
                });
            }
        });

        Self {
            addr,
            hits,
            bytes,
            if_ranges,
            handle,
        }
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

    /// 已发出的**正文字节**总数（不含响应头）。
    ///
    /// 续传用例的判据靠它：「最终 hash 对」证明不了没白下字节，只有统计服务端真正吐了多少
    /// 才知道是不是又整份下了一遍（见 `chunked/resume.rs`）。
    pub fn bytes_sent(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    /// 收到过的 `If-Range` 取值（按到达顺序）。
    ///
    /// 换源用例的判据：源 A 的校验器**不许**出现在源 B 的请求里。夹具照 RFC 7233 §3.2 处置
    /// （对不上就忽略 Range、回 200 + 完整正文），所以这条也顺带把「真实服务器上的后果」跑出来。
    pub fn if_range_values(&self) -> Vec<String> {
        self.if_ranges
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// 关停。
    pub fn stop(self) {
        self.handle.abort();
    }
}

async fn serve(
    mut stream: TcpStream,
    response: MockResponse,
    sent: &std::sync::atomic::AtomicU64,
    seen_if_ranges: &std::sync::Mutex<Vec<String>>,
) -> std::io::Result<()> {
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
    let if_range = request
        .lines()
        .find_map(|line| line.strip_prefix("if-range:"))
        .map(|value| value.trim().to_string());
    if let Some(value) = &if_range {
        seen_if_ranges
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(value.clone());
    }
    // RFC 7233 §3.2：`If-Range` 与当前表示对不上（含「服务器根本没有校验器」）⇒ **忽略 Range**，
    // 回 200 + 完整正文。夹具照此处置，于是「拿别的源的校验器去问」会真的走到
    // `fetch_chunk` 的 `RangeUnsupported` 分支（那就是真实服务器上退化成整份重下的那一跳）。
    let validator_ok = if_range.as_deref().is_some_and(|sent_value| {
        response
            .etag
            .as_deref()
            .is_some_and(|etag| sent_value.eq_ignore_ascii_case(etag))
            // ETag 缺失时按弱校验器（HTTP-date）比：这是 `If-Range` 的兜底支路。
            || response
                .last_modified
                .as_deref()
                .is_some_and(|date| sent_value.eq_ignore_ascii_case(date))
    });
    let range = if response.range_support && if_range.is_some() && !validator_ok {
        None
    } else {
        range
    };

    if response.silent {
        // 接了连接、读完请求头、一个字都不回：读方不设上界就永远挂在这里。
        tokio::time::sleep(HOLD).await;
        return Ok(());
    }

    let total = response.body.len() as u64;
    // 超长包不能发 `Content-Length`，否则读方按声明截断、永远看不到多余字节。
    let with_length = response.trailing_extra.is_none();
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
            let mut headers = vec![
                format!("Content-Range: bytes {claimed_start}-{claimed_end}/{total}"),
                "Accept-Ranges: bytes".to_string(),
            ];
            if with_length {
                headers.push(format!("Content-Length: {}", slice.len()));
            }
            (206, headers, slice)
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
            let mut headers = Vec::new();
            if with_length {
                headers.push(format!("Content-Length: {total}"));
            }
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
    if let Some(date) = &response.last_modified {
        head.push_str(&format!("Last-Modified: {date}\r\n"));
    }
    for header in headers {
        head.push_str(&header);
        head.push_str("\r\n");
    }
    head.push_str("Connection: close\r\n\r\n");
    stream.write_all(head.as_bytes()).await?;
    write_body(&mut stream, &body, &response, sent).await?;
    stream.flush().await?;
    Ok(())
}

/// 按夹具的口径写正文：正常 / 卡在中途 / 短包 / 超长尾巴 / 滴流。发出的正文逐块计数。
async fn write_body(
    stream: &mut TcpStream,
    body: &[u8],
    response: &MockResponse,
    sent: &std::sync::atomic::AtomicU64,
) -> std::io::Result<()> {
    if let Some(stall_at) = response.stall_at {
        // 头部 + 这么多字节已发出，之后一个字不再写（连接不断）：读方只能靠超时脱身。
        let sent_len = stall_at.min(body.len());
        stream.write_all(&body[..sent_len]).await?;
        stream.flush().await?;
        sent.fetch_add(sent_len as u64, Ordering::Relaxed);
        tokio::time::sleep(HOLD).await;
        return Ok(());
    }
    let limit = response.send_len.unwrap_or(body.len()).min(body.len());
    match response.drip {
        Some((piece, gap)) => {
            let piece = piece.max(1);
            let mut offset = 0;
            while offset < limit {
                let end = (offset + piece).min(limit);
                stream.write_all(&body[offset..end]).await?;
                stream.flush().await?;
                sent.fetch_add((end - offset) as u64, Ordering::Relaxed);
                offset = end;
                if offset < limit {
                    tokio::time::sleep(gap).await;
                }
            }
        }
        None => {
            stream.write_all(&body[..limit]).await?;
            stream.flush().await?;
            sent.fetch_add(limit as u64, Ordering::Relaxed);
        }
    }
    if let Some(extra) = response.trailing_extra {
        let filler = vec![0_u8; extra];
        stream.write_all(&filler).await?;
        stream.flush().await?;
        sent.fetch_add(extra as u64, Ordering::Relaxed);
    }
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
