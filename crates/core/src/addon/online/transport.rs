//! 唯一的 HTTP 出口 + 错误分类。
//!
//! # 为什么是 trait
//!
//! 三条理由，每条都对应一个真实麻烦：
//!
//! 1. **测试不能联网**：本模块的解析/端点/补全全部要用录制 JSON 跑（CI 没有网，也不该有）；
//! 2. **换源不改业务**：将来加第二个内容源时，只要它也能 GET 出 JSON，就能复用这套
//!    `Transport`（现在的实现按「一个 URL → 一段文本」抽象，不假设站点）；
//! 3. **错误必须可分辨**：「没联网」和「站点说没有这个项目」是两件事（前者该提示检查网络，
//!    后者该提示换关键词），字符串错误串会让上层只能靠 `contains` 猜。
//!
//! # 生产实现只有一条路：`core::download` 的引擎
//!
//! [`EngineTransport`] 调 [`DownloadEngine::download_bytes`]，于是信号量限流、源健康度、
//! 指数退避、错误分类全在引擎里。本模块**不**新建 `reqwest::Client`、**不**写重试循环
//! （与 `addon::import::fetch` 的纪律同源）。镜像策略同理：`mirrors.json` 里没有
//! `api.modrinth.com` 的前缀规则 ⇒ 元数据请求永远是直连，只有真正的文件下载才会被镜像
//! 规则命中（国内 gh-proxy 那条也只看 GitHub host）。

use std::future::Future;
use std::pin::Pin;

use crate::download::{
    DownloadEngine, DownloadError, DownloadErrorKind, DownloadRequest, ResourceKind,
};

/// trait 方法返回的装箱 future（core 不引 `async-trait`，手写与 `import::fetch` 同一个范式）。
pub type OnlineFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 「拿不到」的成因（**分级判据只在这里**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnlineErrorKind {
    /// 没联网 / 连不上 / 所有候选源都失败 —— 上层要明确说「未联网」。
    NotConnected,
    /// 站点说没有（404 / 项目被删）。
    NotFound,
    /// 站点出错（5xx / 429 退避后仍失败）。
    Server,
    /// 响应不是期望形状（JSON 坏 / 字段类型不对）。
    Malformed,
    /// 本地 IO（写缓存失败等）。
    Io,
    /// 内容与声明的摘要不符（下载到的字节坏了 / 镜像给的其实是别的东西）。
    Integrity,
    /// 这条路走不通（URL 非法、摘要格式不对、没有可用下载地址）。
    Unsupported,
}

impl OnlineErrorKind {
    /// 稳定字符串 id（日志 / DTO 用，**不要改名**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotConnected => "online_not_connected",
            Self::NotFound => "online_not_found",
            Self::Server => "online_server",
            Self::Malformed => "online_malformed",
            Self::Io => "online_io",
            Self::Integrity => "online_integrity",
            Self::Unsupported => "online_unsupported",
        }
    }

    /// 是不是「网络这条路没通」（决定上层画「未联网」还是「查无此项」）。
    #[must_use]
    pub const fn is_offline(self) -> bool {
        matches!(self, Self::NotConnected)
    }
}

/// 一次取数失败。
#[derive(Debug, Clone)]
pub struct OnlineError {
    kind: OnlineErrorKind,
    message: String,
}

impl OnlineError {
    /// 构造。
    #[must_use]
    pub fn new(kind: OnlineErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// 没联网。
    #[must_use]
    pub fn offline(message: impl Into<String>) -> Self {
        Self::new(OnlineErrorKind::NotConnected, message)
    }

    /// 形状不对 / 自身不自洽。
    #[must_use]
    pub fn malformed(message: impl Into<String>) -> Self {
        Self::new(OnlineErrorKind::Malformed, message)
    }

    /// 内容与声明不符。
    #[must_use]
    pub fn integrity(message: impl Into<String>) -> Self {
        Self::new(OnlineErrorKind::Integrity, message)
    }

    /// 这条路走不通。
    #[must_use]
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(OnlineErrorKind::Unsupported, message)
    }

    /// 大类。
    #[must_use]
    pub const fn kind(&self) -> OnlineErrorKind {
        self.kind
    }

    /// 人读说明。
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 没联网吗（上层据此决定降级文案）。
    #[must_use]
    pub const fn is_offline(&self) -> bool {
        self.kind.is_offline()
    }
}

impl std::fmt::Display for OnlineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}：{}", self.kind.as_str(), self.message)
    }
}

impl std::error::Error for OnlineError {}

/// `DownloadEngine` 的错误 → 本模块的错误分类。
///
/// # 为什么读 `effective_kind()` 而不是 `kind`
///
/// 引擎的候选源循环结束时只留下一条 [`DownloadErrorKind::AllSourcesFailed`]，404 / 429 / 5xx /
/// 超时这些**叶子**成因会被折叠掉（`crates/core/src/download/bytes.rs` 的收尾）。
/// 如果直接按 `kind` 判定，`NotFound` / `Server` / `Integrity` 三档在本模块的生产路径上
/// **永远不可达**：项目被删、id 拼错、被限速都会画成「未联网」（r1 P1-1 后果 A）。
///
/// 引擎现在会把汇总出来的叶子 kind 挂在 `DownloadError::leaf` 上，这里按叶子映射；
/// 没有叶子（单源直失败 / 手工构造）就按自身 kind。**不从 `suppressed` 文本里猜状态码。**
///
/// # 分类口径
///
/// | 引擎侧 | 本模块 | 上层该怎么说 |
/// |--------|--------|--------------|
/// | `NotFound`（404/410/403，全源都是） | [`OnlineErrorKind::NotFound`] | 「查无此项 / 换个关键词」 |
/// | `ServerError`（5xx / 429） | [`OnlineErrorKind::Server`] | 「站点那边出错了」 |
/// | `Network` / `Timeout` | [`OnlineErrorKind::NotConnected`] | 「未联网，检查网络」 |
/// | `AllSourcesFailed`（没有叶子） | [`OnlineErrorKind::NotConnected`] | 同上（保守） |
/// | `Integrity` | [`OnlineErrorKind::Integrity`] | 「内容不对」 |
impl From<DownloadError> for OnlineError {
    fn from(error: DownloadError) -> Self {
        let kind = match error.effective_kind() {
            DownloadErrorKind::NotFound => OnlineErrorKind::NotFound,
            DownloadErrorKind::ServerError => OnlineErrorKind::Server,
            DownloadErrorKind::Timeout | DownloadErrorKind::Network => {
                OnlineErrorKind::NotConnected
            }
            DownloadErrorKind::Io => OnlineErrorKind::Io,
            DownloadErrorKind::Integrity => OnlineErrorKind::Integrity,
            DownloadErrorKind::RangeUnsupported => OnlineErrorKind::Malformed,
            DownloadErrorKind::InvalidRequest => OnlineErrorKind::Unsupported,
            // 没有叶子 ⇒ 分不清「没网」与「站点挂了」，按可重试的「没联网」保守处理：
            // 上层至少会提示「检查网络后重试」，而不是宣称「没有匹配的内容」。
            DownloadErrorKind::AllSourcesFailed => OnlineErrorKind::NotConnected,
        };
        Self::new(kind, error.message)
    }
}

/// 一个 URL → 一段文本（JSON 原文）。
///
/// 实现方只负责「取回来」：状态码分类、退避、限流都在实现里（生产实现在引擎里）。
pub trait Transport: Send + Sync {
    /// GET 一段文本。
    fn get_text<'a>(&'a self, url: &'a str) -> OnlineFuture<'a, Result<String, OnlineError>>;
}

/// 生产实现：把 GET 交给 `core::download` 的引擎（内存下载路径）。
#[derive(Debug, Clone, Copy)]
pub struct EngineTransport<'a> {
    engine: &'a DownloadEngine,
}

impl<'a> EngineTransport<'a> {
    /// 绑定引擎。
    #[must_use]
    pub const fn new(engine: &'a DownloadEngine) -> Self {
        Self { engine }
    }
}

impl Transport for EngineTransport<'_> {
    fn get_text<'a>(&'a self, url: &'a str) -> OnlineFuture<'a, Result<String, OnlineError>> {
        Box::pin(async move {
            // `download_bytes` 不落盘（`dest` 只是请求形状的一部分），所以这里给一个中性路径；
            // 内容寻址缓存按 URL 规划，不看它。
            let request = DownloadRequest::new(
                ResourceKind::Other,
                url,
                std::env::temp_dir().join("ull-online-metadata.json"),
            );
            let bytes = self
                .engine
                .download_bytes(request)
                .await
                .map_err(OnlineError::from)?;
            String::from_utf8(bytes)
                .map_err(|error| OnlineError::malformed(format!("{url} 的响应不是 UTF-8：{error}")))
        })
    }
}
