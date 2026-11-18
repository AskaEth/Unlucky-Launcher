//! 唯一的 HTTP 出口：**带请求头**的 GET，回「状态码 + Retry-After + 正文」。
//!
//! # 为什么不复用 `online::Transport`
//!
//! 那个 trait 的签名是「URL → 文本」，两个缺口正好都是 CurseForge 的硬需求：
//!
//! 1. **要发请求头**（`x-api-key`）——它的方法没有位置放；
//! 2. **要看见状态码与 `Retry-After`**——它把非 2xx 折叠成 `OnlineError`，而本模块必须把
//!    401 / 403 / 429 分成三档（[`super::CfErrorKind`]），并且 429 要读站点的等待秒数。
//!
//! 所以这里是**第二个**出口，而不是给那个 trait 加参数：给 `online` 加请求头会让 Modrinth 那条
//! 已经稳定的路径跟着改（并且它的错误分类里没有「限流」这一档）。
//!
//! # 生产实现只发一次请求
//!
//! [`EngineTransport`] 不做重试、不做退避、不判断「该不该再试」——那些在
//! [`super::CurseForge`] 里（[`super::RetryPolicy`]），因为「重试几次」是策略、要能被关掉，
//! 而「怎么发这一条请求」是机制。诊断面上这也更清楚：一次失败的搜索只会在日志里留下
//! **一条** curl 等价的请求，而不是三条。
//!
//! 用的仍然是引擎的 `reqwest::Client`（[`DownloadEngine::client`]）：同一条连接池、同一份代理与
//! 超时配置、同一个 `User-Agent`，与文件下载和 Modrinth 的元数据请求共享——本模块**不新建**
//! client，也不改引擎配置。
//!
//! # 密钥纪律
//!
//! 请求头由调用方给进来（一个 `(名前, 值)` 列表），本层只把它们塞进请求、**从不**写进任何错误串
//! 或日志：错误串里只有 URL（CF 的 URL 不含 Key）与状态码。

use std::future::Future;
use std::pin::Pin;

use crate::download::DownloadEngine;

use super::CfError;

/// trait 方法返回的装箱 future（与 `online::Transport` / `import::fetch` 同一个范式）。
pub type CfFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 一次 HTTP 回答：状态码 + `Retry-After`（秒）+ 正文。
///
/// **不**把「失败」编码在这一层：4xx/5xx 也是「取回来了」，判定归 [`super::CfError::from_status`]。
/// 只有「根本没取回来」（连接失败 / 超时 / 正文不是 UTF-8）才是 `Err`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfResponse {
    /// HTTP 状态码。
    pub status: u16,
    /// `Retry-After` 头的秒数（没有 / 不是数字 ⇒ `None`；站点也可能给 HTTP-date，本模块不解析）。
    pub retry_after: Option<u64>,
    /// 响应正文。
    pub body: String,
}

impl CfResponse {
    /// 构造。
    #[must_use]
    pub fn new(status: u16, retry_after: Option<u64>, body: impl Into<String>) -> Self {
        Self {
            status,
            retry_after,
            body: body.into(),
        }
    }

    /// 2xx 吗。
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
}

/// 一个 URL + 一组请求头 → 一次 HTTP 回答。
///
/// 实现方**只负责取回来**：不校验形状、不重试、不判断该不该再试。
pub trait CfTransport: Send + Sync {
    /// GET 一次。
    fn get<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
    ) -> CfFuture<'a, Result<CfResponse, CfError>>;
}

/// 生产实现：用下载引擎的 `reqwest::Client` 直发（**只发一次**）。
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

impl CfTransport for EngineTransport<'_> {
    fn get<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
    ) -> CfFuture<'a, Result<CfResponse, CfError>> {
        Box::pin(async move {
            // UA 取自引擎配置（**不在这里拼**）：定义只有 core 那一份
            // （`download::http::USER_AGENT`），CF 与 Modrinth、与本模块之外的文件下载必须发同一个串。
            let mut builder = self
                .engine
                .client()
                .get(url)
                .header("User-Agent", self.engine.config().user_agent.as_str());
            for (name, value) in headers {
                builder = builder.header(name.as_str(), value.as_str());
            }
            let response = builder
                .send()
                .await
                // reqwest 的错误只带 URL 与传输层原因，**不含请求头**。
                .map_err(|error| CfError::network(format!("请求 {url} 失败：{error}")))?;
            let status = response.status().as_u16();
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok());
            let body = response
                .text()
                .await
                .map_err(|error| CfError::network(format!("{url} 的响应读不出来：{error}")))?;
            Ok(CfResponse::new(status, retry_after, body))
        })
    }
}
