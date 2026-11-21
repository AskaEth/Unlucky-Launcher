//! M462 的 host 侧守门：**历史视图 → 回到正在跑那一场**。
//!
//! 从 `logs_open_tests.rs` 单独成文件（那边加完这一段会撞 400 行硬线），夹具与 M458 那份共用
//! （`logs_open_fixture.rs`）。走真 seam：`CoreHost::open_session` 发事件 → `AppState::apply_core`
//! 落地，所以「点击 → 退出历史视图 → 下一拍实时行进正文」是一条命令走完的，不是只渲染了一个控件。

use std::path::PathBuf;

use crate::dto::{CoreEvent, LiveLogBatch, LogLevel, LogLine};
use crate::state::AppState;

use super::env::HostEnv;
use super::logs_open_fixture::{drain_into, finished, fixture, TempRoot};
use super::CoreHost;

/// 夹具里「正在跑」的那一场（**故意不在索引里**：索引只在收尾时追加）。
const LIVE: &str = "s-live-2001";

// ---------------------------------------------------------------------------
// M462：回到实时那一场（历史视图 → 正在跑的那一场）
// ---------------------------------------------------------------------------
//
// 这一段的现场事实（与 `state/log_view.rs` 的模块文档同源）：
//
// * 会话索引**只在收尾时**追加（`core::launch::session::SessionWriter::finish` →
//   `session_index::append`），所以正在跑那一场不在索引里 ⇒ `host::logs::open` 的 `find`
//   找不到它。`CoreHost::open_session` 只能靠**尾随器**认出它（`tail.session() == id`）。
// * 尾随器挂在 `env.layout.game_logs()` 下的 `<session-id>.jsonl` 上（`host/handle.rs` 的
//   `HostTask::LaunchStarted` 分支）；这里直接 `attach` 同一份路径，等价于「一场刚起来」。

/// 一场**正在跑**的会话：事件流已经在盘上，**索引里没有它**。返回尾随器该挂的路径。
fn running(env: &HostEnv, id: &str, texts: &[&str]) -> PathBuf {
    let file = env.sessions_root().join(format!("game/{id}.jsonl"));
    std::fs::create_dir_all(file.parent().expect("父目录")).expect("建会话目录");
    let body: String = texts
        .iter()
        .map(|text| format!("{{\"kind\":\"stdout\",\"payload\":{{\"line\":\"{text}\"}}}}\n"))
        .collect();
    std::fs::write(&file, body).expect("写事件流");
    file
}

/// 实时一拍（`host::handle::poll_tail` 发出的就是这一条）。
fn tick(state: &mut AppState, live: &str, text: &str) {
    state.apply_core(CoreEvent::LogAppended(LiveLogBatch {
        session: live.to_string(),
        lines: vec![LogLine {
            text: text.to_string(),
            level: LogLevel::Info,
        }],
        running: true,
    }));
}

/// 正文实测（断言里读得懂）。
fn body(state: &AppState) -> Vec<String> {
    state
        .log_lines
        .iter()
        .map(|line| line.text.clone())
        .collect()
}

/// 夹具：一场历史（`s-old`）+ 一场**正在跑**（[`LIVE`]，不在索引里，尾随器挂在它身上），
/// 正文此刻已经进了实时那一场的一行。
///
/// `label` 在并行的用例之间**必须各不相同**：`fixture` 的临时目录按 `label + pid` 拼，
/// 同名就会互相删（实测过：并发跑两条同名用例，一条在 `写事件流` 处报 NotFound）。
fn live_in_history(label: &str) -> (TempRoot, AppState, CoreHost) {
    let (env, root) = fixture(label, &[("s-old", "[18:40] boom: GLFW error")]);
    let path = running(&env, LIVE, &["[20:01] launching 1.20.4"]);
    let mut host = CoreHost::headless(env);
    host.tail.attach(path, LIVE);
    let mut state = AppState::new();
    tick(&mut state, LIVE, "[20:01] launching 1.20.4");
    assert_eq!(
        state.live_session.as_deref(),
        Some(LIVE),
        "夹具前提：实时在跑"
    );
    (root, state, host)
}

/// **判据①**：有实时会话 + 在历史视图 ⇒ 回到实时的入口存在，且它指向正在跑那一场。
///
/// 变异原文（`state/log_view.rs` 的 `live_return` 里把「仍在跑」那一位去掉，只判历史视图）：
///
/// ```ignore
/// pub fn live_return(&self) -> Option<&str> {
///     if !self.history { return None; }
///     self.held.as_deref()
/// }
/// ```
///
/// 期望红：最后一条断言（收尾之后入口仍在 ⇒ `Some("s-live")`，期望 `None`）。
/// 抓不住：页面画不画它（那是 `crates/app-iced` 的 `view/logs/tests/m462.rs`）。
#[test]
fn the_way_back_to_live_exists_only_while_that_session_runs() {
    let (_root, mut state, mut host) = live_in_history("m462-exists");

    host.open_session("s-old");
    drain_into(&mut host, &mut state);
    assert!(state.log_view.in_history(), "夹具前提：进了历史视图");
    assert_eq!(
        state.log_view.live_return(),
        Some(LIVE),
        "有实时会话 + 在历史视图 ⇒ 入口必须在（载荷 = 正在跑那一场）"
    );

    // 被挡住那一场收尾：行还要继续挡（M459），但**回不去了** —— 入口必须消失。
    finished(&mut state, LIVE);
    assert_eq!(
        state.log_view.held(),
        Some(LIVE),
        "收尾后被挡住位照旧留着（尾巴行判据要用）"
    );
    assert_eq!(
        state.log_view.live_return(),
        None,
        "收尾之后不许再摆一个点了没用的入口"
    );
}

/// **判据②**：点下去真的退出历史视图，且下一拍实时行继续进正文（一条命令走完，不是渲染）。
///
/// 变异原文（`host/mod.rs::open_session` 里删掉尾随器那一段，回到「一律走索引」）：
///
/// ```ignore
/// fn open_session(&mut self, id: &str) {
///     let (session, lines, diagnosis) = logs::open(&self.env, id);
/// ```
///
/// 期望红：`!state.log_view.in_history()` 那条（实测仍在历史视图里），以及正文断言
/// （正在跑那一场不在索引里 ⇒ `logs::open` 返回空行，正文被清空）。
/// 抓不住：页面那颗按钮点得到（那是 `crates/app-iced` 的测试）。
#[test]
fn clicking_back_to_live_leaves_history_and_the_next_tick_lands_in_the_body() {
    let (_root, mut state, mut host) = live_in_history("m462-click");
    host.open_session("s-old");
    drain_into(&mut host, &mut state);

    // 用户点了「回到实时」：页面发的就是 `AppCommand::OpenLogSession(<被挡住那一场>)`。
    let id = state
        .log_view
        .live_return()
        .expect("入口在（判据①）")
        .to_string();
    host.open_session(&id);
    let events = drain_into(&mut host, &mut state);

    assert!(
        matches!(events.first(), Some(CoreEvent::LogSessionOpened(session)) if session == LIVE),
        "身份必须先发回正在跑那一场：{events:?}"
    );
    assert!(!state.log_view.in_history(), "点了之后必须真的退出历史视图");
    assert_eq!(
        state.live_session.as_deref(),
        Some(LIVE),
        "实时尾随身份落回"
    );
    assert_eq!(
        state.log_view.live_return(),
        None,
        "入口这时必须消失（否则它变成一个可以一直点的死控件）"
    );

    // 下一拍尾随：新行继续进正文，且**已有的行不重复**（整份重读把偏移推到了末尾）。
    tick(&mut state, LIVE, "[20:02] joining world");
    assert_eq!(
        body(&state),
        ["[20:01] launching 1.20.4", "[20:02] joining world"],
        "回到实时之后，新行必须进正文且不重复"
    );

    // **连点第二下**（同一颗按钮的事件对再来一次）：幂等——不许把视图推回历史、身份仍是实时那一场。
    // 抓得住：切回实时时不把身份寄存回去（第二下会被当成「又打开了一场历史会话」）。
    host.open_session(&id);
    drain_into(&mut host, &mut state);
    assert!(
        !state.log_view.in_history(),
        "连点第二下不许把视图推回历史视图"
    );
    assert_eq!(
        state.live_session.as_deref(),
        Some(LIVE),
        "连点之后身份仍是正在跑那一场"
    );
    tick(&mut state, LIVE, "[20:03] joining the world");
    assert_eq!(
        body(&state)
            .iter()
            .filter(|line| line.as_str() == "[20:01] launching 1.20.4")
            .count(),
        1,
        "连点之后正文里已有的行不许重复：{:?}",
        body(&state)
    );
    assert_eq!(
        body(&state).last().map(String::as_str),
        Some("[20:03] joining the world"),
        "连点之后尾随照旧：{:?}",
        body(&state)
    );
}

/// **判据③**：没有实时会话 ⇒ 入口不存在（点了没用的控件不许出现在界面上）。
///
/// 变异原文（`host/logs_open_tests.rs` 里把这条用例的夹具换成 [`live_in_history`]，即让
/// 「历史 + 实时在跑」成立）：期望红两条 `live_return()` 断言。
///
/// 这里钉的是三种「没有实时会话」：从来没起过 / 在历史视图里但一场都没在跑 / 只有历史。
#[test]
fn without_a_live_session_there_is_no_way_back() {
    // ① 什么都没跑过。
    let mut state = AppState::new();
    assert_eq!(
        state.log_view.live_return(),
        None,
        "一场都没跑过时不许有入口"
    );

    // ② 有历史会话、打开了它，但**没有在跑的那一场**。
    let (env, _root) = fixture("m462-nolive", &[("s-old", "[18:40] boom")]);
    let mut host = CoreHost::headless(env);
    host.open_session("s-old");
    drain_into(&mut host, &mut state);
    assert!(state.log_view.in_history(), "夹具前提：进了历史视图");
    assert_eq!(
        state.log_view.live_return(),
        None,
        "没有在跑的会话 ⇒ 入口不许画（否则就是一个点了没用的控件）"
    );
}
