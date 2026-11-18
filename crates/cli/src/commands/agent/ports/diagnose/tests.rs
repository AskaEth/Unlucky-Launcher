//! M242：AI 侧 `diagnose_crash` 的结论来源（落盘那份优先，老会话回落重跑）。
//!
//! 两条端到端用例都走**真链路**：真 `SessionWriter` 写盘 → 真工具注册（
//! `unlucky_agent::tools::diagnose::register`）→ 真 handler。判据是「命中哪条规则」，
//! 因为脱敏有损：在脱敏文本上重跑会**换一条**规则（`fabric.loader_0_12` →
//! `integrity.no_class_def_found`）。
//!
//! ## 夹具为什么把类路径登记成 secret
//!
//! M240 收窄了「`≥64` 不透明串」那条规则（路径形状的长串现在保留），所以 `SessionWriter`
//! 落盘时这条证据行今天**不会被抹**。要复现「读回脱敏文本就丢了证据」这个世界（M239 实测的
//! 老落盘形状），夹具把类路径登记成 secret：写盘前它变成 `<redacted:token>`，而结论是在
//! **原始行**上算的 —— 与用户盘上真实存在的那类会话同样失真，足以分辨两条路。
//!
//! 变异证明（改完已还原）：
//! - 删掉 `diagnose()` 里的 `StoredRecord::find(&events)` 分支（等于每次都重跑）⇒
//!   `ai_gets_the_stored_conclusion...` 红（`candidates[0].id` 变成 `integrity.no_class_def_found`）。

use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};
use unlucky_agent::port::DiagnosePort;
use unlucky_agent::registry::{ToolCtx, ToolRegistry};
use unlucky_agent::tools::diagnose::DIAGNOSE_CRASH;
use unlucky_core::launch::exit_analysis::{self, ExitSignals};
use unlucky_core::launch::pump::{LogLine, Stream};
use unlucky_core::launch::redact::Redactor;
use unlucky_core::launch::session::{EndReason, SessionMeta, SessionWriter};
use unlucky_core::launch::session_index;

use super::CoreDiagnose;

/// `fabric.loader_0_12`（critical / priority 80）唯一的证据行（`rules.d/fabric.yaml`）里的类路径。
const CLASS_PATH: &str = "org/spongepowered/asm/mixin/transformer/FabricMixinTransformerProxy";
/// 真凭据（写盘前必须被抹掉的形状）。
const TOKEN: &str = "SECRET-TOKEN-123456";

/// 那条完整的证据行。
fn evidence_line() -> String {
    format!("java.lang.NoClassDefFoundError: {CLASS_PATH}")
}

/// 一个临时数据根（`<root>` 当数据根，会话根就是 `<root>`——端口只认会话根这一层）。
fn temp_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("ull-m242-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// 与生产写入同形（M212/D2）：`log_path` 相对会话根，事件流落在 `<根>/game/<id>.jsonl`。
fn seed(root: &std::path::Path, stderr_line: &str) {
    let sid = "20261029T124501-9f3c2a";
    let log_path = session_index::log_path_for(sid);
    let index = session_index::index_path(root);
    std::fs::create_dir_all(index.parent().expect("索引父目录")).expect("建目录");
    let entry = json!({
        "v": 1,
        "session_id": sid,
        "instance_id": "inst-1",
        "instance_name": "1.20.1 Forge",
        "game_version": "1.20.1",
        "started_at": "2026-10-29T12:45:01.123Z",
        "ended_at": "2026-10-29T12:45:05.123Z",
        "duration_ms": 4000,
        "exit_code": 1,
        "exit_type": "application_error",
        "silent_failure": false,
        "window_detected": false,
        "event_count": 1,
        "log_path": log_path.clone(),
        "crash_found": true,
        "diagnosis_top": null
    });
    std::fs::write(index, format!("{entry}\n")).expect("写索引");
    let event = json!({
        "seq": 0, "ts": "2026-10-29T12:45:01.123Z", "t_ms": 0,
        "kind": "stderr",
        "payload": { "line": stderr_line, "level": "error" }
    });
    std::fs::write(
        session_index::event_file(root, &log_path),
        format!("{event}\n"),
    )
    .expect("写事件");
}

/// 一局真会话：`SessionWriter` 写盘（写前脱敏）→ `finish` 在原始行上补结论。
fn write_real_session(sessions_root: &Path) -> String {
    let session_id = "20261102T091500-m242ai";
    let mut writer = SessionWriter::create_with_id(
        sessions_root,
        session_id,
        SessionMeta::new("inst-1", "1.20.1 Fabric", "1.20.1"),
        // 类路径登记成 secret ⇒ 盘上那条证据行被抹掉（见模块文档：为什么要人为造这份失真）。
        Redactor::new().with_secret(TOKEN).with_secret(CLASS_PATH),
    )
    .expect("建会话");
    writer.session_start().expect("session_start");
    let lines = vec![
        LogLine::new(Stream::Stderr, "Failed to start Fabric loader"),
        LogLine::new(Stream::Stderr, evidence_line()),
        LogLine::new(Stream::Stdout, format!("auth: token={TOKEN}")),
    ];
    for line in &lines {
        writer.line(line).expect("写行");
    }
    let analysis = exit_analysis::analyze(&ExitSignals::new(1, 40_000, &lines));
    writer.exit(&analysis).expect("exit");
    writer.finish(EndReason::Exit).expect("finish");
    session_id.to_string()
}

/// 走真工具入口调 `diagnose_crash`：注册表 + schema + handler 与 `ull agent serve` 同一份。
fn call_diagnose_crash(sessions_root: &Path, session_id: &str) -> Value {
    let mut registry = ToolRegistry::new();
    unlucky_agent::tools::diagnose::register(
        &mut registry,
        Arc::new(CoreDiagnose::new(sessions_root)),
    )
    .expect("注册 diagnose_crash");
    let spec = registry.get(DIAGNOSE_CRASH).expect("工具已注册").clone();
    let ctx = ToolCtx {
        agent_session_id: "m242".to_string(),
        confirmed: false,
    };
    let output =
        (spec.handler)(&ctx, &json!({ "session_id": session_id })).expect("diagnose_crash 应成功");
    output.content
}

#[test]
fn ai_gets_the_stored_conclusion_instead_of_a_rerun_on_redacted_text() {
    let root = temp_root("ai-stored");
    let session_id = write_real_session(&root);

    let content = call_diagnose_crash(&root, &session_id);
    assert_eq!(content["session_id"], session_id.as_str());
    assert_eq!(
        content["candidates"][0]["id"], "fabric.loader_0_12",
        "落盘的结论必须原样交给模型（重跑会降级成 integrity.no_class_def_found）：{content}"
    );
    assert_eq!(content["needs_more_info"], false);
    let confidence = content["top_confidence"].as_f64().expect("置信度是数字");
    assert!(
        (confidence - 0.85).abs() < 1e-3,
        "落盘结论的置信度应原样透传：{content}"
    );

    // 隐私选择不变：给模型的文本片段仍是脱敏的（类路径与凭据都不许出现）。
    let rendered = content.to_string();
    assert!(
        !rendered.contains("FabricMixinTransformerProxy"),
        "{rendered}"
    );
    assert!(!rendered.contains(TOKEN), "{rendered}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn runs_the_rule_engine_over_a_session() {
    let root = temp_root("diagnose");
    seed(
        &root,
        "java.lang.NoClassDefFoundError: cpw/mods/bootstraplauncher/BootstrapLauncher",
    );

    let port = CoreDiagnose::new(root.clone());
    let report = port.diagnose("20261029T124501-9f3c2a").expect("应能诊断");
    assert_eq!(report.session_id, "20261029T124501-9f3c2a");
    assert_eq!(report.source, "rules");
    assert_eq!(report.exit.code, Some(1));
    // 老会话（没有 `diagnosis` 事件）→ 回落重跑：规则集里应有完整性类结论命中这条 stderr。
    assert!(!report.candidates.is_empty(), "应至少命中一条规则");
    assert_eq!(report.candidates[0].id, "integrity.no_class_def_found");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unknown_session_is_a_readable_error() {
    let root = temp_root("diagnose-none");
    let port = CoreDiagnose::new(root);
    let err = port.diagnose("nope").unwrap_err();
    assert!(err.contains("找不到会话"), "{err}");
}
