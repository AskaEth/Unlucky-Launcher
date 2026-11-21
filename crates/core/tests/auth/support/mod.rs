//! `core::auth` 集成测试夹具：临时目录 + 进程内 HTTP 服务器（零额外依赖）。
//!
//! 为什么要 mock 服务器：微软/authlib 两条轨的正确性全在**报文级**（`XBL3.0 x=<uhs>;<xsts>`
//! 的拼接、XSTS 401 带 `XErr`、`/minecraft/profile` 404 的二义复判、Yggdrasil 的
//! `clientToken` 回传校验）——只有把整条链打到一个真实 socket 上，这些分支才被真的跑到。
//!
//! 服务器按 **path + 顺序队列**回应答：同一 path 的第 N 次请求用第 N 个应答
//! （设备码轮询要靠这个：`authorization_pending` → `slow_down` → 成功）。
//! 微软轨的 `MicrosoftAuth` / 路由表 / 断言助手拆在 [`microsoft`] 子模块里（M38）。

#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub mod microsoft;
pub use microsoft::{
    assert_no_credentials, auth_for, expired_session, full_routes, response_of, routes_until_login,
    HTML_BLOCK_PAGE,
};

/// 唯一临时目录（进程内计数 + 时间戳，避免并行测试互踩）。
pub fn temp_dir(tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let dir = std::env::temp_dir().join(format!("ull-auth-test-{tag}-{stamp}-{unique}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建临时目录失败");
    dir
}

/// 一次 mock 应答。
#[derive(Debug, Clone)]
pub struct MockReply {
    /// HTTP 状态码。
    pub status: u16,
    /// 响应体（JSON 文本或空）。
    pub body: String,
    /// `Content-Type`（默认 `application/json`；拦截页场景可改成 `text/html`）。
    pub content_type: &'static str,
}

impl MockReply {
    /// 200 + JSON。
    pub fn json(value: serde_json::Value) -> Self {
        Self {
            status: 200,
            body: value.to_string(),
            content_type: "application/json",
        }
    }

    /// 指定状态码 + JSON。
    pub fn json_status(status: u16, value: serde_json::Value) -> Self {
        Self {
            status,
            body: value.to_string(),
            content_type: "application/json",
        }
    }

    /// 指定状态码 + 空体。
    pub fn status(status: u16) -> Self {
        Self {
            status,
            body: String::new(),
            content_type: "application/json",
        }
    }

    /// 指定状态码 + HTML 拦截页（`Content-Type: text/html`）——M38 的网络分流用。
    pub fn html(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_string(),
            content_type: "text/html; charset=utf-8",
        }
    }

    /// 指定状态码 + 纯文本（网关页的另一种常见形状）。
    pub fn text(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_string(),
            content_type: "text/plain",
        }
    }
}

/// 收到的一次请求（断言报文形状用）。
#[derive(Debug, Clone)]
pub struct Recorded {
    /// 方法（大写）。
    pub method: String,
    /// 路径（不含 query）。
    pub path: String,
    /// 请求体（表单或 JSON 原文）。
    pub body: String,
}

/// 进程内 HTTP/1.1 服务器（按 path 排队回应答）。
pub struct AuthMock {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<Recorded>>>,
    handle: tokio::task::JoinHandle<()>,
}

impl AuthMock {
    /// 启动服务器。`routes` 是 `(path, 该 path 依次使用的应答队列)`。
    pub async fn start(routes: Vec<(&str, Vec<MockReply>)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("绑定端口");
        let addr = listener.local_addr().expect("取地址");
        let requests: Arc<Mutex<Vec<Recorded>>> = Arc::new(Mutex::new(Vec::new()));
        let queues: Arc<Mutex<HashMap<String, VecDeque<MockReply>>>> = Arc::new(Mutex::new(
            routes
                .into_iter()
                .map(|(path, replies)| (path.to_string(), replies.into()))
                .collect(),
        ));

        let recorded = Arc::clone(&requests);
        let queues_for_task = Arc::clone(&queues);
        let handle = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let recorded = Arc::clone(&recorded);
                let queues = Arc::clone(&queues_for_task);
                tokio::spawn(async move {
                    let _ = serve(stream, recorded, queues).await;
                });
            }
        });

        Self {
            addr,
            requests,
            handle,
        }
    }

    /// 基址（喂给 `MicrosoftEndpoints::all_on` / `AuthlibEndpoints` 的根）。
    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// 端口。
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// 已收到的请求（按到达顺序）。
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// 找某 path 的请求体列表。
    pub fn bodies_on(&self, path: &str) -> Vec<String> {
        self.requests()
            .into_iter()
            .filter(|request| request.path == path)
            .map(|request| request.body)
            .collect()
    }

    /// 关停。
    pub fn stop(self) {
        self.handle.abort();
    }
}

async fn serve(
    mut stream: TcpStream,
    recorded: Arc<Mutex<Vec<Recorded>>>,
    queues: Arc<Mutex<HashMap<String, VecDeque<MockReply>>>>,
) -> std::io::Result<()> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 2048];
    // 读头
    let header_end = loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = find_headers_end(&buffer) {
            break position;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let content_length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.trim().eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    while buffer.len() < header_end + content_length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8_lossy(
        &buffer[header_end..(header_end + content_length).min(buffer.len())],
    )
    .to_string();

    let mut first_line = head.lines().next().unwrap_or_default().split(' ');
    let method = first_line.next().unwrap_or_default().to_ascii_uppercase();
    let raw_path = first_line.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/").to_string();

    recorded
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .push(Recorded {
            method,
            path: path.clone(),
            body,
        });

    let reply = queues
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get_mut(&path)
        .and_then(VecDeque::pop_front)
        .unwrap_or_else(|| MockReply::status(500));

    let mut response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status,
        reason(reply.status),
        reply.content_type,
        reply.body.len()
    )
    .into_bytes();
    response.extend_from_slice(reply.body.as_bytes());
    stream.write_all(&response).await?;
    stream.flush().await?;
    Ok(())
}

fn find_headers_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Status",
    }
}
