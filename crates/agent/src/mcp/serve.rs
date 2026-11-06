//! 两种传输：stdio（llama.cpp 内置 MCP client 用）与 Streamable HTTP（主力）。
//!
//! 传输定论（02 §0）：**外部 MCP 主力是 127.0.0.1 上的 Streamable HTTP**，
//! 因为 stdio 形态要被外部 agent 拉起 = 同一份数据目录出现第二个实例，
//! 迟早撞上单写者锁（M18 的会话 jsonl、M12 的 launcher.json 都是单写者）。
//! stdio 保留给**已经常驻的**本地模型服务（llama.cpp server 的 MCP client 由它自己拉子进程）。
//!
//! 令牌：`Authorization: Bearer <agent.mcp.token>`。设置里为空 = 不校验（只允许回环监听，
//! `McpConfig::ensure_loopback` 已经拦掉对外监听）。research/02 §4.4 的原话是
//! 「至少要有 headers 里的一枚随机 token」——我们做成可配、默认留空但样例里带。

use std::net::SocketAddr;

use axum::extract::Request;
use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::ServiceExt;
use tokio::net::TcpListener;

use crate::mcp::server::RegistryServer;
use crate::mcp::{McpConfig, McpServeError};

/// stdio 形态：读 stdin 写 stdout，直到对端关闭。
///
/// 注意：**stdout 只能有 JSON-RPC**。任何日志都要走 stderr（否则客户端解析失败）。
///
/// # Errors
/// 传输中断或初始化失败时返回 [`McpServeError`]。
pub async fn serve_stdio(server: RegistryServer) -> Result<(), McpServeError> {
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|error| McpServeError::Io(format!("stdio 初始化失败：{error}")))?;
    service
        .waiting()
        .await
        .map_err(|error| McpServeError::Io(format!("stdio 传输中断：{error}")))?;
    Ok(())
}

/// 绑定 Streamable HTTP 监听（**只绑回环**），返回实际地址。
///
/// 拆出来是为了让调用方在进入 `axum::serve` 之前就能打印「监听在哪」——
/// 端口被占用时也能拿到一条可操作的错误（tower 的提醒：撞端口要给出换端口的指引）。
///
/// # Errors
/// 非回环地址、端口被占用、或绑定失败时返回 [`McpServeError`]。
pub async fn bind(cfg: &McpConfig) -> Result<(SocketAddr, TcpListener), McpServeError> {
    cfg.ensure_loopback()?;
    let addr = cfg.socket_addr();
    let listener = TcpListener::bind(&addr).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::AddrInUse {
            McpServeError::PortInUse { addr: addr.clone() }
        } else {
            McpServeError::Io(format!("绑定 {addr} 失败：{error}"))
        }
    })?;
    let local = listener
        .local_addr()
        .map_err(|error| McpServeError::Io(error.to_string()))?;
    Ok((local, listener))
}

/// Streamable HTTP 形态：`POST /mcp` 一条 JSON-RPC 请求 / 一条响应。
///
/// `json_response = true`：简单请求直接回 `application/json`（脚本与裸 JSON-RPC
/// 客户端最省事；需要服务端推送时 rmcp 会自动回落到 SSE）。
///
/// # Errors
/// 绑定失败（见 [`bind`]）或服务运行期致命错误时返回 [`McpServeError`]。
pub async fn serve_http(server: RegistryServer, cfg: McpConfig) -> Result<(), McpServeError> {
    let (local, listener) = bind(&cfg).await?;
    serve_http_on(server, cfg, local, listener).await
}

/// 在已绑定的监听上跑（测试用临时端口，避免抢 47823）。
///
/// # Errors
/// 服务运行期致命错误时返回 [`McpServeError`]。
pub async fn serve_http_on(
    server: RegistryServer,
    cfg: McpConfig,
    local: SocketAddr,
    listener: TcpListener,
) -> Result<(), McpServeError> {
    let http_config = StreamableHttpServerConfig::default()
        .with_json_response(true)
        .with_legacy_session_mode(false);
    let factory_server = server.clone();
    let service: StreamableHttpService<RegistryServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(factory_server.clone()),
            std::sync::Arc::new(LocalSessionManager::default()),
            http_config,
        );

    let token = cfg.token.clone();
    let app = axum::Router::new()
        .nest_service("/mcp", service)
        .layer(axum::middleware::from_fn(
            move |request: Request, next: Next| {
                let token = token.clone();
                async move { authorize(token, request, next).await }
            },
        ));

    eprintln!(
        "[unlucky-agent] MCP server 监听 http://{local}/mcp（{} 个工具；令牌：{}）",
        server.tool_count(),
        if cfg.token.is_some() {
            "已启用"
        } else {
            "未设置（仅回环监听）"
        }
    );

    axum::serve(listener, app)
        .await
        .map_err(|error| McpServeError::Io(error.to_string()))
}

/// Bearer 令牌闸门：未配置 = 放行；配置了则严格比对。
async fn authorize(token: Option<String>, request: Request, next: Next) -> Response {
    let Some(expected) = token else {
        return next.run(request).await;
    };
    let presented = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if presented == format!("Bearer {expected}") {
        return next.run(request).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        "缺少或错误的 Authorization: Bearer <agent.mcp.token>",
    )
        .into_response()
}
