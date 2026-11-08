//! 退出分析测试：退出码语义 + `exit 0` 静默失败（`analyse/02` §6.3 §7.2 §7.10-10）。

use crate::prelude::*;

fn line(stream: Stream, text: &str) -> LogLine {
    LogLine::new(stream, text)
}

fn analyze(
    code: i32,
    uptime_ms: u64,
    lines: &[LogLine],
    posix: bool,
) -> unlucky_core::launch::exit_analysis::ExitAnalysis {
    let mut signals = exit_analysis::ExitSignals::new(code, uptime_ms, lines);
    signals.is_posix = posix;
    exit_analysis::analyze(&signals)
}

#[test]
fn clean_long_run_is_normal() {
    let lines = vec![line(
        Stream::Stdout,
        "[12:00:00] [Render thread/INFO]: started",
    )];
    let analysis = analyze(0, 120_000, &lines, true);
    assert_eq!(analysis.exit_type, ExitType::Normal);
    assert!(analysis.silent_failure.is_none());
    assert!(!analysis.needs_diagnosis);
}

#[test]
fn exit_zero_short_lived_is_silent_failure() {
    let lines = vec![line(Stream::Stdout, "Stopping!")];
    let analysis = analyze(0, 8_000, &lines, true);
    assert_eq!(analysis.exit_type, ExitType::Normal);
    let silent = analysis.silent_failure.expect("应标记静默失败");
    assert!(silent.short_lived);
    assert!(!silent.error_output);
    assert!(analysis.needs_diagnosis, "静默失败必须进诊断流水线");
}

#[test]
fn exit_zero_with_error_output_is_silent_failure() {
    let lines = vec![
        line(Stream::Stdout, "[12:00:00] [main/INFO]: loading"),
        line(
            Stream::Stderr,
            "java.lang.RuntimeException: mixin apply failed",
        ),
    ];
    let analysis = analyze(0, 90_000, &lines, true);
    assert_eq!(analysis.exit_type, ExitType::Normal);
    let silent = analysis.silent_failure.expect("应标记静默失败");
    assert!(silent.error_output);
    assert!(!silent.short_lived);
    assert!(silent.describe().contains("错误特征"));
    assert_eq!(analysis.error_lines, 1);
}

#[test]
fn jvm_keywords_take_precedence() {
    let lines = vec![line(
        Stream::Stderr,
        "Error occurred during initialization of VM",
    )];
    let analysis = analyze(1, 500, &lines, true);
    assert_eq!(analysis.exit_type, ExitType::JvmError);
    assert_eq!(analysis.jvm_keywords.len(), 1);
    assert!(analysis.needs_diagnosis);
}

#[test]
fn nonzero_without_keywords_is_application_error() {
    let lines = vec![line(Stream::Stdout, "[12:00:00] [main/INFO]: hi")];
    let analysis = analyze(1, 40_000, &lines, true);
    assert_eq!(analysis.exit_type, ExitType::ApplicationError);
    assert!(analysis.needs_diagnosis);
}

#[test]
fn crash_keywords_promote_exit_zero() {
    let lines = vec![line(
        Stream::Stdout,
        "[12:00:00] [Render thread/ERROR]: Crash report saved to ./crash-reports/crash-1.txt",
    )];
    let analysis = analyze(0, 60_000, &lines, true);
    assert_eq!(analysis.exit_type, ExitType::ApplicationError);
    assert_eq!(analysis.crash_keywords.len(), 1);
    assert!(
        analysis.silent_failure.is_none(),
        "已判异常就不再是静默失败"
    );
}

#[test]
fn posix_signals_are_recognized() {
    assert_eq!(analyze(137, 60_000, &[], true).exit_type, ExitType::SigKill);
    assert_eq!(analyze(143, 60_000, &[], true).exit_type, ExitType::SigTerm);
    // Windows 上 137 没有信号语义，按应用错误处理。
    assert_eq!(
        analyze(137, 60_000, &[], false).exit_type,
        ExitType::ApplicationError
    );
}

#[test]
fn windows_ntstatus_is_explained() {
    let analysis = analyze(-1_073_741_819, 60_000, &[], false);
    assert_eq!(analysis.exit_type, ExitType::ApplicationError);
    assert_eq!(
        analysis.windows_status.as_deref(),
        Some("0xC0000005 ACCESS_VIOLATION")
    );

    let dll = analyze(-1_073_741_515, 60_000, &[], false);
    assert_eq!(
        dll.windows_status.as_deref(),
        Some("0xC0000135 DLL_NOT_FOUND")
    );

    // 0xC000013A = STATUS_CONTROL_C_EXIT（控制台 Ctrl+C / 关窗口），十进制 -1073741510。
    let control_c = analyze(-1_073_741_510, 60_000, &[], false);
    assert_eq!(
        control_c.windows_status.as_deref(),
        Some("0xC000013A CONTROL_C_EXIT")
    );
    // 回归：-1073741743 其实是 0xC0000051，不得被当成 CONTROL_C_EXIT。
    assert!(analyze(-1_073_741_743, 60_000, &[], false)
        .windows_status
        .is_none());
}

#[test]
fn killed_and_interrupted_override_exit_code() {
    let mut signals = exit_analysis::ExitSignals::new(1, 5_000, &[]);
    signals.killed = true;
    assert_eq!(exit_analysis::analyze(&signals).exit_type, ExitType::Killed);

    let mut signals = exit_analysis::ExitSignals::new(1, 5_000, &[]);
    signals.interrupted = true;
    assert_eq!(
        exit_analysis::analyze(&signals).exit_type,
        ExitType::Interrupted
    );
}

#[test]
fn exit_type_strings_match_contract() {
    // 与 M15 刻意保持一致的字符串集合（`launch-接口约定.md` §3）。
    assert_eq!(ExitType::Normal.as_str(), "normal");
    assert_eq!(ExitType::ApplicationError.as_str(), "application_error");
    assert_eq!(ExitType::JvmError.as_str(), "jvm_error");
    assert_eq!(ExitType::SigKill.as_str(), "sigkill");
    assert_eq!(ExitType::SigTerm.as_str(), "sigterm");
    assert_eq!(ExitType::Killed.as_str(), "killed");
    assert_eq!(ExitType::Interrupted.as_str(), "interrupted");
    assert!(!ExitType::Normal.is_abnormal());
    assert!(ExitType::JvmError.is_abnormal());
}

#[test]
fn summary_is_human_readable() {
    let analysis = analyze(-1_073_741_819, 61_500, &[], false);
    let summary = analysis.summary();
    assert!(summary.contains("-1073741819"));
    assert!(summary.contains("61.5s"));
    assert!(summary.contains("ACCESS_VIOLATION"));
}
