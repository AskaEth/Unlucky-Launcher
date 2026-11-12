//! 日志页展示转换的守门测试（从 `app/view/logs.rs` 拆出：那边已经 382 行，贴着 400 硬上限）。
//!
//! 全部走**真实 reducer 路径**（`AppState::apply_core`），不造注入源——r3 的教训是
//! 「测试全走注入源而真实路径零覆盖」，所以这里的夹具就是线上那串事件本身。

use unlucky_i18n::Lang;
use unlucky_presenter::dto::{
    CoreEvent, DiagnosisReport, LaunchFinished, LiveLogBatch, LogLevel, LogLine, LogOutcome,
    LogSession,
};
use unlucky_presenter::state::AppState;

use crate::pages;

use super::logs_model;

/// 崩溃的那一场。
const CRASH: &str = "s-1029-1840";
/// 崩溃之后又启动的、正常跑的一场。
const OK_RUN: &str = "s-1029-1922";

fn session(id: &str, when: &str, outcome: LogOutcome) -> LogSession {
    LogSession {
        id: id.to_string(),
        instance: "开发测试".to_string(),
        when: when.to_string(),
        outcome,
        duration_secs: 62,
    }
}

fn line(text: &str, level: LogLevel) -> LogLine {
    LogLine {
        text: text.to_string(),
        level,
    }
}

fn report(session: &str, rule: &str) -> DiagnosisReport {
    DiagnosisReport {
        session: session.to_string(),
        diagnosis: pages_dto(rule),
    }
}

fn pages_dto(rule: &str) -> unlucky_presenter::dto::Diagnosis {
    unlucky_presenter::dto::Diagnosis {
        rule: rule.to_string(),
        title: "显卡驱动过旧（OpenGL 版本不足）".to_string(),
        confidence: 92,
        hint: "升级显卡驱动".to_string(),
        fix: Some("change_graphics.driver".to_string()),
    }
}

/// **真实数据路径**（复审要求：不许造注入源绕过去）：
/// 崩溃一场跑完 → 再启动一场正常跑。`state.diagnosis` 在这里**故意不清**，
/// 因为 presenter 里根本没有清零点——这就是线上发生串场的条件。
fn crashed_only() -> AppState {
    let mut state = AppState::new();
    state.apply_core(CoreEvent::LogSessionsChanged(vec![
        session(CRASH, "10-29 18:40", LogOutcome::Crashed),
        session(OK_RUN, "10-29 19:22", LogOutcome::Ok),
    ]));

    // 第一场跑起来（live_session = CRASH）→ 日志 → 规则结论 → 收尾（selected_session = CRASH）
    state.apply_core(CoreEvent::LogAppended(LiveLogBatch {
        session: CRASH.to_string(),
        lines: vec![line("[18:40] boom: GLFW error", LogLevel::Error)],
        running: true,
    }));
    state.apply_core(CoreEvent::DiagnosisReady(report(CRASH, "gpu.driver_old")));
    state.apply_core(CoreEvent::LaunchFinished(LaunchFinished {
        id: "dev".to_string(),
        session: CRASH.to_string(),
        outcome: LogOutcome::Crashed,
        exit_code: Some(1),
        uptime_ms: 3_000,
        failure: None,
    }));

    state
}

/// 崩溃之后又启动一场正常跑：`live_session` 前进，`diagnosis` 仍是上一场的。
fn after_a_new_run() -> AppState {
    let mut state = crashed_only();
    state.apply_core(CoreEvent::LogAppended(LiveLogBatch {
        session: OK_RUN.to_string(),
        lines: vec![line("[19:22] launching 1.20.4", LogLevel::Info)],
        running: true,
    }));
    state
}

#[test]
fn a_new_run_does_not_inherit_the_previous_crash() {
    let state = after_a_new_run();
    let logs = logs_model(&state).with_status(Lang::En);

    // 抓得住：身份被上一场的 `diagnosis.session` 顶掉（r2 的 bug）——那时 outcome 会是
    //          Crashed、findings 挂着 gpu.driver_old、摘要里 `lines=` 与结论不同源。
    // 抓不住：presenter 自己 `clear_logs` 后 `selected_session` 不清的缺口（已报 finding），
    //          以及多场并行会话（本仓一次只跑一场）。
    assert_eq!(
        logs.outcome,
        Some(pages::Outcome::Ok),
        "身份应该是正在跑的这一场"
    );

    let status = logs.status.clone().expect("有会话身份就该有副标题");
    assert!(status.contains("Finished normally"), "{status}");
    assert!(
        !status.contains("The game crashed"),
        "页头把新会话标成了上一场的崩溃：{status}"
    );

    assert!(
        logs.findings.is_empty(),
        "上一场的结论不能挂到这一场：{:?}",
        logs.findings
    );

    // 摘要必须与身份同源：outcome 是 ok、不含旧规则、行数就是当前这些行。
    let summary = logs.summary();
    assert!(summary.contains("outcome=ok"), "{summary}");
    assert!(!summary.contains("gpu.driver_old"), "{summary}");
    assert!(
        summary.contains(&format!("lines={}", logs.lines.len())),
        "{summary}"
    );
    assert!(!summary.contains("findings="), "{summary}");
}

#[test]
fn the_crash_itself_is_still_reported() {
    // 上一条测试的**对照**：崩溃当场必须画结论，否则那条断言「findings 为空」是空的。
    // 抓得住：把判别写成「永远不画诊断」（用空断言骗过 P2）。
    // 抓不住：结论排序/去重（那是 core 的规则引擎的事）。
    let state = crashed_only();
    let logs = logs_model(&state).with_status(Lang::En);

    assert_eq!(logs.outcome, Some(pages::Outcome::Crashed));
    assert_eq!(logs.findings.len(), 1, "{:?}", logs.findings);
    assert_eq!(logs.findings[0].rule, "gpu.driver_old");
    let status = logs.status.clone().unwrap_or_default();
    assert!(status.contains("The game crashed"), "{status}");
    assert!(logs.summary().contains("gpu.driver_old:high(92)"));
}

#[test]
fn an_imported_crash_file_still_shows_its_findings() {
    // r3 的**回归**：r3 把归属判据写成「有选中就只认选中」，于是导入崩溃日志
    // （`ImportCrashLog` → `DiagnosisReady` 带 `file:<文件名>` 的会话 id，且**不发**
    // `LogLinesChanged`）被整条丢掉。今天 iced 侧还没有 dispatch 站点（那些还在遗留的
    // Slint shell 里），所以复审没判 p1；这条测试是为了**将来接线的人**不再踩。
    // 抓得住：只有「属于当前选中/live 会话」才画的窄判据（导入的 `file:` 会话不等于任何
    //          索引里的会话，会被误杀）。
    // 抓不住：导入后日志正文该显示什么（`ImportCrashLog` 不载入日志行，那是 presenter 的事）。
    let mut state = AppState::new();
    state.apply_core(CoreEvent::LogSessionsChanged(vec![session(
        CRASH,
        "10-29 18:40",
        LogOutcome::Crashed,
    )]));
    // 上一场刚结束 ⇒ selected_session = CRASH（过期选中项），随后用户导入一份崩溃文件
    state.apply_core(CoreEvent::LaunchFinished(LaunchFinished {
        id: "dev".to_string(),
        session: CRASH.to_string(),
        outcome: LogOutcome::Crashed,
        exit_code: Some(1),
        uptime_ms: 3_000,
        failure: None,
    }));
    state.apply_core(CoreEvent::DiagnosisReady(report(
        "file:crash-2026-11-02_19.40.15-client.txt",
        "integrity.unsatisfied_link",
    )));

    let logs = logs_model(&state).with_status(Lang::En);
    assert_eq!(
        logs.findings.len(),
        1,
        "导入的崩溃日志必须画出它的结论：{:?}",
        logs.findings
    );
    assert_eq!(logs.findings[0].rule, "integrity.unsatisfied_link");
}

#[test]
fn a_live_run_says_it_is_running_instead_of_nothing_runs() {
    // 视觉复审 r3 的 P2-3：session 索引只在**结束**时追加（`host::logs::sessions`），
    // 所以运行中的那一场永远不在 `log_sessions` 里 ⇒ 拿不到实例名与结局 ⇒ r3 的页头在
    // 每次实时运行都说「当前没有运行中的实例」。demo 伪造了身份，截图上根本照不出来。
    // 抓得住：页头退回 Idle 文案（这次走真实 reducer：只有 `LogAppended(running=true)`）。
    // 抓不住：文案长度在某尺寸下会不会撞右侧动作区（那要看截图）。
    let mut state = AppState::new();
    state.apply_core(CoreEvent::LogSessionsChanged(vec![session(
        CRASH,
        "10-29 18:40",
        LogOutcome::Crashed,
    )]));
    state.apply_core(CoreEvent::LogAppended(LiveLogBatch {
        session: OK_RUN.to_string(),
        lines: vec![line("[19:22] launching", LogLevel::Info)],
        running: true,
    }));

    let logs = logs_model(&state).with_status(Lang::En);
    assert!(logs.live, "live_session 非空就该标记为运行中");
    let status = logs.status.clone().expect("运行中必须有副标题");
    let idle = unlucky_i18n::catalog::Catalog::of(Lang::En)
        .text(unlucky_i18n::catalog::Key::LogsSubtitleIdle);
    assert_ne!(status, idle, "运行中的页头不能说「没有运行」");
    assert!(status.contains("Running now"), "{status}");
}

#[test]
fn the_hosts_no_rule_matched_placeholder_is_not_a_finding() {
    // host 的 `open_session` 在规则引擎无结论时发一份 `rule: ""` 的假报告（非 Option 逼的）。
    // 抓得住：把「没命中」画成一条严重度结论（用户会去修一个不存在的毛病）。
    // 抓不住：host 该不该改发 `None`（那是 presenter 的 finding）。
    let mut state = AppState::new();
    state.apply_core(CoreEvent::LogSessionsChanged(vec![session(
        CRASH,
        "10-29 18:40",
        LogOutcome::Crashed,
    )]));
    state.apply_core(CoreEvent::LogLinesChanged(vec![line(
        "[18:40] nothing to see",
        LogLevel::Info,
    )]));

    state.apply_core(CoreEvent::DiagnosisReady(report(CRASH, "")));
    assert!(
        logs_model(&state).findings.is_empty(),
        "空 rule 是『没命中』的占位，不是结论"
    );

    state.apply_core(CoreEvent::DiagnosisReady(report(CRASH, "gpu.driver_old")));
    assert_eq!(
        logs_model(&state).findings.len(),
        1,
        "换成真规则就得画出来（证明上面的过滤不是把诊断一起关了）"
    );
}
