//! 历史视图不接实时行的守门用例（M459）。
//!
//! 全部走**线上唯一状态入口** `AppState::apply_core`，事件序列与 host 的发出顺序一致：
//! 实时路径是 `LogAppended`（每 `TAIL_POLL_TICKS` 拍一次，见 `host/handle.rs::poll_tail`），
//! 历史路径是 `LogSessionOpened` → `LogLinesChanged` → `DiagnosisReady`（`host/mod.rs::open_session`）。
//!
//! 每条用例的文档写清**抓得住什么**（改哪里会让它红）与**抓不住什么**。

use crate::dto::{
    CoreEvent, Diagnosis, DiagnosisReport, DiagnosisSeverity, LaunchFinished, LiveLogBatch,
    LogLevel, LogLine, LogOutcome, LogSession,
};
use crate::state::AppState;

/// 实时在跑的那一场。
const LIVE: &str = "s-1029-2001";
/// 用户在历史里打开的那一场。
const OLD: &str = "s-1029-1840";
/// 另一场历史会话（切会话用）。
const OLDER: &str = "s-1029-1802";
/// 历史看完之后新开的一局。
const NEXT: &str = "s-1029-2100";

fn line(text: &str) -> LogLine {
    LogLine {
        text: text.to_string(),
        level: LogLevel::Info,
    }
}

fn session(id: &str, when: &str) -> LogSession {
    LogSession {
        id: id.to_string(),
        instance: "开发测试".to_string(),
        when: when.to_string(),
        outcome: LogOutcome::Ok,
        duration_secs: 62,
    }
}

fn batch(session: &str, texts: &[&str], running: bool) -> CoreEvent {
    CoreEvent::LogAppended(LiveLogBatch {
        session: session.to_string(),
        lines: texts.iter().map(|text| line(text)).collect(),
        running,
    })
}

fn report(session: &str, rule: &str) -> CoreEvent {
    CoreEvent::DiagnosisReady(DiagnosisReport {
        session: session.to_string(),
        diagnosis: Diagnosis {
            rule: rule.to_string(),
            cause_key: format!("diagnose.cause.{rule}"),
            severity: DiagnosisSeverity::Critical,
            confidence: 90,
            groups: std::collections::BTreeMap::new(),
            system_text: None,
            fix: None,
        },
    })
}

/// 正文实测：`Vec<LogLine>` → `Vec<String>`（断言里读得懂）。
fn texts(state: &AppState) -> Vec<String> {
    state
        .log_lines
        .iter()
        .map(|line| line.text.clone())
        .collect()
}

/// 一份「实时一场在跑」的状态：`live_session` / `selected_session` 都停在它上面。
fn live_running() -> AppState {
    let mut state = AppState::new();
    state.apply_core(CoreEvent::LogSessionsChanged(vec![
        session(OLD, "10-29 18:40"),
        session(OLDER, "10-29 18:02"),
    ]));
    state.apply_core(batch(LIVE, &["[20:01] launching 1.20.4"], true));
    assert_eq!(state.live_session.as_deref(), Some(LIVE), "夹具前提");
    assert_eq!(state.selected_session.as_deref(), Some(LIVE), "夹具前提");
    state
}

/// 历史路径的两条事件（`host::open_session` 的发出顺序）。
fn open(state: &mut AppState, id: &str, body: &[&str]) {
    state.apply_core(CoreEvent::LogSessionOpened(id.to_string()));
    state.apply_core(CoreEvent::LogLinesChanged(
        body.iter().map(|text| line(text)).collect(),
    ));
}

fn finished(session: &str, outcome: LogOutcome) -> CoreEvent {
    CoreEvent::LaunchFinished(LaunchFinished {
        id: "dev".to_string(),
        session: session.to_string(),
        outcome,
        exit_code: Some(0),
        uptime_ms: 5_000,
        failure: None,
    })
}

/// **复现（M459 要修的 P2 本身）**：实时在跑 → 打开历史会话 → 再推一拍实时行。
///
/// 抓得住：`state/live.rs` 的清行判据在 `live_session == None` 时**什么也不做**，于是
/// `append_live_lines` 照样追加、照样写 `selected_session` ⇒ 历史正文多出实时行、身份跳回 live
/// （实测：`left: ["[18:40] boom: GLFW error", "[20:02] still loading"]` /
/// `left: Some("s-1029-2001")`）。
/// 抓不住：真实盘面（这条喂的是 host 该发的事件，不是 host 自己发的）。
#[test]
fn a_live_tick_does_not_bleed_into_the_history_body() {
    let mut state = live_running();
    open(&mut state, OLD, &["[18:40] boom: GLFW error"]);
    assert_eq!(state.selected_session.as_deref(), Some(OLD), "夹具前提");

    // 下一拍尾随：同一个实时会话，行在追加
    state.apply_core(batch(LIVE, &["[20:02] still loading"], true));

    assert_eq!(
        texts(&state),
        ["[18:40] boom: GLFW error"],
        "历史正文里不许冒出实时行"
    );
    assert_eq!(
        state.selected_session.as_deref(),
        Some(OLD),
        "身份不许跳回 live"
    );
    assert_eq!(state.live_session, None, "实时尾随仍处于「被挡住」状态");
}

/// **边界①**：在历史视图里切回「正在跑那一场」⇒ 实时行继续进正文。
///
/// 抓得住：把「打开的就是正在跑那一场」也当成历史视图（那一页会冻在打开那一刻的快照上，
/// 实时行永远进不来）。
/// 抓不住：页面能不能真点到正在跑那一场（会话索引只在结束时追加 ⇒ 选择器里没有这一行，
/// 见 `pages::logs::sessions` 的模块文档；本用例钉的是 reducer 的判据）。
#[test]
fn opening_the_running_session_returns_to_the_live_tail() {
    let mut state = live_running();
    open(&mut state, OLD, &["[18:40] boom: GLFW error"]);
    state.apply_core(batch(LIVE, &["[20:02] still loading"], true));
    assert_eq!(texts(&state).len(), 1, "夹具前提：历史正文还没被染");

    // 用户回到正在跑的那一场：host 读盘发回它已有的行，随后尾随继续
    open(
        &mut state,
        LIVE,
        &["[20:01] launching 1.20.4", "[20:02] still loading"],
    );
    state.apply_core(batch(LIVE, &["[20:03] joining world"], true));

    assert_eq!(
        texts(&state),
        [
            "[20:01] launching 1.20.4",
            "[20:02] still loading",
            "[20:03] joining world"
        ],
        "切回实时那一场之后，新行必须继续进正文"
    );
    assert_eq!(state.selected_session.as_deref(), Some(LIVE));
    assert_eq!(state.live_session.as_deref(), Some(LIVE));
}

/// **边界②**：历史看完后开始新的一局 ⇒ 正文换成新一局，历史内容不许留。
///
/// 抓得住：退历史视图时不清行（新一局的行接在历史正文后面 = 两场缝在一页）；
/// 也抓得住收尾抢身份（`selected_session` 会被换成刚收尾的 `LIVE`）。
/// 抓不住：新一局的行本身对不对（那是 `state/live.rs` 的既有判据）。
#[test]
fn a_new_run_after_history_replaces_the_history_body() {
    let mut state = live_running();
    open(&mut state, OLD, &["[18:40] boom: GLFW error"]);
    state.apply_core(batch(LIVE, &["[20:02] still loading"], true));

    // 被挡住的那一场收尾（历史视图期间不许抢身份），随后用户点了启动
    state.apply_core(finished(LIVE, LogOutcome::Ok));
    assert_eq!(
        state.selected_session.as_deref(),
        Some(OLD),
        "收尾不许把身份从历史会话上抢走"
    );
    assert_eq!(state.launch_ends, 1, "收尾信号照旧（Toast 靠它）");

    state.apply_core(batch(NEXT, &["[21:00] launching 1.21"], true));

    assert_eq!(
        texts(&state),
        ["[21:00] launching 1.21"],
        "新一局的正文里不许留着历史行"
    );
    assert_eq!(state.selected_session.as_deref(), Some(NEXT));
    assert_eq!(state.live_session.as_deref(), Some(NEXT));
}

/// **边界③**：根本没有实时会话时打开历史 ⇒ 之后真的起一局仍要回到实时。
///
/// 抓得住：把「进历史视图」写成有前置条件（只在有会话在跑时置位）——那一支会漏掉本用例，
/// 新一局的第一批行被并进历史正文；也抓得住「进去出不来」（退视图判据要求「被挡住的那一场」
/// 存在，而没有实时会话时它永远是 `None` ⇒ 永久停在历史视图）。
/// 抓不住：启动器自己起没起来（那是 core 的事）。
#[test]
fn history_without_a_live_session_does_not_lock_the_view() {
    let mut state = AppState::new();
    state.apply_core(CoreEvent::LogSessionsChanged(vec![session(
        OLD,
        "10-29 18:40",
    )]));
    open(&mut state, OLD, &["[18:40] boom: GLFW error"]);
    assert_eq!(state.live_session, None, "夹具前提：没有实时会话");
    assert!(state.log_view.in_history(), "夹具前提：真的进了历史视图");

    state.apply_core(batch(NEXT, &["[21:00] launching 1.21"], true));

    assert_eq!(
        texts(&state),
        ["[21:00] launching 1.21"],
        "起新一局之后正文必须换成实时行（历史内容作废）"
    );
    assert_eq!(state.selected_session.as_deref(), Some(NEXT));
    assert_eq!(state.live_session.as_deref(), Some(NEXT));
}

/// 历史视图里**换另一场历史**之后，实时行仍然被挡住（「被挡住的那一场」不许忘）。
///
/// 抓得住：进历史视图时只从 `live_session`（此刻已是 `None`）取「被挡住的那一场」——
/// 第二次打开历史会话时它被抹成 `None`，下一拍实时行就续写进新历史正文。
/// 抓不住：选择器里点哪一行（那是 `page_messages` 的 `OpenLogSession`）。
#[test]
fn switching_to_another_history_session_keeps_the_live_lines_out() {
    let mut state = live_running();
    open(&mut state, OLD, &["[18:40] boom: GLFW error"]);
    open(&mut state, OLDER, &["[18:02] older session"]);

    state.apply_core(batch(LIVE, &["[20:02] still loading"], true));

    assert_eq!(texts(&state), ["[18:02] older session"]);
    assert_eq!(state.selected_session.as_deref(), Some(OLDER));
}

/// 实时那一场在历史视图期间收尾：它的**尾巴行**（`running=false`）同样不许进历史正文。
///
/// 抓得住：收尾时把「被挡住的那一场」清掉（`Tail` 的最后一读常常晚于收尾，那一批行会直接
/// 落进历史正文）。
/// 抓不住：尾巴行本身有没有落盘（那是 host 的 `Tail`）。
#[test]
fn the_blocked_sessions_last_lines_stay_out_too() {
    let mut state = live_running();
    open(&mut state, OLD, &["[18:40] boom: GLFW error"]);
    state.apply_core(finished(LIVE, LogOutcome::Crashed));
    state.apply_core(batch(LIVE, &["[20:02] bye"], false));

    assert_eq!(texts(&state), ["[18:40] boom: GLFW error"]);
    assert_eq!(state.selected_session.as_deref(), Some(OLD));
}

/// **对照组**（M458 的硬要求：清行判据不许动）。
///
/// 正常启动路径的两条既有行为：同一场连续两拍 ⇒ 只追加不清；换一场在跑 ⇒ 清行。
/// 抓得住：「让 `live_session == None` 也算换了会话」那种改法（第一次 `LogAppended` 就把
/// 刚拼好的行清掉，第一条断言先红）。
/// 抓不住：`MAX_LIVE_LINES` 的封顶（那是滚动窗口的另一条用例）。
#[test]
fn the_clearing_criterion_on_the_normal_start_path_is_untouched() {
    let mut state = AppState::new();
    state.apply_core(batch(LIVE, &["[20:01] a"], true));
    state.apply_core(batch(LIVE, &["[20:02] b"], true));
    assert_eq!(
        texts(&state),
        ["[20:01] a", "[20:02] b"],
        "同一场连续两拍只追加、不清行"
    );

    state.apply_core(batch(NEXT, &["[21:00] c"], true));
    assert_eq!(texts(&state), ["[21:00] c"], "换了一场在跑就该清行");
}

/// 历史视图期间，实时那一场刚收尾的结论**归属**不许跟着变成它。
///
/// 抓得住：`LogSessionOpened` 之后 `live_session` 是空的，页面归属判据落到「报告不属于任何
/// 已知会话」那条兜底（它是给 `ImportCrashLog` 的 `file:` 开的）——实时那场崩溃的结论会被
/// 当成历史会话的结论画出来。页面侧的实际判别见 `crates/app-iced` 的
/// `a_live_finding_does_not_land_on_the_history_page`；本用例钉的是它要的那一位。
/// 抓不住：结论本身的规则命中（那是 core 的规则引擎）。
#[test]
fn the_live_sessions_view_hold_is_visible_to_the_page() {
    let mut state = live_running();
    open(&mut state, OLD, &["[18:40] boom: GLFW error"]);
    state.apply_core(report(LIVE, "gpu.driver_old"));

    assert_eq!(
        state.log_view.held(),
        Some(LIVE),
        "被挡住的那一场必须记着（页面靠它把实时结论挡在历史页外）"
    );
    assert!(state.log_view.in_history(), "历史视图位必须在场");
}

/// `DiagnosisReady` 本身不移动视图归属（结论是另一条通道，不是身份）。
///
/// 抓得住：把「有结论」当成「回了实时视图」（那会让下一拍实时行续写进历史正文）。
/// 抓不住：页面画不画它（那是 `app/view/logs.rs` 的归属判据）。
#[test]
fn a_report_does_not_resume_the_live_view() {
    let mut state = live_running();
    open(&mut state, OLD, &["[18:40] boom: GLFW error"]);
    state.apply_core(report(OLD, "graphics.opengl_not_supported"));
    state.apply_core(batch(LIVE, &["[20:02] still loading"], true));

    assert_eq!(texts(&state), ["[18:40] boom: GLFW error"]);
    assert_eq!(state.selected_session.as_deref(), Some(OLD));
}
