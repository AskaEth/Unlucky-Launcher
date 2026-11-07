//! HTTP 薄封装：客户端构造、错误分类、响应头读取。
//!
//! 关键取舍：
//! * **`Accept-Encoding: identity`** —— 下载要按字节对齐做 Range/校验，gzip 会让
//!   `Content-Length` 与实际写入量打架（HMCL 为此专门写了按解压后偏移计数的
//!   `HttpResumeContext`，analyse/03 §5.4），我们直接从协议层绕开这个问题；
//! * 重定向上限 20 跳（承 HMCL），跳转后仍按最终 URL 做 host 校验由调用方负责；
//! * 错误分类只做「同源重试 / 立刻换源 / 整体失败」三态，不做细颗粒映射。

use reqwest::header::{
    HeaderMap, HeaderName, ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED,
};
use reqwest::redirect::Policy;

use super::config::EngineConfig;
use super::error::{DownloadError, DownloadErrorKind};

/// 重定向上限（承 HMCL 的 20 跳）。
pub const MAX_REDIRECTS: usize = 20;

/// 构造共享的 `reqwest::Client`（连接池、超时、重定向策略一次定好）。
pub fn build_client(cfg: &EngineConfig) -> Result<reqwest::Client, DownloadError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT_ENCODING,
        reqwest::header::HeaderValue::from_static("identity"),
    );
    reqwest::Client::builder()
        .user_agent(cfg.user_agent.clone())
        .connect_timeout(cfg.connect_timeout)
        .redirect(Policy::limited(MAX_REDIRECTS))
        .default_headers(headers)
        .build()
        .map_err(|err| {
            DownloadError::new(
                DownloadErrorKind::Network,
                format!("构造 HTTP 客户端失败：{err}"),
            )
        })
}

/// 把 `reqwest::Error` 映射成我们的错误分类。
#[must_use]
pub fn classify_reqwest(err: &reqwest::Error, url: &str) -> DownloadError {
    let kind = if err.is_timeout() {
        DownloadErrorKind::Timeout
    } else if let Some(status) = err.status() {
        return classify_status(status.as_u16(), url);
    } else {
        DownloadErrorKind::Network
    };
    DownloadError::new(kind, format!("{err}")).with_source(url)
}

/// HTTP 状态码 → 错误分类。
#[must_use]
pub fn classify_status(status: u16, url: &str) -> DownloadError {
    let kind = match status {
        404 | 410 => DownloadErrorKind::NotFound,
        // 403/401 对镜像来说基本等于「这条路不能走」，直接换源比退避更有用。
        401 | 403 => DownloadErrorKind::NotFound,
        408 => DownloadErrorKind::Timeout,
        416 => DownloadErrorKind::RangeUnsupported,
        429 => DownloadErrorKind::ServerError,
        500..=599 => DownloadErrorKind::ServerError,
        _ => DownloadErrorKind::ServerError,
    };
    DownloadError::new(kind, format!("HTTP {status}")).with_source(url)
}

/// 读一个头的字符串值。
#[must_use]
pub fn header_str(headers: &HeaderMap, name: HeaderName) -> Option<&str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// `Content-Length`。
#[must_use]
pub fn content_length(headers: &HeaderMap) -> Option<u64> {
    header_str(headers, CONTENT_LENGTH).and_then(|value| value.trim().parse().ok())
}

/// `ETag`（续传的强校验器）。
#[must_use]
pub fn etag(headers: &HeaderMap) -> Option<String> {
    header_str(headers, ETAG).map(str::to_string)
}

/// `Last-Modified`（弱校验器，ETag 缺失时的兜底）。
#[must_use]
pub fn last_modified(headers: &HeaderMap) -> Option<String> {
    header_str(headers, LAST_MODIFIED).map(str::to_string)
}

/// `Content-Type`。
#[must_use]
pub fn content_type(headers: &HeaderMap) -> Option<&str> {
    header_str(headers, CONTENT_TYPE)
}

/// 判断一个 2xx 响应是不是「伪装的档案」（镜像门户的假阳性兜底页）。
///
/// 出处：research/06 §0.2 / §5.3 第 2 条——`mirrors.huaweicloud.com` 对**任意不存在的路径**
/// 都回 `200 + 12 434 字节 Angular 门户 HTML`，只看状态码会把死源当活源。
///
/// 两条判据（都只在证据充分时才拒）：
/// 1. `Content-Type` 含 `html` → 一定是门户页，不是归档；
/// 2. 已知期望大小、且是整文件请求（`offset == 0`）、响应带 `Content-Length` 却对不上
///    → 长度对拍失败，同样不是我们要的那个文件。
///
/// 返回 `Some(原因)` 时调用方应当**换源**（归到 `NotFound` 类：不算源坏、不进冷却）。
#[must_use]
pub fn html_mirror_reason(
    headers: &HeaderMap,
    expected_size: Option<u64>,
    offset: u64,
) -> Option<&'static str> {
    if content_type(headers).is_some_and(|value| value.to_ascii_lowercase().contains("html")) {
        return Some("源返回 HTML（镜像门户兜底页，假阳性：该路径没有真实归档）");
    }
    if offset == 0 {
        if let (Some(expected), Some(actual)) = (expected_size, content_length(headers)) {
            if expected > 0 && actual != expected {
                return Some("源返回的 Content-Length 与期望大小不符（疑似门户页/截断响应）");
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE};

    use super::{html_mirror_reason, MAX_REDIRECTS};

    fn headers(content_type: Option<&str>, length: Option<u64>) -> HeaderMap {
        let mut map = HeaderMap::new();
        if let Some(value) = content_type {
            map.insert(CONTENT_TYPE, HeaderValue::from_str(value).expect("头值"));
        }
        if let Some(value) = length {
            map.insert(CONTENT_LENGTH, HeaderValue::from(value));
        }
        map
    }

    #[test]
    fn html_portal_page_is_rejected_even_with_200() {
        // 华为云实测：任意不存在的路径 → 200 + text/html + 12434 字节。
        let map = headers(Some("text/html;charset=UTF-8"), Some(12_434));
        assert_eq!(
            html_mirror_reason(&map, Some(201_328_186), 0),
            Some("源返回 HTML（镜像门户兜底页，假阳性：该路径没有真实归档）")
        );
        // 只有 Content-Type 也能判（没有期望大小时）。
        assert!(html_mirror_reason(&map, None, 0).is_some());
    }

    #[test]
    fn real_archive_matching_the_expected_size_passes() {
        let map = headers(Some("application/zip"), Some(201_328_186));
        assert_eq!(html_mirror_reason(&map, Some(201_328_186), 0), None);
        // 镜像没给 Content-Type 也照放行（有 sha256 兜底）。
        let bare = headers(None, Some(201_328_186));
        assert_eq!(html_mirror_reason(&bare, Some(201_328_186), 0), None);
    }

    #[test]
    fn mismatched_length_is_rejected_but_resumed_reads_are_left_alone() {
        let map = headers(Some("application/octet-stream"), Some(973_920));
        assert!(html_mirror_reason(&map, Some(205_421_281), 0).is_some());
        // 续传片段当然比全量小：不能按整文件口径对拍。
        assert_eq!(html_mirror_reason(&map, Some(205_421_281), 973_920), None);
        // 未知期望大小 → 不做长度判定。
        assert_eq!(html_mirror_reason(&map, None, 0), None);
    }

    #[test]
    fn redirect_cap_is_still_twenty() {
        assert_eq!(MAX_REDIRECTS, 20);
    }
}
