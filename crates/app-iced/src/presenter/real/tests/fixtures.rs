//! 用例夹具：临时目录 / `launcher.json` 写入 / 回放 provider / 事件泵 / 审计读取。
//!
//! 从 `real/tests.rs` 拆出来的唯一理由是 400 行红线（`guards.yml` 的 file-size 门禁）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use unlucky_agent::brain::chat::{
    ChatProvider, ChatReply, ChatRequest, DeltaObserver, ProviderFuture,
};
use unlucky_agent::brain::provider::ProviderConfig;
use unlucky_agent::brain::ProviderFactory;
use unlucky_agent::brain::ToolCall;
use unlucky_agent::port::Ports;
use unlucky_presenter::agent::AgentRail;
use unlucky_presenter::dto::{AgentEvent as DtoEvent, AppEvent};

use unlucky_cli::commands::agent::ports;
use unlucky_cli::context::Ctx;

use super::super::{Plan, RealTransport};

// ───────────────────────────── 夹具 ─────────────────────────────

/// 一次性临时目录（进程内唯一；用例之间不共享）。
pub(super) fn temp_dir(name: &str) -> PathBuf {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ull-m384-{name}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建临时目录");
    dir
}

/// 写一份 `launcher.json`（`<home>/config/launcher.json`），返回它的路径。
pub(super) fn write_launcher(home: &Path, entries: Value) -> PathBuf {
    let config = home.join("config").join("launcher.json");
    std::fs::create_dir_all(config.parent().expect("config 目录")).expect("建 config 目录");
    std::fs::write(
        &config,
        serde_json::to_string_pretty(&entries).expect("序列化"),
    )
    .expect("写 launcher.json");
    config
}

/// 回放型 provider：按顺序吐出预设回复，最后一条会重复。
pub(super) struct Scripted {
    config: ProviderConfig,
    replies: Mutex<Vec<ChatReply>>,
    calls: Arc<AtomicUsize>,
}

impl ChatProvider for Scripted {
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn complete<'a>(
        &'a self,
        _request: &'a ChatRequest,
        observer: &'a dyn DeltaObserver,
    ) -> ProviderFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let reply = {
            let mut replies = self.replies.lock().expect("脚本");
            if replies.len() > 1 {
                replies.remove(0)
            } else {
                replies.first().cloned().unwrap_or_default()
            }
        };
        if !reply.text.is_empty() {
            observer.on_delta(&reply.text);
        }
        Box::pin(async move { Ok(reply) })
    }
}

/// 造一个回放 provider 工厂 + 调用计数器（计数用来断言「真跑过几轮补全」）。
pub(super) fn scripted(replies: Vec<ChatReply>) -> (ProviderFactory, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let cell = Arc::new(Mutex::new(replies));
    let factory: ProviderFactory = Arc::new(move |config: &ProviderConfig| {
        Ok(Arc::new(Scripted {
            config: config.clone(),
            replies: Mutex::new(cell.lock().expect("脚本").clone()),
            calls: Arc::clone(&counter),
        }) as Arc<dyn ChatProvider>)
    });
    (factory, calls)
}

/// 一条工具调用回复。
pub(super) fn tool_call(name: &str, arguments: Value) -> ChatReply {
    ChatReply::with_tool_calls(vec![ToolCall {
        id: format!("call-{name}"),
        name: name.to_string(),
        arguments,
    }])
}

/// 临时数据根上的生产端口（与 GUI 同一个装配函数 `ports::build`）。
pub(super) fn temp_ports(dir: &Path) -> Ports {
    let ctx = Ctx::resolve_in(
        &unlucky_cli::cli::Globals {
            data_dir: Some(dir.to_path_buf()),
            ..unlucky_cli::cli::Globals::default()
        },
        None,
    )
    .expect("临时数据根必须能解析");
    ports::build(&ctx)
}

/// 抽干队列里的 presenter 事件（用例断言用）。
pub(super) fn drained(
    queue: &crate::app::state::presenter::transport::EventQueue,
) -> Vec<DtoEvent> {
    queue
        .lock()
        .expect("queue")
        .drain(..)
        .filter_map(|event| match event {
            AppEvent::Agent(event) => Some(event),
            _ => None,
        })
        .collect()
}

/// 池化队列直到 `done` 为真（上限 10 秒；超时把已有事件打出来）。
pub(super) fn pump(transport: &RealTransport, done: impl Fn(&[AppEvent]) -> bool) -> Vec<AppEvent> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut collected: Vec<AppEvent> = Vec::new();
    loop {
        if let Ok(mut queue) = transport.queue.lock() {
            collected.extend(queue.drain(..));
        }
        if done(&collected) {
            return collected;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "等事件超时；已有：{collected:#?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// 夹具：临时数据根 + 一份「配好了」的 launcher.json + 回放 provider。
///
/// 放在 fixtures 里是因为 `e2e.rs` 与 `yolo_live.rs` 都要用（M389 拆卷时的纯搬移）。
pub(super) fn harness(
    name: &str,
    replies: Vec<ChatReply>,
    ai_extra: Value,
) -> (RealTransport, PathBuf, PathBuf, Arc<AtomicUsize>) {
    let home = temp_dir(&format!("{name}-home"));
    let data = temp_dir(&format!("{name}-data"));
    let mut ai = json!({
        "enabled": true,
        "provider": "openai_compatible",
        "base_url": "http://127.0.0.1:8080/v1",
        "model": "replay-model",
    });
    if let (Some(ai), Some(extra)) = (ai.as_object_mut(), ai_extra.as_object()) {
        for (key, value) in extra {
            ai.insert(key.clone(), value.clone());
        }
    }
    let config = write_launcher(&home, json!({ "ai": ai }));
    let plan = Plan::at(&config).expect("夹具配置必须完整");
    let (factory, calls) = scripted(replies);
    let transport = RealTransport::spawn(plan, temp_ports(&data), factory);
    (transport, home, data, calls)
}

pub(super) fn dto_events(events: &[AppEvent]) -> Vec<&DtoEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            AppEvent::Agent(event) => Some(event),
            _ => None,
        })
        .collect()
}

pub(super) fn has_tool_finished(events: &[AppEvent], tool: &str, ok: bool) -> bool {
    dto_events(events).iter().any(|event| {
        matches!(
            event,
            DtoEvent::ToolCallFinished { name, ok: found, .. } if name == tool && *found == ok
        )
    })
}

pub(super) fn last_assistant_text(events: &[AppEvent]) -> String {
    // 走真 presenter 的投影规则：把事件喂给 AgentRail，读最终视图模型。
    let mut rail = AgentRail::new();
    for event in dto_events(events) {
        rail.ingest(event);
    }
    rail.messages
        .iter()
        .filter(|message| message.role == unlucky_presenter::dto::ChatRole::Assistant)
        .map(|message| message.text.as_str())
        .collect::<Vec<_>>()
        .join("")
}

pub(super) fn audit_lines(home: &Path) -> Vec<Value> {
    let dir = unlucky_agent::audit::audit_dir(home);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for entry in entries.flatten() {
        let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                lines.push(value);
            }
        }
    }
    lines
}
