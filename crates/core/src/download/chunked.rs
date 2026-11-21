//! 大文件多连接分块下载：分块规划、`Content-Range` 解析、Range 能力判定与单块抓取。
//!
//! HMCL 只做「单文件单连接 + 全局并发闸门」（analyse/03 §5.5），大 jar 会抢占许可并形成
//! 头阻塞。这里对「大文件 + 支持 Range」的资源开 N 条连接并行取段（默认 N=4），
//! 每条连接写进同一个**预分配**中间文件的不同偏移；服务器不支持 Range（返回 200 而不是 206）
//! 时由引擎自动退化为单连接。

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use super::error::{DownloadError, DownloadErrorKind};
use super::http;
use super::stall::Deadlines;
use super::types::{HashAlgo, ResourceKind};
use super::verify;

/// 分块自证用的摘要算法（[`super::resume::ChunkRecord`] 里记的就是它）。
///
/// 固定 SHA-1 而不是跟随请求的校验算法：这条摘要是「这段字节是我们写下去的」的**归属证据**，
/// 不对抗攻击者；整文件的期望摘要仍按请求里的 `algo` 校验。固定算法让记录不必再存一个算法字段。
pub const CHUNK_DIGEST: HashAlgo = HashAlgo::Sha1;

/// 默认分块连接数（设计文档 Q19 定案 N=4）。
pub const DEFAULT_CONNECTIONS: usize = 4;
/// 分块连接数上限（再多收益递减且容易被镜像限流）。
pub const MAX_CONNECTIONS: usize = 16;
/// 默认开启分块的最小文件大小（8 MiB）。
pub const DEFAULT_MIN_SIZE: u64 = 8 * 1024 * 1024;

/// 分块下载配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkedConfig {
    /// 总开关（设置项）。
    pub enabled: bool,
    /// 并行连接数。
    pub connections: usize,
    /// 小于该大小不做分块。
    pub min_size: u64,
}

impl Default for ChunkedConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            connections: DEFAULT_CONNECTIONS,
            min_size: DEFAULT_MIN_SIZE,
        }
    }
}

impl ChunkedConfig {
    /// 关闭分块。
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            connections: 1,
            min_size: DEFAULT_MIN_SIZE,
        }
    }

    /// 该资源当前是否走分块。
    #[must_use]
    pub fn applies_to(&self, kind: ResourceKind, size: Option<u64>) -> bool {
        self.enabled
            && self.connections > 1
            && kind.prefers_chunked()
            && size.is_some_and(|size| size >= self.min_size)
    }

    /// 实际生效的连接数（夹在 `1..=MAX_CONNECTIONS`）。
    #[must_use]
    pub fn effective_connections(&self) -> usize {
        self.connections.clamp(1, MAX_CONNECTIONS)
    }
}

/// 闭区间字节段（HTTP Range 的语义是闭区间，这里保持一致免得 +1/-1 到处飞）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    /// 起始偏移（含）。
    pub start: u64,
    /// 结束偏移（**含**）。
    pub end: u64,
}

impl ByteRange {
    /// 构造。
    #[must_use]
    pub const fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    /// 段长度（字节）。
    #[must_use]
    pub const fn len(&self) -> u64 {
        if self.end < self.start {
            0
        } else {
            self.end - self.start + 1
        }
    }

    /// 空段。
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.end < self.start
    }

    /// `Range` 头的值。
    #[must_use]
    pub fn header_value(&self) -> String {
        format!("bytes={}-{}", self.start, self.end)
    }
}

/// 把 `total` 均分成不超过 `connections` 段（纯函数，单测重点）。
///
/// 保证：段序列连续、无重叠、并集恰为 `[0, total)`；段数 = `min(connections, total)`。
#[must_use]
pub fn plan(total: u64, connections: usize) -> Vec<ByteRange> {
    if total == 0 {
        return Vec::new();
    }
    let parts = connections.clamp(1, MAX_CONNECTIONS) as u64;
    let count = parts.min(total);
    let base = total / count;
    let remainder = total % count;

    let mut ranges = Vec::with_capacity(count as usize);
    let mut offset = 0_u64;
    for index in 0..count {
        // 余数摊到前几段，避免最后一段特别大。
        let extra = u64::from(index < remainder);
        let len = base + extra;
        if len == 0 {
            continue;
        }
        ranges.push(ByteRange::new(offset, offset + len - 1));
        offset += len;
    }
    ranges
}

/// `Content-Range: bytes 0-99/1000` 的解析结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentRange {
    /// 起始。
    pub start: u64,
    /// 结束（含）。
    pub end: u64,
    /// 总长度（`*` 时为 `None`）。
    pub total: Option<u64>,
}

/// 解析 `Content-Range`（畸形一律 `None`，调用方退化处理）。
#[must_use]
pub fn parse_content_range(value: &str) -> Option<ContentRange> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end: u64 = end.trim().parse().ok()?;
    if end < start {
        return None;
    }
    let total = match total.trim() {
        "*" => None,
        other => Some(other.parse().ok()?),
    };
    Some(ContentRange { start, end, total })
}

/// 服务器对 Range 的支持度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeSupport {
    /// 支持（`Accept-Ranges: bytes` 或正常返回 206）。
    Supported,
    /// 明确不支持。
    Unsupported,
    /// 未知（既没头也没探测）。
    Unknown,
}

/// 判定 Range 支持度。
#[must_use]
pub fn range_support(accept_ranges: Option<&str>, status: u16) -> RangeSupport {
    if status == 206 {
        return RangeSupport::Supported;
    }
    match accept_ranges {
        Some(value) if value.to_ascii_lowercase().contains("bytes") => RangeSupport::Supported,
        Some(value) if value.to_ascii_lowercase().contains("none") => RangeSupport::Unsupported,
        Some(_) => RangeSupport::Unsupported,
        None => RangeSupport::Unknown,
    }
}

/// 206 响应的 `Content-Range` 是否与请求段一致。
///
/// * 缺头 → 宽松放过（个别服务器省略该头，后面的长度校验仍能兜底）；
/// * 有头 → 起止必须严格等于请求段，畸形头一律判不符。
///
/// 这条防御与单连接路径（`transfer.rs` 里比对 `content_range.start != offset`）对齐：
/// 只在「长度恰好」时坏文件才会溜过长度校验，起止比对把这条路堵死。
#[must_use]
pub fn content_range_matches(requested: ByteRange, value: Option<&str>) -> bool {
    let Some(value) = value else {
        return true;
    };
    parse_content_range(value)
        .is_some_and(|parsed| parsed.start == requested.start && parsed.end == requested.end)
}

/// 一次成功分块抓取的产物：落盘的字节数 + **它的身份信息**。
///
/// 为什么把这些一起带回来：分块路径的续传元数据（`ResumeMeta::chunks`）要记「这一段是什么」
/// ——段区间、这段字节的摘要（[`Self::sha1`]）、服务器的校验器与总长。它们只有响应头/读流这一处
/// 拿得到，带回来由编排层决定怎么落盘，比在这里再读一遍盘便宜。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkFetch {
    /// 本段区间。
    pub range: ByteRange,
    /// 实际落盘字节数（校验通过时恒等于 `range.len()`）。
    pub written: u64,
    /// 本段字节的摘要（边写边算，算法见 [`CHUNK_DIGEST`]）。
    pub sha1: String,
    /// 响应 `Content-Range` 里的总长（服务器没给就是 `None`）。
    pub total: Option<u64>,
    /// 响应 `ETag`。
    pub etag: Option<String>,
    /// 响应 `Last-Modified`。
    pub last_modified: Option<String>,
}

/// 抓一个段，写到预分配文件的 `range.start` 偏移处，并把字节数累加进 `progress`。
///
/// 服务器若对带 `Range` 的请求返回 200（无视 Range），立刻返回 `RangeUnsupported`
/// —— 引擎据此退化单连接，而不是把整份文件重复写 N 遍。
///
/// `if_range` 是续传时的校验器（ETag 优先 / Last-Modified 兜底）：服务器照办回 206，
/// 内容变过回 200 —— 于是「远端换了内容」由协议层回答，不必等整文件摘要才发现。
///
/// 两处 `await` 都有界（M289）：等响应头用 `http::send_bounded`，等正文用
/// `http::read_step`，上界来自 [`Deadlines`]。失速时错误里带阶段与本段口径的字节数。
pub async fn fetch_chunk(
    client: &reqwest::Client,
    url: &str,
    range: ByteRange,
    part: &Path,
    deadlines: Deadlines,
    progress: &AtomicU64,
    if_range: Option<&str>,
) -> Result<ChunkFetch, DownloadError> {
    use tokio::io::{AsyncSeekExt, AsyncWriteExt};

    let mut request = client
        .get(url)
        .header(reqwest::header::RANGE, range.header_value());
    if let Some(value) = if_range {
        request = request.header(reqwest::header::IF_RANGE, value);
    }
    let response = http::send_bounded(request, url, deadlines, Some(range.len())).await?;

    let status = response.status().as_u16();
    if status == 200 {
        return Err(DownloadError::new(
            DownloadErrorKind::RangeUnsupported,
            format!("服务器忽略 Range 请求（{url}）"),
        ));
    }
    if status != 206 {
        return Err(http::classify_status(status, url));
    }
    // 服务器可能「回 206 但段不对」（CDN 缓存错位、Range 实现有 bug）。
    // 判不符时按 RangeUnsupported 处理：引擎会丢弃中间文件、退化单连接重下，
    // 这比在同一条坏路径上重试更划算。
    let content_range = http::header_str(response.headers(), reqwest::header::CONTENT_RANGE);
    if !content_range_matches(range, content_range) {
        return Err(DownloadError::new(
            DownloadErrorKind::RangeUnsupported,
            format!(
                "Content-Range 与请求段不符（{url}）：请求 {}，响应 {}",
                range.header_value(),
                content_range.unwrap_or("<缺失>")
            ),
        ));
    }

    // 顺手取走总长：编排层要拿它比对「这个源给的还是不是同一个文件」。
    let total = content_range
        .and_then(parse_content_range)
        .and_then(|parsed| parsed.total);
    let etag = http::etag(response.headers());
    let last_modified = http::last_modified(response.headers());

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(part)
        .await
        .map_err(|err| io_error(&err, "打开分块中间文件失败"))?;
    file.seek(std::io::SeekFrom::Start(range.start))
        .await
        .map_err(|err| io_error(&err, "定位分块偏移失败"))?;

    let mut stream = response.bytes_stream();
    let mut hasher = verify::Hasher::new(CHUNK_DIGEST);
    let mut written = 0_u64;
    loop {
        let next = http::read_step(
            futures_util::StreamExt::next(&mut stream),
            deadlines.read_idle,
            written,
            Some(range.len()),
            url,
        )
        .await?;
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|err| http::classify_reqwest(&err, url))?;
        file.write_all(&chunk)
            .await
            .map_err(|err| io_error(&err, "写入分块失败"))?;
        hasher.update(&chunk);
        let len = chunk.len() as u64;
        written += len;
        progress.fetch_add(len, Ordering::Relaxed);
    }
    file.flush()
        .await
        .map_err(|err| io_error(&err, "刷新分块失败"))?;

    if written != range.len() {
        return Err(DownloadError::new(
            DownloadErrorKind::Network,
            format!("分块长度不符：期望 {} 实得 {written}（{url}）", range.len()),
        ));
    }
    Ok(ChunkFetch {
        range,
        written,
        sha1: hasher.finish(),
        total,
        etag,
        last_modified,
    })
}

fn io_error(err: &std::io::Error, context: &str) -> DownloadError {
    DownloadError::new(DownloadErrorKind::Io, format!("{context}: {err}"))
}
