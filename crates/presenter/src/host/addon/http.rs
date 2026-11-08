//! HTTP 取文本（内容库唯一的网络出口）。
//!
//! ## 两条路径，为什么不是一条
//!
//! | 请求 | 走哪 | 拿到什么 |
//! |---|---|---|
//! | 无自定义 header（Modrinth 的全部请求） | [`DownloadEngine::download_bytes`] | **信号量限流 + 源健康度 + 指数退避 + 分类错误**，全是 core 现成的 |
//! | 带 header（只有 CurseForge 的 `X-API-KEY`） | `engine.client()` 直发 | 引擎的请求体没有 header 字段，只能自己发；代价是这条路径自己带一个 429/5xx 退避 |
//!
//! ## 密钥纪律
//!
//! 带 header 的那条路径**只把 header 值喂给请求**：不进日志、不进错误串。错误串里只有 URL
//! （CF 的 URL 不含密钥）与状态码。

use std::time::Duration;

use unlucky_core::download::blocking::block_on;
use unlucky_core::download::{DownloadRequest, ResourceKind};

use super::super::env::HostEnv;

/// 站点要求的 User-Agent（Modrinth 明文要求带联系方式，否则限流）。
pub(super) const USER_AGENT: &str =
    "Unlucky-Launcher/0.1 (+https://github.com/AskaEth/Unlucky-Launcher)";

/// 最多重试次数（含首次）。
const ATTEMPTS: u32 = 3;

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
    let owned: Vec<(String, String)> = headers
        .iter()
        .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
        .collect();

    for attempt in 0..ATTEMPTS {
        let result = block_on(async {
            let mut builder = engine.client().get(url).header("User-Agent", USER_AGENT);
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
