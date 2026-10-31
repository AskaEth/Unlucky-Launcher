//! 真机验收：起 `ull agent serve --stdio`，用裸 JSON-RPC 走一遍握手 → 列工具 → 调用。
//!
//! 这是 M39 任务 5 的自动化形态：**不引任何 MCP 客户端库**，直接按协议喂行给子进程。
//! stdio 形态是 llama.cpp 内置 MCP client 的入口，它的 stdout 必须是纯 JSON-RPC——
//! 本测试顺带守住「stdout 里没有多余输出」这条最容易破的约束。

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

fn data_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ull-agent-stdio-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建临时数据根");
    dir
}

struct Session {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::io::Lines<BufReader<std::process::ChildStdout>>,
}

impl Session {
    fn start(dir: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ull"))
            .args([
                "agent",
                "serve",
                "--stdio",
                "--allow-write",
                "--data-dir",
                &dir.display().to_string(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("应能起 ull agent serve --stdio");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout")).lines();
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").expect("写请求");
        self.stdin.flush().expect("刷新 stdin");
    }

    fn next_response(&mut self) -> Value {
        let line = self
            .stdout
            .next()
            .expect("stdio 应有一行响应（不应挂断）")
            .expect("读响应行失败");
        serde_json::from_str(&line).unwrap_or_else(|_| panic!("stdout 不是 JSON-RPC：{line}"))
    }

    fn rpc(&mut self, id: u32, method: &str, params: Value) -> Value {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        self.next_response()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn stdio_handshake_lists_tools_and_calls_them() {
    let dir = data_dir("handshake");
    let mut session = Session::start(&dir);

    // 1. initialize
    let init = session.rpc(
        1,
        "initialize",
        json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "ull-test", "version": "0.1" }
        }),
    );
    assert_eq!(
        init["result"]["serverInfo"]["name"],
        json!("unlucky-launcher"),
        "{init}"
    );
    session.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    // 2. tools/list —— 写工具在（--allow-write），十个工具全在
    let list = session.rpc(2, "tools/list", json!({}));
    let names: Vec<String> = list["result"]["tools"]
        .as_array()
        .expect("tools 数组")
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect();
    assert_eq!(names.len(), 10, "{names:?}");
    for expected in [
        "get_config",
        "set_config",
        "list_instances",
        "list_accounts",
        "diagnose_crash",
    ] {
        assert!(names.iter().any(|name| name == expected), "缺 {expected}");
    }

    // 3. 只读工具真返回（真实 core 端口：空数据根 → 空实例/空账户）
    let instances = session.rpc(
        3,
        "tools/call",
        json!({ "name": "list_instances", "arguments": {} }),
    );
    assert_ne!(instances["result"]["isError"], json!(true), "{instances}");
    let text = instances["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(text.contains("\"total\":0"), "空数据根应 0 个实例：{text}");

    // 4. 白名单普通键写入真落盘
    let written = session.rpc(
        4,
        "tools/call",
        json!({ "name": "set_config", "arguments": { "path": "download.concurrency", "value": 16 } }),
    );
    assert_ne!(written["result"]["isError"], json!(true), "{written}");
    let launcher = dir.join(".ULL").join("config").join("launcher.json");
    let content = std::fs::read_to_string(&launcher).expect("应写出 launcher.json");
    assert!(content.contains("\"concurrency\""), "{content}");

    // 5. 敏感键被拒（外部客户端没有弹窗），文案引导到设置页 / YOLO
    let denied = session.rpc(
        5,
        "tools/call",
        json!({ "name": "set_config", "arguments": { "path": "java.selected_path", "value": "D:/jdk17/bin/java.exe" } }),
    );
    assert_eq!(denied["result"]["isError"], json!(true), "{denied}");
    let text = denied["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(text.contains("YOLO"), "{text}");

    // 6. 审计落盘：每一条都在，且会话标签能区分「外部 MCP」
    let audit_dir = dir.join(".ULL").join("data").join("agent").join("audit");
    let mut records: Vec<Value> = Vec::new();
    for entry in std::fs::read_dir(&audit_dir)
        .expect("审计目录应存在")
        .flatten()
    {
        let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        for line in text.lines() {
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                records.push(value);
            }
        }
    }
    assert!(
        records
            .iter()
            .any(|record| record["tool"] == json!("set_config") && record["outcome"] == json!("ok")),
        "普通键写入应有 ok 审计：{records:?}"
    );
    assert!(
        records
            .iter()
            .any(|record| record["outcome"] == json!("denied")),
        "敏感键拒绝应有 denied 审计：{records:?}"
    );
    assert!(records
        .iter()
        .all(|record| record["agent_session_id"] == json!("mcp-stdio")));

    let _ = std::fs::remove_dir_all(&dir);
}
