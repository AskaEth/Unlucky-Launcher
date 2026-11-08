//! 授权码流的本地回调服务器（`research/08` §3 方案 A：HMCL 主流做法）。
//!
//! 只干一件事：等浏览器把 `?code=` 送到 `http://localhost:{port}/auth`，把 code 交出去。
//! 协议面故意压到最小（只认一个 GET 路径、只回一个静态页），这样它能被单测直接驱动。
//!
//! # 四条容易踩的细节
//!
//! 1. **重定向 URI 用 `localhost`，监听绑 `127.0.0.1`**：微软对 `localhost` 与 `127.0.0.1`
//!    处理不一致（坑 5），URI 只能写前者；浏览器把 `localhost` 解析到 `::1` 时会被拒一次
//!    再回落 IPv4，Chrome/Edge 都走这条回退，所以只绑 v4 就够。
//! 2. **端口由内核分配**（绑定 `:0`），端口被占时按 `attempts` 重试；授权 URL 里的
//!    `redirect_uri` 必须用**最终绑到的端口**回填，换 token 时再原样带上（URI 严格一致）。
//! 3. **非 `/auth` 的请求（`/favicon.ico`、浏览器预连）回 404 后继续等**：让一个 favicon
//!    把整次登录判成失败是最容易犯的错。
//! 4. **`state` 不匹配的请求一律回 400 后继续等**——包括带 `error=` 的那些：旧标签页的
//!    迟到回调不该打断新的登录，更要紧的是 RFC 6749 §4.1.2.1 / RFC 9207 要求**错误响应同样
//!    要校验 `state`**；否则任何人 `GET /auth?error=access_denied` 就能静默中止别人的登录。
//!    真正的超时由调用方的时间预算兜底。

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::auth::error::{AuthError, AuthResult};

/// 回调路径（重定向 URI 的路径段）。
pub const REDIRECT_PATH: &str = "/auth";
/// 一次回调请求最多读这么多字节（超了就丢弃连接，防被灌内存）。
pub const MAX_REQUEST_HEAD: usize = 8 * 1024;
/// 端口被占用时的重试间隔。
const RETRY_DELAY: Duration = Duration::from_millis(20);
/// 单条连接读完请求头的上限（浏览器/预连不把请求头发完时不能把整个登录卡死）。
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(5);

const SUCCESS_PAGE: &str = "<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\">\
<title>登录完成</title></head><body style=\"font-family:system-ui;padding:40px\">\
<h1>登录完成</h1><p>可以关闭此窗口，回到启动器继续。</p></body></html>";
const DENIED_PAGE: &str = "<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\">\
<title>登录未完成</title></head><body style=\"font-family:system-ui;padding:40px\">\
<h1>登录未完成</h1><p>授权没有完成，可以回到启动器重试。</p></body></html>";
const BAD_REQUEST_PAGE: &str = "<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\">\
<title>回调无效</title></head><body style=\"font-family:system-ui;padding:40px\">\
<h1>回调无效</h1><p>这个链接不属于本次登录。</p></body></html>";
const NOT_FOUND_PAGE: &str = "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<title>Not Found</title></head><body><h1>Not Found</h1></body></html>";

/// 本地回调服务器（一个实例只服务一次登录）。
#[derive(Debug)]
pub struct CallbackServer {
    listener: TcpListener,
    port: u16,
    state: String,
}

impl CallbackServer {
    /// 绑定回调端口。
    ///
    /// `port` 为 `None` 时由内核分配（生产路径）；给定端口时按 `attempts` 重试
    /// （CLI/测试里固定端口用，端口被占会退避再试）。
    ///
    /// # Errors
    ///
    /// 全部尝试都绑定失败时返回 [`AuthError::Network`]（文案带端口与重试次数）。
    pub async fn bind(
        port: Option<u16>,
        attempts: u8,
        state: impl Into<String>,
    ) -> AuthResult<Self> {
        let wanted = port.unwrap_or(0);
        let attempts = attempts.max(1);
        let mut last = String::new();
        for attempt in 0..attempts {
            match TcpListener::bind(("127.0.0.1", wanted)).await {
                Ok(listener) => {
                    let port = listener
                        .local_addr()
                        .map_err(|error| AuthError::network("localhost", error.to_string()))?
                        .port();
                    return Ok(Self {
                        listener,
                        port,
                        state: state.into(),
                    });
                }
                Err(error) => {
                    last = error.to_string();
                    if attempt + 1 < attempts {
                        tokio::time::sleep(RETRY_DELAY).await;
                    }
                }
            }
        }
        let what = if wanted == 0 {
            "本地回调端口分配失败".to_string()
        } else {
            format!("本地回调端口 {wanted} 绑定失败（重试 {attempts} 次）")
        };
        Err(AuthError::network("localhost", format!("{what}：{last}")))
    }

    /// 实际端口（回填 `redirect_uri` 用）。
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 本次登录的 `state`（授权 URL 与回调校验共用）。
    #[must_use]
    pub fn state(&self) -> &str {
        &self.state
    }

    /// 等浏览器把 code 送到 `/auth`，最多等 `timeout`。
    ///
    /// # Errors
    ///
    /// 超时（[`AuthError::Timeout`]）、用户在授权页点了取消（[`AuthError::Canceled`]）、
    /// 授权页回了别的 `error`（[`AuthError::Denied`]）、socket 出错（[`AuthError::Network`]）。
    pub async fn wait_for_code(self, timeout: Duration) -> AuthResult<String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(wait_timeout(timeout));
            }
            let accepted = tokio::time::timeout(remaining, self.listener.accept()).await;
            let (mut stream, _) = match accepted {
                Ok(Ok(pair)) => pair,
                Ok(Err(error)) => {
                    return Err(AuthError::network("localhost", error.to_string()));
                }
                Err(_) => return Err(wait_timeout(timeout)),
            };
            // 一条连接最多读这么久请求头：慢客户端/半开连接不能把整个登录拖过时间预算。
            // 预算按 **accept 之后** 的剩余时间重算（用 accept 前那份会让总时长超发最多 5s）。
            let budget = REQUEST_READ_TIMEOUT
                .min(deadline.saturating_duration_since(tokio::time::Instant::now()));
            if budget.is_zero() {
                return Err(wait_timeout(timeout));
            }
            match tokio::time::timeout(budget, self.handle(&mut stream)).await {
                Ok(result) => {
                    if let Some(code) = result? {
                        return Ok(code);
                    }
                }
                Err(_) => continue,
            }
        }
    }

    /// 处理一条连接：`Some(code)` = 登录完成，`None` = 与本次登录无关，继续等。
    async fn handle(&self, stream: &mut TcpStream) -> AuthResult<Option<String>> {
        let Some(target) = read_request_target(stream).await else {
            return Ok(None);
        };
        let (path, query) = match target.split_once('?') {
            Some((path, query)) => (path, query),
            None => (target.as_str(), ""),
        };
        if path != REDIRECT_PATH {
            respond(stream, 404, NOT_FOUND_PAGE).await;
            return Ok(None);
        }
        let params = Query::parse(query);
        // state 必须先校验，且**先于** error：错误响应同样要认 state（RFC 6749 §4.1.2.1 /
        // RFC 9207）。否则任意一次 `GET /auth?error=access_denied`（state 随便给或不给）就能
        // 把这台机器上正在进行的登录静默中止 —— 不需要知道随机 state、不需要 code。
        if params.get("state") != Some(self.state.as_str()) {
            respond(stream, 400, BAD_REQUEST_PAGE).await;
            return Ok(None);
        }
        if let Some(error) = params.get("error") {
            respond(stream, 200, DENIED_PAGE).await;
            return Err(oauth_error(error, params.get("error_description")));
        }
        let Some(code) = params.get("code").filter(|code| !code.is_empty()) else {
            respond(stream, 400, BAD_REQUEST_PAGE).await;
            return Ok(None);
        };
        respond(stream, 200, SUCCESS_PAGE).await;
        Ok(Some(code.to_string()))
    }
}

fn wait_timeout(timeout: Duration) -> AuthError {
    AuthError::Timeout {
        what: format!("等待浏览器回调（上限 {} 秒）", timeout.as_secs()),
    }
}

/// 授权页回的 `error` → 错误分流（`access_denied` 归取消，其余归拒绝）。
fn oauth_error(code: &str, description: Option<&str>) -> AuthError {
    if matches!(code, "access_denied" | "authorization_declined") {
        return AuthError::Canceled;
    }
    AuthError::Denied {
        code: code.to_string(),
        description: description.unwrap_or("授权页没有给出原因").to_string(),
    }
}

/// 读请求行里的 target（`/auth?code=...`）。非 GET/HEAD、超长头、连接中断都给 `None`。
async fn read_request_target(stream: &mut TcpStream) -> Option<String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        if let Some(end) = find_headers_end(&buffer) {
            let head = String::from_utf8_lossy(&buffer[..end]).into_owned();
            let mut parts = head.lines().next()?.split(' ');
            let method = parts.next()?;
            if !method.eq_ignore_ascii_case("GET") && !method.eq_ignore_ascii_case("HEAD") {
                return None;
            }
            return parts.next().map(str::to_string);
        }
        if buffer.len() > MAX_REQUEST_HEAD {
            return None;
        }
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return None,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }
    }
}

fn find_headers_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

async fn respond(stream: &mut TcpStream, status: u16, page: &str) {
    let response = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: text/html; charset=utf-8\r\n\
Content-Length: {}\r\nConnection: close\r\n\r\n{page}",
        reason(status),
        page.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Status",
    }
}

/// 回调 query 的最小解析（`k=v` 对，percent 解码）。
struct Query(Vec<(String, String)>);

impl Query {
    fn parse(raw: &str) -> Self {
        Self(
            raw.split('&')
                .filter(|pair| !pair.is_empty())
                .filter_map(|pair| {
                    let (key, value) = pair.split_once('=')?;
                    Some((decode(key), decode(value)))
                })
                .collect(),
        )
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }
}

/// `percent` 解码（`+` 视为空格；坏转义原样保留）。
fn decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let parsed = std::str::from_utf8(&bytes[index + 1..index + 3])
                    .ok()
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok());
                match parsed {
                    Some(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    None => {
                        out.push(b'%');
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{decode, oauth_error, Query};
    use crate::auth::error::AuthError;

    #[test]
    fn query_decodes_percent_and_plus() {
        let query = Query::parse("code=ab%2Fcd&state=s+1&empty=");
        assert_eq!(query.get("code"), Some("ab/cd"));
        assert_eq!(query.get("state"), Some("s 1"));
        assert_eq!(query.get("empty"), Some(""));
        assert_eq!(query.get("missing"), None);
    }

    #[test]
    fn decode_keeps_broken_escapes() {
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%zz"), "%zz");
        assert_eq!(decode("a%20b"), "a b");
    }

    #[test]
    fn access_denied_maps_to_canceled() {
        assert_eq!(oauth_error("access_denied", None), AuthError::Canceled);
        match oauth_error("invalid_request", Some("bad scope")) {
            AuthError::Denied { code, description } => {
                assert_eq!(code, "invalid_request");
                assert_eq!(description, "bad scope");
            }
            other => panic!("应是 Denied，实际 {other:?}"),
        }
    }
}
