//! 现场事实的断言：三态证据分类、脱敏、缺件写明、槽位读写。

use std::path::PathBuf;

use super::{clear, failing_launch, publish, ArtifactFact, CrashReport, Evidence, ReportContext};

fn report(artifacts: Vec<ArtifactFact>, tail: Vec<String>) -> CrashReport {
    CrashReport {
        instance: "survival".into(),
        name: "生存".into(),
        version: "1.20.4".into(),
        session: "0f8fad5b".into(),
        session_log: Some(PathBuf::from("/sessions/game/0f8fad5b.jsonl")),
        version_json: None,
        launcher_logs: Some(PathBuf::from("/.ULL/logs")),
        run_dir: Some(PathBuf::from("/instances/survival")),
        artifacts,
        exit_code: Some(1),
        exit_type: "application_error".into(),
        uptime_ms: 12_000,
        command: "javaw -Xmx4096m".into(),
        java: "javaw.exe (21)".into(),
        system: "os=windows".into(),
        stage: None,
        stage_detail: None,
        stderr_tail: tail,
    }
}

#[test]
fn evidence_tells_the_three_shapes_apart() {
    let with_report = report(
        vec![ArtifactFact {
            source: "crash_report".into(),
            path: PathBuf::from("/instances/survival/crash-reports/crash-1.txt"),
            excerpt: Some("Description: Ticking entity".into()),
        }],
        vec!["Exception in thread main".into()],
    );
    assert_eq!(with_report.evidence(), Evidence::GameCrashReport);
    assert_eq!(with_report.evidence().slug(), "game_crash_report");

    let stderr_only = report(Vec::new(), vec!["Error: could not find java.dll".into()]);
    assert_eq!(
        stderr_only.evidence(),
        Evidence::CapturedOutput,
        "没有 crash-report 但有捕获输出 ⇒ 不许说「找不到日志」"
    );

    let nothing = report(Vec::new(), Vec::new());
    assert_eq!(nothing.evidence(), Evidence::Nothing);

    // 三种 slug 必须互不相同（界面按它选 key，撞了就会显示错的一句）。
    let slugs = [
        with_report.evidence().slug(),
        stderr_only.evidence().slug(),
        nothing.evidence().slug(),
    ];
    let mut unique = slugs.to_vec();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 3, "{slugs:?}");
}

#[test]
fn missing_crash_report_is_listed_not_silently_dropped() {
    let report = report(Vec::new(), vec!["line".into()]);
    let missing = report.missing_lines().join("\n");
    assert!(missing.contains("crash-reports"), "{missing}");
    assert!(missing.contains("游戏没来得及写"), "{missing}");
    assert!(
        !missing.contains("会话 JSONL"),
        "会话在，不该说没会话：{missing}"
    );
}

#[test]
fn notes_name_the_fallback_reason_classification() {
    let report = report(Vec::new(), Vec::new());
    let notes = report.note_lines().join("\n");
    assert!(notes.contains("缺 Java"), "{notes}");
    assert!(notes.contains("内存不足"), "{notes}");
}

#[test]
fn before_start_marks_the_launch_as_never_started_and_scrubs_the_detail() {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    if home.is_empty() {
        return;
    }
    let ctx = ReportContext::new(
        "survival",
        "生存",
        "1.20.4",
        std::path::Path::new("/instances/survival"),
        std::path::Path::new("/.ULL/logs"),
        std::time::SystemTime::now(),
        "os=windows",
    );
    let report = CrashReport::before_start(
        &ctx,
        "spawn",
        &format!("{home}/java/bin/javaw.exe 不存在"),
        "spawn",
    );
    assert!(!report.started(), "进程没起来");
    assert_eq!(report.evidence(), Evidence::Nothing);
    let detail = report.stage_detail.expect("阶段原文");
    assert!(detail.contains("<home>"), "{detail}");
    assert!(!detail.contains(&home), "主目录原文漏了：{detail}");
}

#[test]
fn slot_keeps_the_last_failure_only() {
    clear();
    assert!(failing_launch().is_none());
    publish(report(Vec::new(), vec!["first".into()]));
    publish(report(Vec::new(), vec!["second".into()]));
    let last = failing_launch().expect("最近一次");
    assert_eq!(last.stderr_tail, vec!["second".to_string()]);
    clear();
    assert!(failing_launch().is_none());
}
