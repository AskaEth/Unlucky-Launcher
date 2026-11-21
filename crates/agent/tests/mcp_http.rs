//! MCP over Streamable HTTP 的端到端测试：真起监听、打裸 JSON-RPC，断言工具返回与审计。
//!
//! 为什么不用 MCP 客户端 crate：客户端依赖会再拖一棵 reqwest 树（且 3.8 的客户端 feature
//! 与我们的服务端 feature 不是一组）。裸 HTTP 反而更贴近「用户用 Claude Code 连上来」
//! 的实际形状——握手、会话头、`tools/list`、`tools/call` 一个都不省。
//!
//! 覆盖的安全语义（02 §3.1 / §4b）：
//! - 只读模式下 `set_config` 根本不在工具表里，硬调 → tool-level 错误 + `unknown_tool` 审计；
//! - 普通键（`download.concurrency`，Low）直接写入，审计 `outcome=ok`；
//! - 敏感键（`java.selected_path`，High）在**没有弹窗**的外部场景被拒，文案引导去设置页/YOLO；
//! - 打开 YOLO 后同一个写入成功，审计 `yolo=true`。

use std::net::SocketAddr;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use unlucky_agent::audit::{AuditOutcome, AuditRecord, JsonlAudit};
use unlucky_agent::brain::exec::ToolExecutor;
use unlucky_agent::clock::{FixedClock, SharedClock};
use unlucky_agent::confirm::{ConfirmPolicy, DenyGuidance, NoUiGate};
use unlucky_agent::event::NoopSink;
use unlucky_agent::mcp::{bind, serve_http_on, McpConfig, RegistryServer};
use unlucky_agent::testing::mock_ports;
use unlucky_agent::tools::build_v1_registry;

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"ull-test","version":"0.1"}}}"#;

struct Harness {
    addr: SocketAddr,
    audit_dir: std::path::PathBuf,
}

fn audit_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ull-mcp-http-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn server(read_only: bool, yolo: bool, audit: Arc<JsonlAudit>) -> RegistryServer {
    let (ports, _settings) = mock_ports();
    let registry = Arc::new(build_v1_registry(&ports).expect("注册表应装配成功"));
    let exposed = if read_only {
        Arc::new(registry.read_only_subset())
    } else {
        Arc::clone(&registry)
    };
    let clock: SharedClock = Arc::new(FixedClock::new(1_793_304_753_123));
    let executor = Arc::new(
        ToolExecutor::new(
            exposed,
            audit,
            Arc::new(NoUiGate),
            ConfirmPolicy { yolo },
            clock,
            Arc::new(NoopSink),
        )
        .with_guidance(DenyGuidance::External),
    );
    RegistryServer::new(registry, executor, "mcp-http", read_only)
}

async fn spawn(tag: &str, read_only: bool, yolo: bool) -> Harness {
    let dir = audit_dir(tag);
    let clock: SharedClock = Arc::new(FixedClock::new(1_793_304_753_123));
    let audit = Arc::new(JsonlAudit::new(&dir, clock));
    let server = server(read_only, yolo, audit);

    let cfg = McpConfig {
        port: 0,
        ..McpConfig::default()
    };
    let (addr, listener) = bind(&cfg).await.expect("应能绑定临时端口");
    tokio::spawn(async move {
        let _ = serve_http_on(server, cfg, addr, listener).await;
    });
    Harness {
        addr,
        audit_dir: dir,
    }
}

/// 发一条 HTTP POST（`Connection: close` 让读取变成 read_to_end）。
async fn post(
    addr: SocketAddr,
    body: &str,
    session: Option<&str>,
    token: Option<&str>,
) -> (u16, Option<String>, String) {
    let mut stream = TcpStream::connect(addr).await.expect("连接失败");
    let mut head = format!(
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nConnection: close\r\n\
         Content-Length: {}\r\n",
        body.len()
    );
    if let Some(session) = session {
        head.push_str(&format!("Mcp-Session-Id: {session}\r\n"));
    }
    if let Some(token) = token {
        head.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    head.push_str("\r\n");
    stream
        .write_all(format!("{head}{body}").as_bytes())
        .await
        .expect("写请求失败");
    stream.flush().await.expect("刷新失败");

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("读响应失败");
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    let session = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("mcp-session-id")
            .then(|| value.trim().to_string())
    });
    (status, session, body.to_string())
}

/// 响应体可能是裸 JSON，也可能是 SSE（`data: {...}`）；两种都解出第一条 JSON-RPC 响应。
fn parse_rpc(body: &str) -> Value {
    if let Ok(value) = serde_json::from_str::<Value>(body.trim()) {
        return value;
    }
    for line in body.lines() {
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        if let Ok(value) = serde_json::from_str::<Value>(data.trim()) {
            return value;
        }
    }
    panic!("响应体既不是 JSON 也不是 SSE：{body}");
}

async fn initialize(addr: SocketAddr) -> Option<String> {
    let (status, session, body) = post(addr, INITIALIZE, None, None).await;
    assert_eq!(status, 200, "initialize 应成功：{body}");
    let response = parse_rpc(&body);
    assert_eq!(
        response["result"]["serverInfo"]["name"],
        json!("unlucky-launcher")
    );
    // 按 MCP 规范补一条 initialized 通知（无 id、无响应体）。
    let _ = post(
        addr,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        session.as_deref(),
        None,
    )
    .await;
    session
}

async fn rpc(
    addr: SocketAddr,
    session: Option<&str>,
    id: u32,
    method: &str,
    params: Value,
) -> Value {
    let body =
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
    let (status, _session, text) = post(addr, &body, session, None).await;
    assert_eq!(status, 200, "{method} 应成功：{text}");
    parse_rpc(&text)
}

fn audit_records(dir: &std::path::Path) -> Vec<AuditRecord> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
            for line in text.lines() {
                if let Ok(record) = serde_json::from_str::<AuditRecord>(line) {
                    out.push(record);
                }
            }
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn readonly_server_lists_and_serves_only_read_tools() {
    let harness = spawn("readonly", true, false).await;
    let session = initialize(harness.addr).await;

    let list = rpc(harness.addr, session.as_deref(), 2, "tools/list", json!({})).await;
    let tools = list["result"]["tools"].as_array().expect("应有 tools 数组");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert_eq!(names.len(), 9, "{names:?}");
    assert!(!names.contains(&"set_config"), "只读模式不得暴露写工具");
    assert!(names.contains(&"get_config"));

    // 真调一个只读工具：返回真实配置概要（mock 端口）。
    let call = rpc(
        harness.addr,
        session.as_deref(),
        3,
        "tools/call",
        json!({ "name": "get_config", "arguments": {} }),
    )
    .await;
    assert_ne!(call["result"]["isError"], json!(true), "{call}");
    let text = call["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(text.contains("download.concurrency"), "{text}");

    // 硬调写工具：tool-level 错误（客户端能看见文案）+ unknown_tool 审计。
    let denied = rpc(
        harness.addr,
        session.as_deref(),
        4,
        "tools/call",
        json!({ "name": "set_config", "arguments": { "path": "download.concurrency", "value": 16 } }),
    )
    .await;
    assert_eq!(denied["result"]["isError"], json!(true), "{denied}");

    let records = audit_records(&harness.audit_dir);
    assert!(records.iter().any(|r| r.tool == "get_config"
        && r.outcome == AuditOutcome::Ok
        && r.agent_session_id == "mcp-http"));
    assert!(records
        .iter()
        .any(|r| r.tool == "set_config" && r.outcome == AuditOutcome::UnknownTool));
    let _ = std::fs::remove_dir_all(&harness.audit_dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn write_server_enforces_sensitive_key_confirmation() {
    let harness = spawn("write", false, false).await;
    let session = initialize(harness.addr).await;

    // 普通键：直接写。
    let ok = rpc(
        harness.addr,
        session.as_deref(),
        2,
        "tools/call",
        json!({ "name": "set_config", "arguments": { "path": "download.concurrency", "value": 16 } }),
    )
    .await;
    assert_ne!(ok["result"]["isError"], json!(true), "{ok}");

    // 敏感键：外部客户端没有弹窗 → 拒绝，文案要能指导用户下一步。
    let denied = rpc(
        harness.addr,
        session.as_deref(),
        3,
        "tools/call",
        json!({ "name": "set_config", "arguments": { "path": "java.selected_path", "value": "D:/jdk17/bin/java.exe" } }),
    )
    .await;
    assert_eq!(denied["result"]["isError"], json!(true), "{denied}");
    let text = denied["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(
        text.contains("YOLO"),
        "拒绝文案应引导到 YOLO/设置页：{text}"
    );
    assert!(text.contains("设置页"), "{text}");

    let records = audit_records(&harness.audit_dir);
    assert!(records
        .iter()
        .any(|r| r.tool == "set_config" && r.outcome == AuditOutcome::AwaitingConfirmation));
    assert!(records
        .iter()
        .any(|r| r.tool == "set_config" && r.outcome == AuditOutcome::Denied && !r.yolo));
    let _ = std::fs::remove_dir_all(&harness.audit_dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn yolo_lets_the_same_write_through_and_audits_the_flag() {
    let harness = spawn("yolo", false, true).await;
    let session = initialize(harness.addr).await;

    let written = rpc(
        harness.addr,
        session.as_deref(),
        2,
        "tools/call",
        json!({ "name": "set_config", "arguments": { "path": "java.selected_path", "value": "D:/jdk17/bin/java.exe" } }),
    )
    .await;
    assert_ne!(written["result"]["isError"], json!(true), "{written}");

    let records = audit_records(&harness.audit_dir);
    let yolo_ok = records
        .iter()
        .find(|r| r.tool == "set_config" && r.outcome == AuditOutcome::Ok);
    let record = yolo_ok.expect("应有一条 ok 审计");
    assert!(record.yolo, "YOLO 放行必须标记 yolo=true");
    assert!(!record.confirmed, "YOLO 自动放行不算用户确认");
    let _ = std::fs::remove_dir_all(&harness.audit_dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn bearer_token_is_enforced_when_configured() {
    let dir = audit_dir("token");
    let clock: SharedClock = Arc::new(FixedClock::new(1_793_304_753_123));
    let audit = Arc::new(JsonlAudit::new(&dir, clock));
    let server = server(true, false, audit);
    let cfg = McpConfig {
        port: 0,
        token: Some("s3cret".to_string()),
        ..McpConfig::default()
    };
    let (addr, listener) = bind(&cfg).await.expect("应能绑定");
    tokio::spawn(async move {
        let _ = serve_http_on(server, cfg, addr, listener).await;
    });

    let (status, _, _) = post(addr, INITIALIZE, None, None).await;
    assert_eq!(status, 401, "没带令牌必须 401");

    let (status, _, body) = post(addr, INITIALIZE, None, Some("s3cret")).await;
    assert_eq!(status, 200, "带对令牌应通过：{body}");
    let _ = std::fs::remove_dir_all(&dir);
}
