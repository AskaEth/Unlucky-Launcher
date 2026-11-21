//! M462 的页面侧守门（历史视图下「回到实时」那一场）：从 `app/view/logs/tests.rs` 单独成文件。
//!
//! 页面只做一件事：把 presenter 的判据（`AppState::log_view.live_return()`）转成
//! `pages::Logs::back_to_live`——`Some(id)` 就画那颗按钮，`None` 就一个控件都不画。
//! 所以这里的判据是**模型**的两档，加上「点下去之后它自己消失」。
//!
//! 抓不住：按钮画在哪、点了发什么消息（`pages/logs/sessions_tests.rs` 钉消息载荷与卡片源码）、
//! 点了之后状态机怎么走（presenter 的 `host/logs_open_tests.rs` 钉真 seam）。

use super::*;

/// 实时在跑的会话（**不在会话索引里**：索引只在收尾时追加）。
const LIVE: &str = "s-1029-2001";

/// 一份「实时一场在跑」的状态（`live_session` / `selected_session` 都停在它上面）。
fn live_running() -> AppState {
    let mut state = AppState::new();
    state.apply_core(CoreEvent::LogSessionsChanged(vec![session(
        CRASH,
        "10-29 18:40",
        LogOutcome::Crashed,
    )]));
    state.apply_core(CoreEvent::LogAppended(LiveLogBatch {
        session: LIVE.to_string(),
        lines: vec![line("[20:01] launching 1.20.4", LogLevel::Info)],
        running: true,
    }));
    state
}

/// **判据①**：有实时会话 + 在历史视图 ⇒ 模型里有这个入口，载荷是**正在跑那一场**。
///
/// 变异原文（`app/view/logs.rs::logs_model` 里把这一个字段换成常量）：
///
/// ```ignore
/// back_to_live: None,
/// ```
///
/// 期望红：第一条断言（`None` ≠ `Some("s-1029-2001")`）。
/// 抓不住：按钮真的存在（`pages/logs/sessions_tests.rs` 的源码扫描 + 截图）。
#[test]
fn the_page_offers_a_way_back_to_the_running_session() {
    let mut state = live_running();
    state.apply_core(CoreEvent::LogSessionOpened(CRASH.to_string()));
    state.apply_core(CoreEvent::LogLinesChanged(vec![line(
        "[18:40] boom: GLFW error",
        LogLevel::Error,
    )]));

    let logs = logs_model(&state).with_status(Lang::En);
    assert_eq!(
        logs.back_to_live.as_deref(),
        Some(LIVE),
        "历史视图 + 实时在跑 ⇒ 入口必须在，且指向正在跑那一场"
    );
    // 入口与「正文是哪一场」不冲突：正文/高亮仍是用户点开的那一场历史。
    assert_eq!(logs.selected.as_deref(), Some(CRASH));

    // 点下去（host 那一侧就是把上一场整份读回来 + 身份）之后，入口自己消失。
    state.apply_core(CoreEvent::LogSessionOpened(LIVE.to_string()));
    state.apply_core(CoreEvent::LogLinesChanged(vec![line(
        "[20:01] launching 1.20.4",
        LogLevel::Info,
    )]));
    let logs = logs_model(&state).with_status(Lang::En);
    assert_eq!(
        logs.back_to_live, None,
        "回到实时之后入口必须消失（否则它会变成一个可以一直点的死控件）"
    );
    assert_eq!(
        logs.selected.as_deref(),
        Some(LIVE),
        "正文与页头都回到实时那一场"
    );
}

/// **判据③**：没有实时会话 ⇒ 模型里没有这个入口（页面因此一个控件都不画）。
///
/// 变异原文（`presenter::state::log_view::LogView::live_return` 里去掉「仍在跑」那一位）：
///
/// ```ignore
/// if !self.history { return None; }
/// self.held.as_deref()
/// ```
///
/// 期望红：第二条断言（收尾之后入口还在）。
/// 抓不住：入口的像素位置（那是 `DevDocs/artifacts/m462/` 的两张图）。
#[test]
fn without_a_running_session_the_page_offers_nothing() {
    // ① 一场历史、没有在跑的会话：打开它 ⇒ 进历史视图，但没有回实时的路。
    let mut state = AppState::new();
    state.apply_core(CoreEvent::LogSessionsChanged(vec![session(
        CRASH,
        "10-29 18:40",
        LogOutcome::Crashed,
    )]));
    state.apply_core(CoreEvent::LogSessionOpened(CRASH.to_string()));
    state.apply_core(CoreEvent::LogLinesChanged(vec![line(
        "[18:40] boom",
        LogLevel::Error,
    )]));
    assert!(state.log_view.in_history(), "夹具前提：进了历史视图");
    assert_eq!(
        logs_model(&state).with_status(Lang::En).back_to_live,
        None,
        "没有在跑的会话 ⇒ 不许画一个点了没用的控件"
    );

    // ② 实时那一场在历史视图里收尾 ⇒ 入口落下（行还要继续挡，那是 presenter 的事）。
    let mut state = live_running();
    state.apply_core(CoreEvent::LogSessionOpened(CRASH.to_string()));
    state.apply_core(CoreEvent::LogLinesChanged(vec![line(
        "[18:40] boom",
        LogLevel::Error,
    )]));
    assert_eq!(
        logs_model(&state)
            .with_status(Lang::En)
            .back_to_live
            .as_deref(),
        Some(LIVE),
        "夹具前提：在跑时入口在"
    );
    state.apply_core(CoreEvent::LaunchFinished(LaunchFinished {
        id: "dev".to_string(),
        session: LIVE.to_string(),
        outcome: LogOutcome::Crashed,
        exit_code: Some(1),
        uptime_ms: 5_000,
        failure: None,
    }));
    assert_eq!(
        logs_model(&state).with_status(Lang::En).back_to_live,
        None,
        "那一场收尾之后回不去 ⇒ 入口必须消失"
    );
}
