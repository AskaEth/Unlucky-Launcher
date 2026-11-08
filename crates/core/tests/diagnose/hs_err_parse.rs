//! `hs_err_pid*.log` 解析器：标题行 / 线程段 / 内存段 / 失败帧。

use crate::prelude::*;

#[test]
fn parses_access_violation_report() {
    let text = read_sample("samples/hs_err/access_violation.log");
    let report = hs_err::parse(&text);

    assert_eq!(report.kind, HsErrKind::AccessViolation);
    assert_eq!(
        report.header.as_deref(),
        Some("A fatal error has been detected by the Java Runtime Environment:")
    );
    assert_eq!(
        report.exception.as_deref(),
        Some("EXCEPTION_ACCESS_VIOLATION (0xc0000005)")
    );
    assert_eq!(report.pc.as_deref(), Some("0x00007ffb1a2b3c4d"));
    assert_eq!(report.pid, Some(12344));
    assert_eq!(report.tid.as_deref(), Some("0x0000000000002a1c"));

    let frame = report.failed_frame.as_ref().expect("失败帧");
    assert_eq!(frame.kind, FrameKind::Native);
    assert_eq!(frame.module.as_deref(), Some("ig8d64.dll"));
    assert_eq!(frame.offset.as_deref(), Some("0x9b3c4d"));

    let thread = report.current_thread.as_deref().expect("当前线程");
    assert!(thread.contains("Render thread"), "{thread}");
    assert!(report
        .jre_version
        .as_deref()
        .is_some_and(|line| line.contains("8.0_51")));
    assert!(report
        .java_vm
        .as_deref()
        .is_some_and(|line| line.contains("HotSpot")));

    let memory = report.memory.as_ref().expect("内存段");
    assert_eq!(memory.page_size.as_deref(), Some("4k"));
    assert_eq!(memory.physical_kb, Some(16_741_480));
    assert_eq!(memory.physical_free_kb, Some(4_716_856));
    assert_eq!(memory.swap_kb, Some(18_672_384));
    assert_eq!(memory.swap_free_kb, Some(9_433_608));
    assert!(memory.summary().contains("GB"));

    assert!(!report.heap.is_empty(), "Heap 段应被抽取");
    assert!(report
        .vm_arguments
        .iter()
        .any(|arg| arg.contains("-Xmx4096m")));
    assert!(report
        .command_line
        .as_deref()
        .is_some_and(|line| line.contains("net.minecraft.launchwrapper.Launch")));
}

#[test]
fn digest_keeps_header_block_for_cross_line_rules() {
    let report = hs_err::parse(&read_sample("samples/hs_err/access_violation.log"));
    let digest = report.digest();
    // 跨行图形规则依赖「EXCEPTION_ACCESS_VIOLATION … # C  [ig…」的相对位置
    assert!(digest.contains("EXCEPTION_ACCESS_VIOLATION"));
    assert!(digest.contains("# C  [ig8d64.dll"));
    assert!(digest.contains("Current thread"));
    assert!(digest.contains("Memory:"));
}

#[test]
fn parses_out_of_memory_report() {
    let report = hs_err::parse(&read_sample("samples/hs_err/out_of_memory.log"));
    assert_eq!(report.kind, HsErrKind::OutOfMemory);
    assert_eq!(report.pid, Some(9876));
    assert_eq!(
        report.exception, None,
        "OOM 型 hs_err 没有 EXCEPTION_* 行，不该瞎猜"
    );
    assert!(report
        .header
        .as_deref()
        .is_some_and(|header| header.contains("insufficient memory")));
    let memory = report.memory.as_ref().expect("内存段");
    assert_eq!(memory.physical_kb, Some(8_134_564));
    assert!(report.summary().contains("out_of_memory"));
}

#[test]
fn parse_never_fails_on_garbage() {
    for text in ["", "\n\n", "随便一段中文", "#  EXCEPTION_ACCESS_VIOLATION"] {
        let report = hs_err::parse(text);
        // 不 panic 即可；字段可以全空
        let _ = report.digest();
        let _ = report.summary();
    }
    assert_eq!(hs_err::parse("").kind, HsErrKind::Other);
}

#[test]
fn parse_file_attaches_path() {
    let path = rules_dir().join("samples/hs_err/access_violation.log");
    let report = hs_err::parse_file(&path).expect("读文件");
    assert_eq!(report.path.as_deref(), Some(path.as_path()));
    assert_eq!(report.kind, HsErrKind::AccessViolation);
}

#[test]
fn missing_file_is_an_io_error() {
    let path = temp_dir("hs-err-missing").join("hs_err_pid1.log");
    assert!(hs_err::parse_file(&path).is_err());
}

#[test]
fn java_frame_is_classified() {
    let text = "\
#  EXCEPTION_ACCESS_VIOLATION (0xc0000005) at pc=0x0000000000000000, pid=1, tid=0x2
# Problematic frame:
# J 123 c1 com.example.mod.Foo.bar(Ljava/lang/String;)V (12 bytes) @ 0x00007ffb
";
    let report = hs_err::parse(text);
    let frame = report.failed_frame.as_ref().expect("失败帧");
    assert_eq!(frame.kind, FrameKind::Java);
    assert_eq!(frame.module, None);
}

#[test]
fn engine_reports_hs_err_out_of_memory() {
    let report = hs_err::parse(&read_sample("samples/hs_err/out_of_memory.log"));
    let mut input = DiagnosisInput::new();
    input.push_hs_err(report);
    let analysis = diagnoser().analyze(&input);
    let finding = analysis
        .findings
        .iter()
        .find(|finding| finding.rule_id == "hs_err.out_of_memory")
        .expect("hs_err OOM 应产出内置结论");
    assert_eq!(finding.category, RuleCategory::Memory);
    assert!(finding.message.contains("内存不足"));
    assert_eq!(
        finding.materialized_actions()[0].tool,
        "memory.set",
        "M16 需要能直接执行的工具 id"
    );
}
