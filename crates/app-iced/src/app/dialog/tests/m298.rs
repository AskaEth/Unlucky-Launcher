//! M298 的收尾失败 / 并发守卫用例（从 `dialog/tests.rs` 拆出：那个文件贴 400 行硬线）。
//!
//! 共用父模块的夹具：`headless()`（注入式 `App`）、`report()`（一份现场）、`slot_guard()`
//! （`crash_report` 的现场是进程级单槽，凡 publish 的用例都要拿它）。

use std::path::PathBuf;

use unlucky_core::launch::crash_report::{self, CrashReport, ReportContext, SessionAttribution};
use unlucky_presenter::dto::{LaunchFailureStage, LaunchFailureView, LaunchFinished, LogOutcome};

use super::{headless, report, slot_guard};

/// 一次启动的现场上下文（与 presenter 里那份同形：实例 / 运行目录 / 日志目录）。
fn context() -> ReportContext {
    ReportContext::new(
        "survival",
        "生存",
        "1.20.4",
        std::path::Path::new("/instances/survival"),
        std::path::Path::new("/logs"),
        std::time::SystemTime::now(),
        "os=windows",
    )
}

// ------------------------------------------------------------------ M298：收尾失败必须弹

/// **真收尾失败必须弹出**（M298 修的那条静默漏弹）。
///
/// 出口形状取自 `crates/presenter/src/host/launch.rs` 的 `finish()` 失败分支，逐字段对齐：
/// 进程**起来过**（有会话）、`finish()` 报错 ⇒ `outcome = SilentFail` + `failure = Some(Spawn)`。
/// 现场由 core 的 `CrashReport::before_start` 登记，会话归因 = 收尾事件里那个会话。
///
/// ⚠️ 这一条出口**拿不到退出码**：`finish()` 只在「等进程出错」或「看门狗放弃等待」时失败，
/// 两种都没有 `ExitStatus`（`pending_ms` 那条路径进程还在跑）。所以「exit_code 有值」在本出口上
/// 不成立，判据改用「报告带会话 + 进程起来过」；带退出码的那条出口见下一条用例。
///
/// **变异证明**：把 `CrashReport::before_start` 的 `session` 改回写死空串（本 mission 之前的
/// 写法），`open_failure_dialog` 的并发守卫成立并 `return` ⇒ 本用例第一段立刻红。
#[test]
fn a_finish_failure_pops_the_dialog_with_its_own_session() {
    let _slot = slot_guard();
    let mut app = headless();
    let log = PathBuf::from("/sessions/game/9f2c.jsonl");
    crash_report::publish(CrashReport::before_start(
        &context(),
        "spawn",
        "会话写入器不可用（锁中毒）",
        "spawn",
        Some(SessionAttribution {
            id: "9f2c",
            log: Some(log.as_path()),
        }),
    ));
    let finished = LaunchFinished {
        id: "survival".to_string(),
        session: "9f2c".to_string(),
        outcome: LogOutcome::SilentFail,
        exit_code: None,
        uptime_ms: 0,
        failure: Some(LaunchFailureView::new(
            LaunchFailureStage::Spawn,
            "会话写入器不可用（锁中毒）",
        )),
    };
    app.open_failure_dialog(&finished);
    assert!(
        app.state.dialogs.current().is_some(),
        "真收尾失败必须弹框——用户原话是「找半天没找到崩溃日志」"
    );
    let card = app.dialog_card.as_ref().expect("卡片");
    assert_eq!(
        card.report.session, finished.session,
        "弹的是**这一场**的现场"
    );
    // 现场带着会话 ⇒ 事件流路径必须出现在可复制的路径行里（不是「进程没起来」那一套）。
    let modal = app.dialog_modal().expect("模型");
    let paths = modal.card.as_ref().expect("卡片视图").paths.clone();
    assert!(
        paths.iter().any(|(_, path)| path.ends_with("9f2c.jsonl")),
        "会话 JSONL 必须可复制：{paths:?}"
    );
    crash_report::clear();
}

/// 带**退出码**的那条出口（真崩溃）照旧要弹：报告的 `session` 与收尾事件同源。
///
/// **变异证明**：把 `CrashReport::collect` 里的 `session: session.session_id.clone()` 改成空串，
/// 本用例红（并发守卫把"自己这一场"判成别人的）。
#[test]
fn a_crashed_launch_with_an_exit_code_pops_with_its_evidence() {
    let _slot = slot_guard();
    let mut app = headless();
    crash_report::publish(report(true, vec!["Exception in thread main".to_string()]));
    let finished = LaunchFinished {
        id: "survival".to_string(),
        session: "9f2c".to_string(),
        outcome: LogOutcome::Crashed,
        exit_code: Some(1),
        uptime_ms: 1_000,
        failure: None,
    };
    app.open_failure_dialog(&finished);
    assert!(app.state.dialogs.current().is_some(), "真崩溃必须弹框");
    let modal = app.dialog_modal().expect("模型");
    assert!(
        modal.body_text().contains("退出码 1"),
        "正文必须带退出码：{}",
        modal.body_text()
    );
    crash_report::clear();
}

/// **并发守卫的原意不许被松掉**：登记的是另一场 ⇒ 不许拿它解释这一场。
///
/// 这条与上一条是一对：修「自己这一场被误判」不能靠删守卫（删了这条就红）。
#[test]
fn a_report_from_another_session_is_never_reused() {
    let _slot = slot_guard();
    let mut app = headless();
    crash_report::publish(CrashReport::before_start(
        &context(),
        "spawn",
        "另一场的现场",
        "spawn",
        Some(SessionAttribution {
            id: "aaaa",
            log: None,
        }),
    ));
    let finished = LaunchFinished {
        id: "survival".to_string(),
        session: "9f2c".to_string(),
        outcome: LogOutcome::SilentFail,
        exit_code: None,
        uptime_ms: 0,
        failure: Some(LaunchFailureView::new(LaunchFailureStage::Spawn, "x")),
    };
    app.open_failure_dialog(&finished);
    assert!(
        app.state.dialogs.current().is_none(),
        "A 场的现场不许拿来解释 B 场"
    );
    crash_report::clear();
}

/// 没有会话的两个出口（装配失败 / spawn 失败）**照旧要弹**：那里空串是事实，守卫不该拦它。
///
/// 抓得住：把「没会话」也编一个 id 出来（那会让装配失败的现场指向一个不存在的会话）；
/// 也抓得住这条出口被新判据误伤（例如把「session 为空」当成「必须不弹」）。
///
/// **抓不住「把并发守卫整条删掉」**——复审的变异 C 实测：删掉守卫后本用例照样绿（它本来就
/// 走 session 为空的路径）。那条守卫由 `a_report_from_another_session_is_never_reused` 守。
#[test]
fn a_session_less_failure_still_pops() {
    let _slot = slot_guard();
    for (stage, detail) in [
        (LaunchFailureStage::VersionManifest, "缺 1.20.4.json"),
        (LaunchFailureStage::Spawn, "javaw.exe 不存在"),
    ] {
        let mut app = headless();
        crash_report::publish(CrashReport::before_start(
            &context(),
            stage.slug(),
            detail,
            stage.slug(),
            None,
        ));
        let finished = LaunchFinished {
            id: "survival".to_string(),
            session: String::new(),
            outcome: LogOutcome::Crashed,
            exit_code: None,
            uptime_ms: 0,
            failure: Some(LaunchFailureView::new(stage, detail)),
        };
        app.open_failure_dialog(&finished);
        assert!(
            app.state.dialogs.current().is_some(),
            "{stage:?} 出口必须弹框（这条出口的 session 为空是事实）"
        );
        let card = app.dialog_card.as_ref().expect("卡片");
        assert!(card.report.session.is_empty(), "{stage:?}");
        assert_eq!(
            card.report.exit_code, None,
            "{stage:?}: 没有退出码就是 None"
        );
        assert_eq!(
            card.report.stage.as_deref(),
            Some(stage.slug()),
            "{stage:?}"
        );
        crash_report::clear();
    }
}
