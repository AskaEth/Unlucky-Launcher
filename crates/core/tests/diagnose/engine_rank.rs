//! 引擎行为：主因排序 / 抑制关系 / 去重 / 内置结论 / 动作物化。

use crate::prelude::*;

#[test]
fn root_cause_outranks_secondary_error() {
    // 一条日志里既 OOM 又 NoClassDefFoundError：OOM 是根因（priority 80 vs 40）。
    let text = "\
java.lang.OutOfMemoryError: Java heap space
java.lang.NoClassDefFoundError: net/minecraftforge/fml/loading/FMLEnvironment
";
    let report = analyze_text(SignalOrigin::Stdout, text);
    assert_eq!(
        report.primary().expect("命中").rule_id,
        "memory.out_of_memory"
    );
    assert!(
        ids(&report).contains(&"integrity.no_class_def_found"),
        "低级错误也应给出（哪怕是次生）：{:?}",
        ids(&report)
    );
}

#[test]
fn suppress_moves_rule_into_secondary() {
    // 32 位 Java 分配失败 + 次生 OutOfMemoryError
    let text = "\
Could not reserve enough space for 4194304KB object heap
java.lang.OutOfMemoryError: Java heap space
";
    let report = analyze_text(SignalOrigin::Stdout, text);
    assert_eq!(report.primary().expect("命中").rule_id, "jvm.32bit");
    let secondary = report
        .secondary
        .iter()
        .find(|finding| finding.rule_id == "memory.out_of_memory")
        .expect("OOM 应被降级为次生结论而不是丢弃");
    assert_eq!(secondary.suppressed_by.as_deref(), Some("jvm.32bit"));
    assert!(
        !ids(&report).contains(&"memory.out_of_memory"),
        "被抑制的结论不应出现在主因列表"
    );
}

#[test]
fn same_rule_keeps_best_evidence_source() {
    // 同一条文本在两个来源出现：只留一条结论，且优先崩溃报告。
    let mut input = DiagnosisInput::new();
    input.push_text(
        SignalOrigin::Stdout,
        "java.lang.OutOfMemoryError: Java heap space",
    );
    input.push_text(
        SignalOrigin::CrashReport,
        "---- Minecraft Crash Report ----\njava.lang.OutOfMemoryError: Java heap space",
    );
    let report = diagnoser().analyze(&input);
    let hits = report
        .findings
        .iter()
        .filter(|finding| finding.rule_id == "memory.out_of_memory")
        .count();
    assert_eq!(hits, 1, "同一规则只应报一次");
    let finding = &report.findings[0];
    assert_eq!(finding.evidence[0].origin, SignalOrigin::CrashReport);
}

#[test]
fn access_violation_exit_is_diagnosed() {
    let exit = ExitSignal::classify(Some(-1_073_741_819), Some(90_000), 0, "");
    let mut input = DiagnosisInput::new();
    input.exit = Some(exit);
    let report = diagnoser().analyze(&input);
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.rule_id == "signal.access_violation")
        .expect("0xC0000005 应产出内置结论");
    assert_eq!(finding.category, RuleCategory::Graphics);
    let action = &finding.materialized_actions()[0];
    assert_eq!(action.kind, FixActionKind::ChangeGraphics);
    assert_eq!(action.tool, "graphics.set");
    assert_eq!(
        action.args.get("update_driver").map(String::as_str),
        Some("true")
    );
}

#[test]
fn silent_failure_is_marked_on_exit_zero() {
    // analyse/02 §7.10-10：exit 0 + 存活 <30s + 有错误行 → 静默失败，HMCL 会判 NORMAL 放过。
    let exit = ExitSignal::classify(Some(0), Some(4_200), 3, "java.lang.Exception: boom");
    assert_eq!(exit.exit_type, ExitType::Normal);
    assert!(exit.silent_failure);
    let mut input = DiagnosisInput::new();
    input.exit = Some(exit);
    let report = diagnoser().analyze(&input);
    assert!(
        ids(&report).contains(&"signal.silent_failure"),
        "静默失败应单独打标：{:?}",
        ids(&report)
    );
    // 低置信 → 交 AI 深化（PlanDoc/06 §2.3）
    assert!(report.needs_more_info());
}

#[test]
fn empty_input_asks_for_more_information() {
    let report = diagnoser().analyze(&DiagnosisInput::new());
    assert!(report.findings.is_empty());
    assert!(report.needs_more_info());
}

#[test]
fn conflict_rule_materializes_capture_groups() {
    let text = read_sample("samples/fabric/mod_resolution_conflict.log");
    let report = analyze_text(SignalOrigin::Stdout, &text);
    let primary = report.primary().expect("命中");
    assert_eq!(primary.rule_id, "fabric.mod_resolution_conflict");
    let action = &primary.materialized_actions()[0];
    assert_eq!(action.kind, FixActionKind::RemoveMod);
    assert_eq!(action.tool, "mod.remove");
    assert_eq!(
        action.args.get("mod_ids").map(String::as_str),
        Some("sodium,sodium-extra"),
        "mod_ids 应已代入命名捕获组"
    );
}

/// 回归（review P2-1）：OptiFine/Forge 兼容规则最后一个 alternative 必须是**前缀语义**，
/// 既收裸 `(Reflector.java)` 也收真实的带行号形态 `(Reflector.java:455)`。
#[test]
fn optifine_forge_matches_both_frame_forms() {
    for frame in [
        "\tTRANSFORMER/net.optifine/net.optifine.reflect.Reflector.<clinit>(Reflector.java:455)",
        "\tTRANSFORMER/net.optifine/net.optifine.reflect.Reflector.<clinit>(Reflector.java)",
    ] {
        let report = analyze_text(SignalOrigin::Stdout, frame);
        assert!(
            ids(&report).contains(&"conflict.optifine_forge"),
            "真实栈帧形态应命中：{frame}（实际命中：{:?}）",
            ids(&report)
        );
    }
}

#[test]
fn too_old_java_converts_class_file_version() {
    let text = read_sample("samples/jvm/too_old_java.log");
    let report = analyze_text(SignalOrigin::Stdout, &text);
    let primary = report.primary().expect("命中");
    assert_eq!(primary.rule_id, "jvm.too_old_java");
    let action = &primary.materialized_actions()[0];
    assert_eq!(
        action.args.get("required_major").map(String::as_str),
        Some("17"),
        "class file version 61 应换算成 Java 17"
    );
    assert_eq!(class_file_to_java_major(61), Some(17));
    assert_eq!(class_file_to_java_major(52), Some(8));
    assert_eq!(class_file_to_java_major(40), None);
}

#[test]
fn fix_actions_are_deduplicated_across_findings() {
    // 一条日志同时命中 OOM（adjust_memory）与 32bit（adjust_memory + change_java）
    let text = "\
Could not reserve enough space for 4194304KB object heap
java.lang.OutOfMemoryError: Java heap space
";
    let report = analyze_text(SignalOrigin::Stdout, text);
    let actions = report.fix_actions();
    assert!(!actions.is_empty());
    let mut unique = actions.clone();
    unique.sort_by(|a, b| a.tool.cmp(b.tool).then(a.args.len().cmp(&b.args.len())));
    unique.dedup();
    assert_eq!(unique.len(), actions.len(), "fix_actions 应当已去重");
}

#[test]
fn record_matches_launch_diagnosis_payload() {
    let text = read_sample("samples/memory/out_of_memory.log");
    let report = analyze_text(SignalOrigin::Stdout, &text);
    let record = report.to_record();
    assert_eq!(record.source, "rules");
    assert!(!record.candidates.is_empty());
    let candidate = &record.candidates[0];
    assert_eq!(candidate.id, "memory.out_of_memory");
    assert_eq!(candidate.severity, "critical");
    assert_eq!(candidate.category, "memory");
    assert_eq!(candidate.message_key, "diagnose.cause.memory.out_of_memory");
    assert!(!candidate.message.is_empty(), "M16 需要渲染后的中文结论");
    assert!(!candidate.evidence.is_empty());
    assert_eq!(candidate.fix_actions[0].tool, "memory.set");
}

#[test]
fn hs_err_report_feeds_builtin_findings() {
    let report = hs_err::parse(&read_sample("samples/hs_err/access_violation.log"));
    let mut input = DiagnosisInput::new();
    input.push_hs_err(report);
    let analysis = diagnoser().analyze(&input);
    assert_eq!(
        analysis.primary().expect("命中").rule_id,
        "hs_err.access_violation"
    );
    // graphics.graphics_driver 会被内置结论 suppress 成次生（同一条证据、不同粒度）
    assert!(
        analysis
            .secondary
            .iter()
            .any(|finding| finding.rule_id == "graphics.graphics_driver"),
        "hs_err 原文也应命中图形规则，但被降级：{:?}",
        analysis
            .secondary
            .iter()
            .map(|finding| finding.rule_id.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn file_scope_rules_survive_very_long_logs() {
    // >1 MiB：首尾窗口扫描，尾部窗口里的签名仍应命中。
    let mut text = String::from("noise line\n").repeat(120_000);
    text.push_str("java.lang.OutOfMemoryError: Java heap space\n");
    assert!(text.len() > 1024 * 1024);
    let report = analyze_text(SignalOrigin::LatestLog, &text);
    assert!(
        ids(&report).contains(&"memory.out_of_memory"),
        "超长日志的尾部窗口应被扫描"
    );
}
