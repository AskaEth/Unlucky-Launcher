//! M459 的页面侧守门（历史视图不接实时行）：从 `logs/tests.rs` 拆出，两份都守着 400 行硬线。
//!
//! 事件序列与线上同源：`host::open_session`（身份 → 行）与 `poll_tail` 的 `LogAppended`。
//! 每条用例写清**抓得住什么**与**抓不住什么**。

use super::*;

/// M459 的页面侧：实时在跑时打开历史会话，**下一拍实时行不进页面**，页头/高亮留在历史那一场。
///
/// 事件序列就是线上那两拍：`host::open_session`（身份 → 行）与 `poll_tail` 的
/// `LogAppended`（每 `TAIL_POLL_TICKS` 一次）。
///
/// 抓得住：presenter 的清行判据在 `live_session == None` 时不动手（M458 之前的实况）——
/// 那时 `logs.lines` 多出实时行、`logs.selected` 跳回 `OK_RUN`、页头也写成「正在运行」；
/// 抓不住：真机上那两拍之间的时序（这条喂的是事件，不是真盘）。
#[test]
fn a_live_tick_does_not_bleed_into_the_history_page() {
    let mut state = AppState::new();
    state.apply_core(CoreEvent::LogSessionsChanged(vec![
        session(CRASH, "10-29 18:40", LogOutcome::Crashed),
        session(OK_RUN, "10-29 19:22", LogOutcome::Ok),
    ]));
    // 实时一场在跑（live_session = OK_RUN），用户点开 CRASH 那一场
    state.apply_core(CoreEvent::LogAppended(LiveLogBatch {
        session: OK_RUN.to_string(),
        lines: vec![line("[19:22] launching 1.20.4", LogLevel::Info)],
        running: true,
    }));
    state.apply_core(CoreEvent::LogSessionOpened(CRASH.to_string()));
    state.apply_core(CoreEvent::LogLinesChanged(vec![line(
        "[18:40] boom: GLFW error",
        LogLevel::Error,
    )]));

    // 下一拍尾随
    state.apply_core(CoreEvent::LogAppended(LiveLogBatch {
        session: OK_RUN.to_string(),
        lines: vec![line("[19:23] joining world", LogLevel::Info)],
        running: true,
    }));

    let logs = logs_model(&state).with_status(Lang::En);
    let texts: Vec<&str> = logs.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(
        texts,
        ["[18:40] boom: GLFW error"],
        "历史页的正文里不许冒出实时行"
    );
    assert_eq!(
        logs.selected.as_deref(),
        Some(CRASH),
        "高亮不许跳回实时那一场"
    );
    assert_eq!(
        logs.outcome,
        Some(pages::Outcome::Crashed),
        "页头写的还是这一场历史"
    );
    assert!(!logs.live, "历史视图期间页头不写「正在运行」");
}

/// M459 的页面侧（结论）：实时那一场在历史视图期间崩溃，结论**不许**挂到历史页上。
///
/// 抓得住：`LogSessionOpened` 之后 `live_session` 是空的，归属判据落到「报告不属于任何已知
/// 会话」那条兜底（给 `ImportCrashLog` 的 `file:` 开的）⇒ 历史页画出实时那一场的崩溃结论；
/// 抓不住：presenter 为什么保留这份报告（`state.diagnosis` 只有一格，那是 presenter 的事）。
#[test]
fn a_live_finding_does_not_land_on_the_history_page() {
    let mut state = AppState::new();
    state.apply_core(CoreEvent::LogSessionsChanged(vec![session(
        CRASH,
        "10-29 18:40",
        LogOutcome::Crashed,
    )]));
    state.apply_core(CoreEvent::LogAppended(LiveLogBatch {
        session: OK_RUN.to_string(),
        lines: vec![line("[19:22] launching 1.20.4", LogLevel::Info)],
        running: true,
    }));
    state.apply_core(CoreEvent::LogSessionOpened(CRASH.to_string()));
    state.apply_core(CoreEvent::LogLinesChanged(vec![line(
        "[18:40] boom: GLFW error",
        LogLevel::Error,
    )]));
    // 被挡住的那一场崩了（`OK_RUN` 还没进索引：会话索引只在结束时追加）
    state.apply_core(CoreEvent::DiagnosisReady(report(OK_RUN, "gpu.driver_old")));

    let logs = logs_model(&state).with_status(Lang::En);
    assert!(
        logs.findings.is_empty(),
        "实时那一场的结论不许挂到历史页上：{:?}",
        logs.findings
    );
    assert_eq!(logs.selected.as_deref(), Some(CRASH));

    // 对照：同一份「选中 + 无 live」但不在历史视图里时，它的结论照旧算数（证明上面的过滤
    // 不是把诊断一起关了）。
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
    state.apply_core(CoreEvent::DiagnosisReady(report(CRASH, "gpu.driver_old")));
    assert_eq!(
        logs_model(&state).findings.len(),
        1,
        "历史那一场自己的结论必须照画"
    );
}
