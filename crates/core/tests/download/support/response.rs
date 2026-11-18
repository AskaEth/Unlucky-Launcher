//! 夹具的一次应答描述（状态码 / 正文 / Range 支持 / 校验器 / 各种坏形态）。
//!
//! 从 `support/server.rs` 拆出只为守 400 行硬线（M293 r1：给夹具加上 `If-Range` 观测之后）。

use std::time::Duration;

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
    /// `Last-Modified`（`ETag` 缺失时的弱校验器；HTTP-date 字样原样发出）。
    pub last_modified: Option<String>,
    /// 返回 206 时故意把 `Content-Range` 写成从 0 开始（模拟段错位的坏服务器）。
    pub lie_content_range: bool,
    /// 读完请求头后**一个字节都不写**（服务端接了连接但永不回话）。
    ///
    /// 这是「无界 `await`」唯一能表达成用例的形态：读方不设上界就永远不返回。
    pub silent: bool,
    /// 写出这么多字节正文后就**停住不再写**（中途卡住），连接保持打开。
    pub stall_at: Option<usize>,
    /// 正文实际只发这么多字节（头部仍声称全量 = 短包）。
    pub send_len: Option<usize>,
    /// 正文之后**多**发这么多字节，且故意不发 `Content-Length`（超长包：HTTP/1.1 靠关连接分帧）。
    pub trailing_extra: Option<usize>,
    /// 正文按 `piece` 字节一片、片间睡 `gap` 滴流（慢但持续，用来证明超时不会误杀它）。
    pub drip: Option<(usize, Duration)>,
}

impl MockResponse {
    /// 200 + 完整内容（支持 Range）。
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: body.into(),
            range_support: true,
            etag: Some("\"v1\"".to_string()),
            last_modified: None,
            lie_content_range: false,
            silent: false,
            stall_at: None,
            send_len: None,
            trailing_extra: None,
            drip: None,
        }
    }

    /// 固定状态码 + 空体。
    pub fn status(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
            range_support: false,
            etag: None,
            last_modified: None,
            lie_content_range: false,
            silent: false,
            stall_at: None,
            send_len: None,
            trailing_extra: None,
            drip: None,
        }
    }

    /// 200 但**永不回话**：读完请求头就挂住，一个字都不写。
    pub fn silent() -> Self {
        Self {
            silent: true,
            ..Self::ok(Vec::new())
        }
    }

    /// 写完 `bytes` 字节正文后停住（中途卡住，连接不断）。
    pub fn stalling_after(mut self, bytes: usize) -> Self {
        self.stall_at = Some(bytes);
        self
    }

    /// 头部声称全量、正文只发 `sent` 字节（短包，客户端读不满）。
    pub fn short_body(mut self, sent: usize) -> Self {
        self.send_len = Some(sent);
        self
    }

    /// 正文之后多发 `extra` 字节且不发 `Content-Length`（超长包）。
    pub fn oversize_body(mut self, extra: usize) -> Self {
        self.trailing_extra = Some(extra);
        self
    }

    /// 正文按 `piece` 字节一片、片间 `gap` 滴流（慢但持续）。
    pub fn dripping(mut self, piece: usize, gap: Duration) -> Self {
        self.drip = Some((piece, gap));
        self
    }

    /// 声明不支持 Range（模拟部分镜像）。
    pub fn without_range(mut self) -> Self {
        self.range_support = false;
        self
    }

    /// 指定 `ETag`（换源用例要能造出「两台服务器各自不同的校验器」）。
    pub fn with_etag(mut self, etag: impl Into<String>) -> Self {
        self.etag = Some(etag.into());
        self
    }

    /// 完全不发 `ETag`（模拟「自己没有强校验器」的源）。
    pub fn without_etag(mut self) -> Self {
        self.etag = None;
        self
    }

    /// 指定 `Last-Modified`（`ETag` 缺失时 `If-Range` 的兜底取值）。
    pub fn with_last_modified(mut self, value: impl Into<String>) -> Self {
        self.last_modified = Some(value.into());
        self
    }

    /// 206 但 `Content-Range` 谎报（模拟段错位的坏 CDN）。
    pub fn with_wrong_content_range(mut self) -> Self {
        self.lie_content_range = true;
        self
    }
}
