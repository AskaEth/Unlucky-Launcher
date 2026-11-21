//! 「测试连接」的守卫（M174）。四类失败**各有一条真 TCP 假服务器**用例——不是拿字符串假装的。
//!
//! | 用例 | 守什么 | 变异（改成什么会红） |
//! |---|---|---|
//! | `auth_failure_is_classified_as_auth` | 401 → 鉴权失败 | `map_error` 的 401 分支改成 `Other` |
//! | `missing_model_is_model_not_found` | 404 → 模型不存在 | 把 404 归进 auth |
//! | `closed_port_is_unreachable` | 连不上 → 端点不可达 | 把 `ErrorKind::Http` 归到 `Other` |
//! | `silent_server_times_out` | 不响应 → 超时，且在**本次预算**到点收手 | 去掉 `tokio::time::timeout`（那就得等假服务器睡满 5s） |
//! | `success_is_one_real_completion` | 200 + SSE → 通过 | 把 `Ok(Ok(_))` 当成失败 |
//! | `the_request_carries_the_configured_model` | **请求体里的 `model` 就是配置那个** | `probe_with` 里把 model 写死成别的名字 |
//! | 每条失败用例都断言 `target` | 失败结论带「对哪个端点/模型」 | `outcome_from_error` 丢掉 target（r1 复审 P2-1） |

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use unlucky_agent::brain::rig::RigProvider;

use super::{probe_with, AiSettings, ProbeOutcome};

/// 假服务器的行为。
enum Behaviour {
    /// 立刻回这一份（状态行 + Content-Type + body）。
    Reply(&'static str, &'static str, &'static str),
    /// 收下请求但**永远不回**（超时用例）。
    Silent,
}

/// 一条收到的请求：头 + 正文（P2-3：正文里才有 `model`）。
#[derive(Clone)]
struct Hit {
    head: String,
    body: String,
}

/// 一个只服务 127.0.0.1 的假 HTTP 服务器：记下每一份请求，按 `behaviour` 回。
struct Server {
    port: u16,
    hits: Arc<Mutex<Vec<Hit>>>,
}

impl Server {
    fn start(behaviour: Behaviour) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑本地端口");
        let port = listener.local_addr().expect("本地地址").port();
        let hits = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                handle(stream, &behaviour, &sink);
            }
        });
        Self { port, hits }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    fn hits(&self) -> Vec<Hit> {
        self.hits.lock().expect("锁").clone()
    }
}

fn handle(mut stream: TcpStream, behaviour: &Behaviour, sink: &Arc<Mutex<Vec<Hit>>>) {
    let mut buffer = [0_u8; 8192];
    let mut raw = Vec::new();
    // 先读头，再按 `Content-Length` 把正文读全（P2-3：只读头就断言不到 `model`）。
    let head_end = loop {
        if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        match stream.read(&mut buffer) {
            Ok(0) => break raw.len(),
            Ok(n) => raw.extend_from_slice(&buffer[..n]),
            Err(_) => return,
        }
    };
    let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
    let length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    while raw.len() < head_end + length {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buffer[..n]),
            Err(_) => break,
        }
    }
    let body =
        String::from_utf8_lossy(&raw[head_end..raw.len().min(head_end + length)]).to_string();
    sink.lock().expect("锁").push(Hit { head, body });

    match behaviour {
        Behaviour::Silent => {
            // 拖住这条连接（客户端会先超时）：比客户端时限长得多。
            std::thread::sleep(Duration::from_secs(5));
        }
        Behaviour::Reply(status, content_type, body) => {
            let response = format!(
                "{status}\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
                len = body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    }
}

fn settings(base_url: &str, model: &str) -> AiSettings {
    AiSettings {
        enabled: true,
        provider: "openai_compatible".to_string(),
        base_url: base_url.to_string(),
        model: model.to_string(),
        api_key: Some("sk-test-not-a-real-key".to_string()),
    }
}

const AUTH_BODY: &str = r#"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error","code":"invalid_api_key"}}"#;
const MISSING_MODEL_BODY: &str =
    r#"{"error":{"message":"model_not_found: no such model gpt-nope","code":"model_not_found"}}"#;

/// 一份最小但合法的 OpenAI 流式回复（rig 走 `model.stream(..)`，要 SSE 不是整包 JSON）。
const SSE_BODY: &str = concat!(
    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1700000000,\"model\":\"gpt-test\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"pong\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1700000000,\"model\":\"gpt-test\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n",
    "data: [DONE]\n\n"
);

#[test]
fn auth_failure_is_classified_as_auth() {
    let server = Server::start(Behaviour::Reply(
        "HTTP/1.1 401 Unauthorized",
        "application/json",
        AUTH_BODY,
    ));
    let outcome = probe_with(
        &RigProvider::factory(),
        &settings(&server.base_url(), "gpt-test"),
        Duration::from_secs(5),
    );
    let ProbeOutcome::Auth { target, detail } = &outcome else {
        panic!("401 必须归到鉴权失败，实际 {outcome:?}");
    };
    assert!(!detail.is_empty());
    // P2-1：失败结论必须说清「对哪个端点 / 哪个模型」。
    assert_eq!(target.base_url, server.base_url());
    assert_eq!(target.model, "gpt-test");
    // 请求确实发出去了（否则「鉴权失败」是凭空说的）。
    assert_eq!(server.hits().len(), 1, "鉴权失败之前必须真的连上过");
}

#[test]
fn missing_model_is_model_not_found() {
    let server = Server::start(Behaviour::Reply(
        "HTTP/1.1 404 Not Found",
        "application/json",
        MISSING_MODEL_BODY,
    ));
    let outcome = probe_with(
        &RigProvider::factory(),
        &settings(&server.base_url(), "gpt-nope"),
        Duration::from_secs(5),
    );
    assert_eq!(
        outcome.kind(),
        "model_not_found",
        "404 必须归到模型不存在，实际 {outcome:?}"
    );
    assert_eq!(outcome.target().map(|t| t.model.as_str()), Some("gpt-nope"));
}

#[test]
fn closed_port_is_unreachable() {
    // 绑一个端口再放掉：这个端口上没人监听 = 连接被拒。
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑端口");
        listener.local_addr().expect("地址").port()
    };
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let outcome = probe_with(
        &RigProvider::factory(),
        &settings(&base_url, "gpt-test"),
        Duration::from_secs(5),
    );
    assert_eq!(
        outcome.kind(),
        "unreachable",
        "没人监听的端口必须归到端点不可达，实际 {outcome:?}"
    );
    assert_eq!(
        outcome.target().map(|t| t.base_url.as_str()),
        Some(base_url.as_str())
    );
}

/// 超时用例的预算：跟产品默认时限（`DEFAULT_TIMEOUT` = 8s）同一个量级，取 1/20 只是为了让整个
/// 测试套不必等 8 秒。判据只依赖「预算生效时远早于假服务器沉默的 5 秒」，不依赖这个绝对值。
const TIMEOUT_BUDGET: Duration = Duration::from_millis(400);

/// 允许的调度噪声：一次本地回环往返 + 建 tokio 运行时是毫秒级，1 秒是给繁忙机器的余量；它必须
/// **远小于**「预算没生效」的代价（假服务器要睡 5s 才收手），否则两种情形区分不开。
const TIMEOUT_SLACK: Duration = Duration::from_secs(1);

/// 不回应 ⇒ 超时，且在**本次预算**到点时就收手（不是等对面 5 秒沉默、也不是等别处的默认时限）。
///
/// 三层判据各管一件事，别混：
///
/// 1. **走了哪条分支**：`kind == "timeout"`。没被本模块预算截住的「不回应」落到的是传输层错误
///    ——实测：摘掉 `tokio::time::timeout` ⇒ 假服务器睡满 5s 后关连接，结论是 `unreachable`，
///    所以这一条先把「根本没走到超时分支」挡在外面。
/// 2. **是本次预算那条分支**：`detail` 里的时限就是传进去的预算（Timeout 分支把 `timeout` 原样
///    写进 detail）。它证明「结论由这条预算分支产出」，**不**证明「计时器用的就是这个值」：分支
///    里把计时值换掉、detail 照抄参数，本条抓不住——那是下一条的活。
/// 3. **墙钟**：`elapsed < 预算 + SLACK`，抓的正是上一条抓不住的形态（「分支对、但等的不是这个
///    预算」）。实测：把计时值硬编码成 3s ⇒ 本断言红（3.006s > 1.4s）。
///
/// 抓得住：「预算没生效 / 生效的不是这个预算」——那种情况要等假服务器睡满 5s 或等 3s 之类的
/// 别处时限，远超 `预算 + SLACK`。
/// 抓不住：预算到底是 400ms 还是 1.2s（都落在 SLACK 里），也不量定时器精度；机器再忙只吃掉
/// SLACK 的余量，不吃预算本身。
#[test]
fn silent_server_times_out() {
    // 假服务器收下请求后睡 5s（`Behaviour::Silent`）：这就是「预算没生效」时本用例会等到的墙钟。
    let server = Server::start(Behaviour::Silent);
    let started = std::time::Instant::now();
    let outcome = probe_with(
        &RigProvider::factory(),
        &settings(&server.base_url(), "gpt-test"),
        TIMEOUT_BUDGET,
    );
    let elapsed = started.elapsed();

    assert_eq!(
        outcome.kind(),
        "timeout",
        "不回应的端点必须归到超时，实际 {outcome:?}"
    );
    let ProbeOutcome::Timeout { detail, .. } = &outcome else {
        unreachable!("上一行已经断言 kind 是 timeout")
    };
    assert!(
        detail.contains(&TIMEOUT_BUDGET.as_secs_f32().to_string()),
        "超时结论里的时限不是本次预算（{TIMEOUT_BUDGET:?}）：{detail}"
    );
    assert_eq!(
        server.hits().len(),
        1,
        "超时之前必须真的把一次请求发给了假服务器（否则「超时」可能只是没连上）"
    );
    assert!(
        elapsed < TIMEOUT_BUDGET + TIMEOUT_SLACK,
        "预算没生效或收手太晚：预算 {TIMEOUT_BUDGET:?}，实际 {elapsed:?}；假服务器会沉默 5s，\
         超过 {:?} 就说明等的不是这个预算",
        TIMEOUT_BUDGET + TIMEOUT_SLACK
    );
}

#[test]
fn success_is_one_real_completion() {
    let server = Server::start(Behaviour::Reply(
        "HTTP/1.1 200 OK",
        "text/event-stream",
        SSE_BODY,
    ));
    let outcome = probe_with(
        &RigProvider::factory(),
        &settings(&server.base_url(), "gpt-test"),
        Duration::from_secs(5),
    );
    match &outcome {
        ProbeOutcome::Passed { target } => {
            assert_eq!(target.base_url, server.base_url());
            assert_eq!(target.model, "gpt-test");
        }
        other => panic!("200 + 合法 SSE 必须算通过，实际 {other:?}"),
    }
}

/// **P2-3**：请求体里的 `model` 必须是用户配置的那个（不是写死的）。
///
/// 变异：把 `probe_with` 的 `model: target.model.clone()` 改成 `"gpt-4o".to_string()` ⇒ 本用例
/// 断言 `\"model\":\"gpt-test\"` 失败。这是「配置里的模型真的上了线」唯一可信的证据：只看
/// 状态码通过并不能证明服务端收到的是哪个模型。
#[test]
fn the_request_carries_the_configured_model() {
    let server = Server::start(Behaviour::Reply(
        "HTTP/1.1 200 OK",
        "text/event-stream",
        SSE_BODY,
    ));
    let outcome = probe_with(
        &RigProvider::factory(),
        &settings(&server.base_url(), "gpt-test"),
        Duration::from_secs(5),
    );
    assert!(outcome.is_pass(), "前提：这一轮应当通过，实际 {outcome:?}");

    let hits = server.hits();
    assert_eq!(hits.len(), 1, "应当恰好一次请求");
    let hit = &hits[0];
    assert!(
        hit.head.starts_with("POST /v1/chat/completions"),
        "请求行不对：{}",
        hit.head.lines().next().unwrap_or("")
    );
    assert!(
        !hit.body.is_empty(),
        "没有读到请求正文（夹具没按 Content-Length 读 body？）"
    );
    assert!(
        hit.body.contains("\"model\":\"gpt-test\""),
        "请求正文里的 model 不是配置里的那个：{}",
        hit.body
    );
}

#[path = "tests_guards.rs"]
mod guards;
