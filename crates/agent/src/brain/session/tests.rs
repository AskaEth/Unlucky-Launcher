//! [`super`] 的会话用例（拆出来守 400 行硬线；纯搬移，语义一字未改）。

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::brain::chat::{ChatProvider, ChatReply, ChatRequest, DeltaObserver, ProviderFuture};
use crate::confirm::{ConfirmPolicy, NoUiGate};
use crate::event::{AgentEvent, MemoryEventSink};
use crate::testing::mock_ports;

/// 回放型 provider：按顺序吐出预设回复，用完最后一条后重复它。
struct Scripted {
    config: ProviderConfig,
    replies: Mutex<Vec<ChatReply>>,
    calls: Arc<AtomicUsize>,
}

impl Scripted {
    fn factory(replies: Vec<ChatReply>) -> (ProviderFactory, Arc<AtomicUsize>) {
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

fn local_provider() -> ProviderConfig {
    ProviderConfig::openai_compatible("http://127.0.0.1:8080/v1", "test-model")
}

fn config(dir: &std::path::Path) -> SessionConfig {
    SessionConfig::new(local_provider(), ConfirmPolicy::default(), dir).with_session_id("chat-test")
}

fn open(dir: &std::path::Path, replies: Vec<ChatReply>) -> (Session, Arc<MemoryEventSink>) {
    let (ports, _settings) = mock_ports();
    let (factory, _calls) = Scripted::factory(replies);
    let sink = Arc::new(MemoryEventSink::new());
    let session = Session::open(
        &ports,
        &factory,
        config(dir),
        Arc::new(NoUiGate),
        sink.clone(),
    )
    .expect("装配应成功");
    (session, sink)
}

// 本 crate 的测试不引 tokio 的测试宏（那要 `rt` feature 之外的东西）；用 core 的阻塞桥跑。
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    unlucky_core::download::blocking::block_on(future)
}

#[test]
fn a_turn_appends_user_and_assistant_to_the_history_it_owns() {
    let dir = std::env::temp_dir().join("ull-agent-session-history");
    let (mut session, sink) = open(&dir, vec![ChatReply::text("先确认版本。")]);
    assert!(session.history().is_empty());

    let outcome = block_on(session.run_turn("装 Fabric 要注意什么")).expect("一轮应成功");
    assert_eq!(outcome.text, "先确认版本。");
    assert_eq!(outcome.steps, 1);
    assert_eq!(session.history().len(), 2, "user + assistant");
    assert!(matches!(
        session.history().first(),
        Some(ChatMessage::User { .. })
    ));
    assert!(sink.streamed_text().contains("先确认版本"));
    assert_eq!(session.agent_session_id(), "chat-test");
}

#[test]
fn all_ten_tools_are_wired_into_the_gui_session() {
    let dir = std::env::temp_dir().join("ull-agent-session-tools");
    let (session, _sink) = open(&dir, vec![ChatReply::text("嗯")]);
    let names = session.tool_names();
    assert_eq!(names.len(), 10, "v1 工具是十个：{names:?}");
    for expected in [
        "read_game_log",
        "set_config",
        "get_config",
        "diagnose_crash",
    ] {
        assert!(names.contains(&expected), "缺 {expected}：{names:?}");
    }
}

#[test]
fn an_incomplete_provider_config_fails_at_assembly_not_at_the_first_turn() {
    let (ports, _settings) = mock_ports();
    let (factory, _calls) = Scripted::factory(vec![ChatReply::text("不该跑到这里")]);
    // 没有模型名 ⇒ ProviderConfig::validate 拒
    let broken = ProviderConfig::openai_compatible("http://127.0.0.1:8080/v1", "");
    let error = Session::open(
        &ports,
        &factory,
        SessionConfig::new(broken, ConfirmPolicy::default(), std::env::temp_dir()),
        Arc::new(NoUiGate),
        Arc::new(MemoryEventSink::new()),
    )
    .expect_err("配置不完整时装配必须失败");
    assert!(matches!(error, SessionError::Provider(_)), "{error}");
    assert!(error.to_string().contains("模型名"), "{error}");
}

#[test]
fn a_tool_call_round_trips_through_the_executor_and_is_visible_in_events() {
    let dir = std::env::temp_dir().join("ull-agent-session-tool-call");
    let call = ChatReply::with_tool_calls(vec![crate::brain::ToolCall {
        id: "c1".to_string(),
        name: "get_config".to_string(),
        arguments: serde_json::json!({}),
    }]);
    let (mut session, sink) = open(&dir, vec![call, ChatReply::text("读完了")]);

    let outcome = block_on(session.run_turn("看看配置")).expect("一轮应成功");
    assert_eq!(outcome.tool_calls, 1);
    assert_eq!(outcome.text, "读完了");
    let events = sink.events();
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::ToolCallStarted { tool, .. } if tool == "get_config"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::ToolCallFinished { tool, ok: true, .. } if tool == "get_config"
    )));
    // 历史里是 user → assistant(带 tool_calls) → tool → assistant
    assert_eq!(session.history().len(), 4);
}

// ---------------------------------------------------------------- M388：时限与取消

/// 永远不 Ready 的 provider（「端点收了请求就不说话」）。
struct Silent {
    config: ProviderConfig,
}

impl ChatProvider for Silent {
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn complete<'a>(
        &'a self,
        _request: &'a ChatRequest,
        _observer: &'a dyn DeltaObserver,
    ) -> ProviderFuture<'a> {
        Box::pin(std::future::pending())
    }
}

/// 第一次挂住（模拟「请求发出去就没回来」），之后正常回一句（模拟「重试成功」）。
struct SilentOnce {
    config: ProviderConfig,
    calls: Arc<AtomicUsize>,
}

impl ChatProvider for SilentOnce {
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn complete<'a>(
        &'a self,
        _request: &'a ChatRequest,
        observer: &'a dyn DeltaObserver,
    ) -> ProviderFuture<'a> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return Box::pin(std::future::pending());
        }
        observer.on_delta("第二次就通了");
        Box::pin(async move { Ok(ChatReply::text("第二次就通了")) })
    }
}

fn session_with(
    dir: &std::path::Path,
    factory: &ProviderFactory,
    timeout: std::time::Duration,
) -> Session {
    let (ports, _settings) = mock_ports();
    let mut config = config(dir);
    config.request_timeout = timeout;
    Session::open(
        &ports,
        factory,
        config,
        Arc::new(NoUiGate),
        Arc::new(MemoryEventSink::new()) as Arc<dyn crate::event::AgentEventSink>,
    )
    .expect("装配应成功")
}

#[test]
fn a_silent_provider_ends_the_turn_with_a_timeout_and_rolls_the_history_back() {
    let dir = std::env::temp_dir().join("ull-agent-session-timeout");
    let factory: ProviderFactory = Arc::new(|config: &ProviderConfig| {
        Ok(Arc::new(Silent {
            config: config.clone(),
        }) as Arc<dyn ChatProvider>)
    });
    let mut session = session_with(&dir, &factory, std::time::Duration::from_millis(80));

    let error =
        block_on(session.run_turn("你好")).expect_err("端点不理人 ⇒ 必须判超时，而不是挂住");
    assert!(matches!(error, BrainError::Timeout { .. }), "{error}");
    assert!(
        session.history().is_empty(),
        "超时那一轮不许在历史里留下悬空的 user 消息（否则重试带着半截上下文）"
    );
}

#[test]
fn cancelling_ends_the_turn_and_the_next_turn_still_runs() {
    let dir = std::env::temp_dir().join("ull-agent-session-cancel");
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let factory: ProviderFactory = Arc::new(move |config: &ProviderConfig| {
        Ok(Arc::new(SilentOnce {
            config: config.clone(),
            calls: Arc::clone(&counter),
        }) as Arc<dyn ChatProvider>)
    });
    // 时限给足（5 秒）：这条要证的是「取消」，不是超时。
    let mut session = session_with(&dir, &factory, std::time::Duration::from_secs(5));
    let cancel = session.cancel_handle();

    let canceller = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(40));
        cancel.cancel();
    });
    let error = block_on(session.run_turn("你好")).expect_err("取消必须让本轮结束");
    canceller.join().expect("取消线程不该 panic");
    assert_eq!(error, BrainError::Cancelled);
    assert!(session.history().is_empty(), "被取消的那一轮要回滚");

    // 重试：不重启、不手动清旗标（`run_turn` 每轮开头 reset）。
    let outcome = block_on(session.run_turn("再说一次")).expect("取消之后必须能立刻重试");
    assert_eq!(outcome.text, "第二次就通了");
    assert_eq!(session.history().len(), 2, "重试那一轮正常入历史");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

// ------------------------------------------------- M388 r1 P2-2：空闲计时按「每次请求」重置

/// 确认闸门：**先睡一段再放行**（模拟「工具执行比空闲时限还长」）。
struct SlowGate(std::time::Duration);

impl ConfirmGate for SlowGate {
    fn request(&self, _request: crate::confirm::ConfirmRequest) -> crate::confirm::ConfirmFuture {
        let wait = self.0;
        Box::pin(async move {
            tokio::time::sleep(wait).await;
            crate::confirm::ConfirmDecision::Approved
        })
    }
}

/// 第一步立刻回工具调用、第二步**过一会儿**才回（模拟「请求刚发出，正在路上」）。
struct SlowSecond {
    config: ProviderConfig,
    calls: Arc<AtomicUsize>,
}

impl ChatProvider for SlowSecond {
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn complete<'a>(
        &'a self,
        _request: &'a ChatRequest,
        observer: &'a dyn DeltaObserver,
    ) -> ProviderFuture<'a> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            let call = crate::brain::ToolCall {
                id: "c1".to_string(),
                name: "set_config".to_string(),
                // High 敏感键 ⇒ 走确认闸门（这里就是「把工具那一段拉长」的手段）
                arguments: serde_json::json!({
                    "path": "java.selected_path",
                    "value": "D:/jdk17/bin/java.exe",
                }),
            };
            return Box::pin(async move { Ok(ChatReply::with_tool_calls(vec![call])) });
        }
        // 第二次请求「在路上」20ms（< 时限），不是立刻 Ready：否则 Guard 的第一次 poll 会因为
        // provider Ready 而不进空闲判据，这条用例就抓不住 P2-2。
        //
        // 增量**必须放在 sleep 之后**：`on_delta` 会 `activity.touch()`（重置计时），先吐一段
        // 就等于替被测代码把计时器重置了，变异版本会因此假绿（这是本用例第一版踩到的坑）。
        Box::pin(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            observer.on_delta("好了");
            Ok(ChatReply::text("好了"))
        })
    }
}

/// 复审 r1 P2-2：一步工具跑得比空闲时限还长时，**下一步请求不许一发出就被判超时**。
///
/// 抓得住什么（实测变异：删掉 `turn.rs` 的 `activity.restart()` 后本用例红）：工具那一段
/// 200ms > 时限 100ms，第二步的 Guard 一进来就看到 `idle(200ms) >= timeout(100ms)` ⇒ 当场
/// `BrainError::Timeout`，而那个请求其实刚刚发出。
/// 抓不住什么：真实工具真的跑那么久的情形（这里用确认闸门把工具那一段拉长，不需要真 I/O）。
#[test]
fn a_tool_step_longer_than_the_timeout_does_not_time_out_the_next_request() {
    let dir = std::env::temp_dir().join("ull-agent-session-slow-tool");
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let factory: ProviderFactory = Arc::new(move |config: &ProviderConfig| {
        Ok(Arc::new(SlowSecond {
            config: config.clone(),
            calls: Arc::clone(&counter),
        }) as Arc<dyn ChatProvider>)
    });
    let (ports, _settings) = mock_ports();
    let mut config = config(&dir);
    // 时限 100ms，工具那一段 200ms：旧实现下第二步一发出就判超时
    config.request_timeout = std::time::Duration::from_millis(100);
    let mut session = Session::open(
        &ports,
        &factory,
        config,
        Arc::new(SlowGate(std::time::Duration::from_millis(200))),
        Arc::new(MemoryEventSink::new()) as Arc<dyn crate::event::AgentEventSink>,
    )
    .expect("装配应成功");

    let started = std::time::Instant::now();
    let outcome = block_on(session.run_turn("把 java 路径设一下"))
        .expect("工具步比时限长，不许把下一步请求判成超时");
    eprintln!("[probe] 一轮耗时 {:?}", started.elapsed());
    assert!(
        started.elapsed() >= std::time::Duration::from_millis(150),
        "前提：工具那一段真的被拉长了（确认闸门被调用过）：{:?}",
        started.elapsed()
    );
    assert_eq!(outcome.tool_calls, 1);
    assert_eq!(outcome.text, "好了");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "两轮补全都是真的发出去了（第二轮不该被超时短路）"
    );
    assert_eq!(
        session.history().len(),
        4,
        "user → assistant(tool) → tool → assistant"
    );
}
