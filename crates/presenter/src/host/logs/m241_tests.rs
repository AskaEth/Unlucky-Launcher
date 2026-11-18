//! M241：诊断吃**原始**文本、展示/导出只吃脱敏文本（拆文件守 400 行上限）。

use super::*;
use unlucky_core::launch::exit_analysis::{self, ExitSignals};
use unlucky_core::launch::pump::{LogLine as CoreLine, Stream};
use unlucky_core::launch::redact::Redactor;
use unlucky_core::launch::session::{EndReason, SessionMeta, SessionWriter};

/// 一个临时数据根（`<root>/.ULL` + `<root>/.minecraft`）。
fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ull-m241-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// `fabric.loader_0_12`（critical / priority 80）在 `rules.d/fabric.yaml:114` 的唯一证据行。
///
/// 类路径 66 字符、无点号 → 撞上 `redact.rs` 的「≥64 不透明串」规则 → 脱敏后被抹成
/// `<redacted:opaque>`。这正是 M239 报的那个「会话路上永不命中」。
const EVIDENCE: &str = "java.lang.NoClassDefFoundError: \
                        org/spongepowered/asm/mixin/transformer/FabricMixinTransformerProxy";
/// 真凭据（写盘前必须被抹掉的形状）。
const TOKEN: &str = "SECRET-TOKEN-123456";

fn meta() -> SessionMeta {
    SessionMeta::new("inst", "1.20.1", "1.20.1")
}

/// 判据（M241）：① 规则引擎对含长类名的行**能命中**；② 用户可见/导出的文本**仍是被脱敏的**。
///
/// 端到端走真链路：`SessionWriter` 写盘（写前脱敏）→ 真 JSONL → [`open`] 读回。
///
/// 变异证明：
/// - 把 `SessionWriter::finish` 里补结论的输入换成脱敏后的行 ⇒ ① 红（`fabric.loader_0_12`
///   命不中，`diagnosis_top` 不是它）；
/// - 把 `SessionWriter::line` 里的 `scrub` 去掉（导出面改成原始文本）⇒ ② 红（类路径与凭据
///   原样出现在展示行与盘上）。
#[test]
fn raw_evidence_hits_the_rule_while_the_export_surface_stays_redacted() {
    let root = temp_root("diagraw");
    let env = HostEnv::at_root(&root);
    let redactor = Redactor::new().with_secret(TOKEN);
    let mut writer =
        SessionWriter::create_with_id(&env.sessions_root(), "diagraw-1", meta(), redactor)
            .expect("建会话");
    writer.session_start().expect("session_start");
    let lines = vec![
        CoreLine::new(Stream::Stderr, "Failed to start Fabric loader"),
        CoreLine::new(Stream::Stderr, EVIDENCE),
        CoreLine::new(Stream::Stdout, format!("auth: token={TOKEN}")),
    ];
    for line in &lines {
        writer.line(line).expect("写行");
    }
    let analysis = exit_analysis::analyze(&ExitSignals::new(1, 40_000, &lines));
    writer.exit(&analysis).expect("exit");
    let entry = writer.finish(EndReason::Exit).expect("finish");

    // ① 规则对含长类名的行能命中（结论在原始行上算过，落进索引）。
    assert_eq!(
        entry.diagnosis_top.as_deref(),
        Some("fabric.loader_0_12"),
        "会话路上必须命中 fabric.loader_0_12"
    );
    let (opened, diagnosis) = open(&env, "diagraw-1");
    assert_eq!(
        diagnosis.map(|report| report.diagnosis.rule),
        Some("fabric.loader_0_12".to_string())
    );

    // ② 展示面（日志行 DTO）：M240 收窄之后词形长路径不再被抹，所以类名会**原样出现**——
    // 这是有意的（它不是凭据，留着才看得懂崩溃原因）。凭据仍然必须被抹。
    assert!(
        opened
            .iter()
            .any(|line| line.text.contains("FabricMixinTransformerProxy")),
        "M240 之后展示行应保留类路径：{opened:?}"
    );
    assert!(
        !opened
            .iter()
            .any(|line| line.text.contains("<redacted:opaque>")),
        "类路径不该再被当成不透明串：{opened:?}"
    );
    assert!(
        !opened.iter().any(|line| line.text.contains(TOKEN)),
        "展示行不许出现凭据：{opened:?}"
    );

    // ②' 盘上那份（= 导出包收进去的同一份文件）也没有原文——修 bug 不等于把密钥写进盘。
    let disk = std::fs::read_to_string(event_path(&env, &entry)).expect("读会话文件");
    assert!(
        disk.contains("FabricMixinTransformerProxy"),
        "M240 之后盘上应保留类路径（它不是凭据）：{disk}"
    );
    assert!(!disk.contains(TOKEN), "盘上不许出现凭据：{disk}");
    std::fs::remove_dir_all(&root).ok();
}

/// 老会话（本次改动之前写的 JSONL，没有 `diagnosis` 事件）走兜底：在脱敏文本上重跑规则。
///
/// 判据：短字面量的规则（`memory.out_of_memory`）在脱敏文本上照样命中 ⇒ 兜底没坏。
#[test]
fn sessions_without_a_stored_conclusion_still_get_a_card() {
    let root = temp_root("diagraw-legacy");
    let env = HostEnv::at_root(&root);
    let entry = SessionIndexEntry {
        session_id: "legacy".to_string(),
        instance_id: "inst".to_string(),
        started_at: "2026-11-01T10:00:00.000Z".to_string(),
        ended_at: "2026-11-01T10:01:00.000Z".to_string(),
        exit_code: Some(1),
        exit_type: Some("application_error".to_string()),
        log_path: "game/legacy.jsonl".to_string(),
        ..SessionIndexEntry::default()
    };
    session_index::append(&session_index::index_path(&env.sessions_root()), &entry)
        .expect("写索引");
    let file = env.sessions_root().join("game/legacy.jsonl");
    std::fs::create_dir_all(file.parent().expect("父目录")).expect("建目录");
    std::fs::write(
        &file,
        concat!(
            "{\"kind\":\"stderr\",\"payload\":{\"line\":\"java.lang.OutOfMemoryError: Java heap space\",\"level\":\"error\"}}\n",
            "{\"kind\":\"session_end\",\"payload\":{\"reason\":\"exit\"}}\n"
        ),
    )
    .expect("写会话");

    let (lines, diagnosis) = open(&env, "legacy");
    assert!(lines
        .iter()
        .any(|line| line.text.contains("OutOfMemoryError")));
    assert_eq!(
        diagnosis.map(|report| report.diagnosis.rule),
        Some("memory.out_of_memory".to_string()),
        "没有落盘结论时应回落到规则重跑"
    );
    std::fs::remove_dir_all(&root).ok();
}
