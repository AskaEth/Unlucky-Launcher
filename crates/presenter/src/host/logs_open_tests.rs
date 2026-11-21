//! 「打开历史会话 ⇒ 选中项跟着这一场走」的守门（M458）。
//!
//! 走的是**真 seam**：`CoreHost::open_session` 现读真索引 + 真事件流后发事件，
//! `AppState::apply_core`（线上唯一状态入口）逐条落地。所以 host 造事件与 reducer 记账
//! 两侧的契约一起被钉住，而不是拿一条手搓的事件流自证。
//!
//! ## 缺口长什么样（M453 的会话选择器把它暴露出来）
//!
//! 实时路径的 `CoreEvent::LogAppended` 自带 `session`，`state/live.rs` 就是靠它写
//! `selected_session` 的；历史的 `LogLinesChanged` 载荷里没有会话 id（也不该加：那是全仓
//! 构造点最多的契约），于是「selected 停在 B 时点 A」= 页头/高亮写 B、正文与诊断是 A。
//! 修法：`open_session` 先发一条只带身份的事件 [`CoreEvent::LogSessionOpened`]。
//!
//! M462 的三条用例（回到实时那一场）在同一份夹具上，见 `logs_liveback_tests.rs`
//! （本文件加了那一段会撞 400 行硬线）。

use crate::dto::CoreEvent;
use crate::state::AppState;

use super::logs_open_fixture::{drain_into, finished, fixture};
use super::CoreHost;

/// 判据①：选中项停在 B 时打开 A ⇒ `selected_session == A`，且正文确实是 A 的。
///
/// 变异原文（`host/mod.rs::open_session` 里把新事件那三行删掉，回到 M458 之前）：
///
/// ```ignore
/// let (session, lines, diagnosis) = logs::open(&self.env, id);
/// // if let Some(session) = session { self.send(CoreEvent::LogSessionOpened(session)); }
/// self.send(CoreEvent::LogLinesChanged(lines));
/// ```
///
/// 期望红：`断言 selected 必须是 A` 那条（实测 left `Some("s-b")`，right `Some("s-a")`）。
#[test]
fn opening_a_historical_session_moves_the_selection() {
    let (env, _root) = fixture("open", &[("s-b", "B 的那一行"), ("s-a", "A 的那一行")]);
    let mut host = CoreHost::headless(env);
    let mut state = AppState::new();
    finished(&mut state, "s-b");
    assert_eq!(state.selected_session.as_deref(), Some("s-b"), "夹具前提");

    host.open_session("s-a");
    let events = drain_into(&mut host, &mut state);

    assert_eq!(
        state.selected_session.as_deref(),
        Some("s-a"),
        "打开 A 之后选中项必须是 A（否则页头/高亮继续写上一场）"
    );
    assert!(
        matches!(events.first(), Some(CoreEvent::LogSessionOpened(id)) if id == "s-a"),
        "身份要在日志行之前发出来：{events:?}"
    );
    assert!(
        state
            .log_lines
            .iter()
            .any(|line| line.text.contains("A 的那一行")),
        "正文应当真的是 A 的：{:?}",
        state.log_lines
    );
}

/// 判据③：打开一个**不在索引里**的 id（`ImportCrashLog` 的 `file:<文件名>`）⇒ 选中项不脏。
///
/// 变异原文（把 `open_session` 的条件去掉，无论如何都发身份事件）：
///
/// ```ignore
/// self.send(CoreEvent::LogSessionOpened(session.unwrap_or_else(|| id.to_string())));
/// ```
///
/// 期望红：`selected_session` 变成 `file:…`（选择器里一行都配不上 ⇒ 全部看着没选中），
/// 以及「不许发身份事件」那一条。
#[test]
fn an_id_outside_the_index_does_not_dirty_the_selection() {
    let (env, _root) = fixture("unknown", &[("s-b", "B 的那一行")]);
    let mut host = CoreHost::headless(env);
    let mut state = AppState::new();
    finished(&mut state, "s-b");

    host.open_session("file:crash-2026-11-02_19.40.15-client.txt");
    let events = drain_into(&mut host, &mut state);

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, CoreEvent::LogSessionOpened(_))),
        "不在索引里的 id 不许发身份事件：{events:?}"
    );
    assert_eq!(
        state.selected_session.as_deref(),
        Some("s-b"),
        "选中项不许被一个配不上任何一行的假 id 顶掉"
    );
    // 其余行为不变：占位诊断照发（`rule` 为空 ⇒ 页面不画），身份仍写在报告里。
    assert!(
        matches!(events.last(), Some(CoreEvent::DiagnosisReady(report)) if report.session.starts_with("file:")),
        "{events:?}"
    );
}

/// 前缀 key（`logs::open` 与 CLI 一样接受唯一前缀）⇒ 事件里必须是**索引里的整条 id**。
///
/// 理由：选择器拿 `selected` 去和 `log_sessions.id` 逐字比对（`app/view/logs.rs` 的
/// `described`），前缀配不上任何一行 ⇒ 高亮会整体消失。把解析结果（而不是调用方给的 key）
/// 写进状态，页面与状态才是同一个身份。
///
/// 变异原文（`logs::open` 改成 `(Some(key.to_string()), lines, diagnosis)`）：
///
/// ```ignore
/// (Some(key.to_string()), lines, diagnosis)
/// ```
///
/// 期望红：`selected_session` 变成 `sess-18`。
#[test]
fn a_prefix_key_is_announced_as_the_full_session_id() {
    let (env, _root) = fixture(
        "prefix",
        &[("sess-1840", "十八点四十"), ("sess-1922", "十九点二十二")],
    );
    let mut host = CoreHost::headless(env);
    let mut state = AppState::new();

    host.open_session("sess-18");
    let events = drain_into(&mut host, &mut state);

    assert!(
        events
            .iter()
            .any(|event| matches!(event, CoreEvent::LogSessionOpened(id) if id == "sess-1840")),
        "身份事件必须带索引里的整条 id：{events:?}"
    );
    assert_eq!(
        state.selected_session.as_deref(),
        Some("sess-1840"),
        "选中项要是整条 id，前缀在选择器里一行都配不上"
    );
}
