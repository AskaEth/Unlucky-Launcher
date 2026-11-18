//! 五类信号采集：环形缓冲语义 / 退出码分类 / 磁盘采集器 / 引擎输入拼装。

use std::fs;
use std::time::{Duration, SystemTime};

use crate::prelude::*;

#[test]
fn log_buffer_keeps_key_lines_when_evicted() {
    let mut buffer = LogBuffer::new(2);
    buffer.push(SignalOrigin::Stdout, "java.lang.Exception: boom");
    buffer.push(SignalOrigin::Stdout, "line 2");
    buffer.push(SignalOrigin::Stdout, "line 3");
    buffer.push(SignalOrigin::Stderr, "line 4");

    assert_eq!(buffer.pushed(), 4);
    assert_eq!(buffer.dropped(), 2);
    assert_eq!(buffer.kept_key_lines(), 1, "关键行被淘汰后仍应保留");
    assert_eq!(buffer.len(), 3, "保留 1 行关键历史 + 环形区 2 行");
    let text = buffer.text();
    assert!(text.contains("boom"), "根因行不该被挤出缓冲区");
    assert!(text.contains("line 4"));
    assert!(!text.contains("line 2"));
    assert!(!buffer.is_empty());
}

#[test]
fn exit_types_match_launch_contract() {
    // 字符串值与 launch-接口约定.md §3 exit.exit_type 完全一致
    let kill = ExitSignal::classify(Some(137), Some(60_000), 0, "");
    assert_eq!(kill.exit_type, ExitType::Sigkill);
    assert_eq!(kill.exit_type.as_str(), "sigkill");
    assert_eq!(ExitType::from_name("sigkill"), ExitType::Sigkill);

    let term = ExitSignal::classify(Some(143), Some(60_000), 1, "");
    assert_eq!(term.exit_type, ExitType::Sigterm);

    let jvm = ExitSignal::classify(
        Some(1),
        Some(800),
        2,
        "Error occurred during initialization of VM\nCould not create the Java Virtual Machine",
    );
    assert_eq!(jvm.exit_type, ExitType::JvmError);
    assert!(!jvm.silent_failure);

    let normal = ExitSignal::classify(Some(0), Some(600_000), 0, "Game started fine");
    assert_eq!(normal.exit_type, ExitType::Normal);
    assert!(!normal.silent_failure);
    assert!(normal.status_hex().is_none());

    // exit 0 + 短命 + 有错误行 → 静默失败（HMCL 会判 NORMAL 放过）
    let silent = ExitSignal::classify(Some(0), Some(5_000), 4, "java.lang.Exception: boom");
    assert_eq!(silent.exit_type, ExitType::Normal);
    assert!(silent.silent_failure);

    // Windows 访问违例：负的 i32 → 十六进制状态码
    let access = ExitSignal::classify(Some(-1_073_741_819), Some(90_000), 0, "");
    assert!(access.is_access_violation());
    assert_eq!(access.status_hex().as_deref(), Some("0xc0000005"));

    for exit_type in [
        ExitType::Normal,
        ExitType::ApplicationError,
        ExitType::JvmError,
        ExitType::Sigkill,
        ExitType::Sigterm,
        ExitType::Killed,
        ExitType::Interrupted,
    ] {
        assert_eq!(ExitType::from_name(exit_type.as_str()), exit_type);
    }
}

#[test]
fn crash_report_collector_reads_newest_and_honours_started_at() {
    let run_dir = temp_dir("collect-crash");
    let reports = run_dir.join("crash-reports");
    fs::create_dir_all(&reports).expect("建目录");
    fs::write(
        reports.join("crash-2026-10-29_12.00.00-client.txt"),
        "---- Minecraft Crash Report ----\nDescription: Example\n",
    )
    .expect("写崩溃报告");

    let collector = CrashReportCollector::new();
    let collectors: [&dyn SignalCollector; 1] = [&collector];
    let (bundle, errors) = collect_all(&collectors, &CollectionContext::new(&run_dir));
    assert!(errors.is_empty());
    let captured = bundle.crash_report.expect("应采到崩溃报告");
    assert!(captured.text.contains("Description: Example"));
    assert_eq!(bundle.sources.len(), 1);

    // 只要「本次会话之后」的文件：把下界设到未来 → 不采
    let future =
        CollectionContext::new(&run_dir).since(SystemTime::now() + Duration::from_secs(3_600));
    let (bundle, errors) = collect_all(&collectors, &future);
    assert!(errors.is_empty());
    assert!(bundle.crash_report.is_none(), "历史崩溃不该算进本次");
}

#[test]
fn hs_err_collector_parses_newest_file() {
    let run_dir = temp_dir("collect-hs-err");
    fs::write(
        run_dir.join("hs_err_pid4242.log"),
        "#\n# A fatal error has been detected by the Java Runtime Environment:\n#\n#  EXCEPTION_ACCESS_VIOLATION (0xc0000005) at pc=0x1, pid=4242, tid=0x2\n# Problematic frame:\n# C  [nvoglv64.dll+0x10]\n",
    )
    .expect("写 hs_err");

    let collector = HsErrCollector::new();
    let collectors: [&dyn SignalCollector; 1] = [&collector];
    let (bundle, errors) = collect_all(&collectors, &CollectionContext::new(&run_dir));
    assert!(errors.is_empty());
    let report = bundle.hs_err.expect("应解析 hs_err");
    assert_eq!(report.kind, HsErrKind::AccessViolation);
    assert_eq!(report.pid, Some(4242));
    assert_eq!(bundle.sources.len(), 1);
}

#[test]
fn latest_log_collector_tails_n_lines() {
    let run_dir = temp_dir("collect-latest");
    let logs = run_dir.join("logs");
    fs::create_dir_all(&logs).expect("建目录");
    let mut text = String::new();
    for index in 0..10 {
        text.push_str(&format!("line {index}\n"));
    }
    fs::write(logs.join("latest.log"), text).expect("写日志");

    let collector = LatestLogCollector::new().with_max_lines(4);
    let collectors: [&dyn SignalCollector; 1] = [&collector];
    let (bundle, errors) = collect_all(&collectors, &CollectionContext::new(&run_dir));
    assert!(errors.is_empty());
    assert_eq!(bundle.log.pushed(), 4);
    assert!(bundle.log.text().contains("line 9"));
    assert!(!bundle.log.text().contains("line 5"));
}

#[test]
fn collect_all_aggregates_pushed_and_disk_signals() {
    let run_dir = temp_dir("collect-all");
    let reports = run_dir.join("crash-reports");
    fs::create_dir_all(&reports).expect("建目录");
    fs::write(
        reports.join("crash-a.txt"),
        "---- Minecraft Crash Report ----\n",
    )
    .expect("写报告");

    let snapshot = EnvironmentSnapshot {
        java_major: Some(17),
        java_bits: Some(64),
        xmx_mb: Some(4096),
        physical_memory_mb: Some(16_384),
        mods: vec![ModRecord::from_file_name("sodium-fabric-0.5.8.jar")],
        components: vec![unlucky_core::diagnose::collectors::ComponentVersion::new(
            "fabric", "0.15.0",
        )],
        ..EnvironmentSnapshot::default()
    };

    let exit = ExitCollector::new(ExitSignal::classify(Some(1), Some(9_000), 1, "boom"));
    let environment = EnvironmentCollector::new(snapshot);
    let crash = CrashReportCollector::new();
    let hs_err = HsErrCollector::new();
    let collectors: [&dyn SignalCollector; 4] = [&exit, &environment, &crash, &hs_err];

    let (bundle, errors) = collect_all(&collectors, &CollectionContext::new(&run_dir));
    assert!(errors.is_empty(), "单源缺失不应报错：{errors:?}");
    assert!(bundle.exit.is_some());
    assert!(bundle.crash_report.is_some());
    assert!(
        bundle.hs_err.is_none(),
        "没有 hs_err 文件时是 None（不是错误）"
    );
    let snapshot = bundle.environment.as_ref().expect("环境快照");
    assert_eq!(snapshot.java_major, Some(17));
    assert!(snapshot.summary().contains("Java 17"));
    assert_eq!(bundle.mods.len(), 1);
    assert!(bundle.mods[0].enabled);
    assert!(!bundle.is_empty());
}

#[test]
fn mod_record_parses_file_name() {
    let record = ModRecord::from_file_name("jei-1.20.1-15.2.0.27.jar");
    assert_eq!(record.id, "jei-1.20.1");
    assert_eq!(record.version.as_deref(), Some("15.2.0.27"));
    assert!(record.enabled);

    let disabled = ModRecord::from_file_name("optifine.jar.disabled");
    assert!(!disabled.enabled);
    assert_eq!(disabled.id, "optifine");
}

#[test]
fn bundle_input_groups_log_by_origin_and_carries_hs_err() {
    let mut bundle = SignalBundle::new();
    bundle.log.push(SignalOrigin::Stdout, "stdout line");
    bundle.log.push(SignalOrigin::Stderr, "stderr line");
    bundle.exit = Some(ExitSignal::classify(Some(1), Some(1_000), 1, "boom"));

    let input = bundle.diagnosis_input();
    assert!(input.exit.is_some());
    let origins = input
        .haystacks
        .iter()
        .map(|haystack| haystack.origin)
        .collect::<Vec<_>>();
    assert!(origins.contains(&SignalOrigin::Stdout));
    assert!(origins.contains(&SignalOrigin::Stderr));
    assert!(!input.is_empty());

    let mut with_hs_err = SignalBundle::new();
    with_hs_err.hs_err = Some(hs_err::parse(&read_sample(
        "samples/hs_err/access_violation.log",
    )));
    let input = with_hs_err.diagnosis_input();
    assert!(input.hs_err.is_some());
    assert!(input
        .haystacks
        .iter()
        .any(|haystack| haystack.origin == SignalOrigin::HsErr));
}

#[test]
fn empty_bundle_produces_empty_input() {
    let bundle = SignalBundle::new();
    assert!(bundle.is_empty());
    assert!(bundle.diagnosis_input().is_empty());
}
