//! M242：`ull diag <sid>` 的结论来源。两条端到端用例：**真 `SessionWriter` 写盘 → 真 CLI 入口**。
//!
//! 判据是「命中哪条规则」而不是「有没有结论」：脱敏有损，重跑出来的结论会**换一条**规则
//! （`fabric.loader_0_12` → `integrity.no_class_def_found`）。
//!
//! ## 夹具为什么把类路径登记成 secret
//!
//! M240 把「`≥64` 不透明串」这条收窄了：**路径形状**的长串（被 `/` 切开且每段都是词形名字）
//! 现在整条保留，所以 `SessionWriter` 落盘时这条证据行**今天不会被抹**。要复现「读回脱敏文本
//! 就丢了证据」这个世界（M239 实测的那份老落盘形状，也是脱敏规则以后再收紧时的形状），夹具
//! 把类路径登记成 secret 交给 `Redactor`：写盘前它被抹成 `<redacted:token>`，而结论是
//! 在**原始行**上算的 —— 与用户盘上真实存在的那类会话同样失真，足以分辨两条路。
//!
//! 变异证明（改完已还原）：
//! - 把 [`super::diagnosis::conclusion`] 里的 `stored(...)` 分支删掉（等于每次都重跑）⇒
//!   `diag_reads_the_stored_conclusion...` 红（`candidates[0].id` 变成 `integrity.no_class_def_found`）。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use unlucky_core::launch::exit_analysis::{self, ExitSignals};
use unlucky_core::launch::pump::{LogLine, Stream};
use unlucky_core::launch::redact::Redactor;
use unlucky_core::launch::session::{EndReason, SessionMeta, SessionWriter};
use unlucky_core::launch::session_index;

use crate::cli::Globals;
use crate::context::Ctx;

/// `fabric.loader_0_12`（critical / priority 80）唯一的证据行（`rules.d/fabric.yaml`）里的类路径。
const CLASS_PATH: &str = "org/spongepowered/asm/mixin/transformer/FabricMixinTransformerProxy";

/// 那条完整的证据行。
fn evidence_line() -> String {
    format!("java.lang.NoClassDefFoundError: {CLASS_PATH}")
}

/// 一个临时数据根（`Drop` 时清掉，与 `crates/cli/tests/support` 同口径）。
struct TempRoot(PathBuf);

impl TempRoot {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("ull-m242-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("建临时数据根");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 与生产同一条解析（`--data-dir` → `Ctx`）：会话根不在测试里手拼，避免「夹具说另一种形状」。
fn ctx_for(root: &Path) -> Ctx {
    let globals = Globals {
        data_dir: Some(root.to_path_buf()),
        ..Globals::default()
    };
    Ctx::resolve_in(&globals, None).expect("解析数据根")
}

/// 跑一次真 CLI 命令（`crate::run` = 解析 + `Ctx` + 命令实现），返回 `--json` 载荷的 `data`。
fn run_diag_json(root: &Path, session_id: &str) -> Value {
    let args: Vec<String> = vec![
        "diag".to_string(),
        session_id.to_string(),
        "--json".to_string(),
        "--data-dir".to_string(),
        root.display().to_string(),
    ];
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = crate::run(&args, &mut out, &mut err);
    let stdout = String::from_utf8_lossy(&out).into_owned();
    let stderr = String::from_utf8_lossy(&err).into_owned();
    assert_eq!(code, 0, "`ull diag` 应成功：{stderr}\n{stdout}");
    let value: Value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
        panic!("`--json` 输出不是 JSON：{error}\n{stdout}");
    });
    value["data"].clone()
}

/// 一局真会话：`SessionWriter` 写盘（写前脱敏）→ `finish` 在原始行上补结论 → 索引 + JSONL。
fn write_real_session(root: &Path) -> String {
    let session_id = "20261102T090000-m242abc";
    let mut writer = SessionWriter::create_with_id(
        &ctx_for(root).sessions_root(),
        session_id,
        SessionMeta::new("inst-1", "1.20.1 Fabric", "1.20.1"),
        // 登记成 secret ⇒ 盘上那条证据行的类路径被抹掉（见模块文档：为什么要人为造这份失真）。
        Redactor::new().with_secret(CLASS_PATH),
    )
    .expect("建会话");
    writer.session_start().expect("session_start");
    let lines = vec![
        LogLine::new(Stream::Stderr, "Failed to start Fabric loader"),
        LogLine::new(Stream::Stderr, evidence_line()),
        LogLine::new(Stream::Stdout, "[main/INFO]: Loading Minecraft 1.20.1"),
    ];
    for line in &lines {
        writer.line(line).expect("写行");
    }
    let analysis = exit_analysis::analyze(&ExitSignals::new(1, 40_000, &lines));
    writer.exit(&analysis).expect("exit");
    writer.finish(EndReason::Exit).expect("finish");
    session_id.to_string()
}

/// 落盘的会话文件（诊断事件也在这份里）。
fn event_file(root: &Path, session_id: &str) -> PathBuf {
    session_index::event_file(
        &ctx_for(root).sessions_root(),
        &session_index::log_path_for(session_id),
    )
}

/// 主判据：结论是**盘上那份**。只有读到 `diagnosis` 事件才能命中 `fabric.loader_0_12`——
/// 在脱敏文本上重跑永远命中不了它（证据行已被抹成占位符）。
#[test]
fn diag_reads_the_stored_conclusion_instead_of_rerunning_redacted_text() {
    let temp = TempRoot::new("diag-stored");
    let root = temp.path();
    let session_id = write_real_session(root);

    let data = run_diag_json(root, &session_id);
    assert_eq!(data["session_id"], session_id.as_str());
    assert_eq!(
        data["conclusion_source"], "stored",
        "有 diagnosis 事件时不得重跑：{data}"
    );
    assert_eq!(
        data["diagnosis"]["candidates"][0]["id"], "fabric.loader_0_12",
        "落盘的结论必须原样读出（重跑会降级成 integrity.no_class_def_found）：{data}"
    );
    assert_eq!(data["diagnosis"]["needs_more_info"], false);

    // 盘上那份**真的**丢了证据（不然这条用例证明不了「重跑会失真」），而修 bug ≠ 把原文写进盘。
    let disk = std::fs::read_to_string(event_file(root, &session_id)).expect("读会话文件");
    assert!(
        disk.contains("<redacted:token>"),
        "夹具前提：盘上的证据行必须已被抹掉：{disk}"
    );
    assert!(!disk.contains("FabricMixinTransformerProxy"), "{disk}");

    // 人类路径也要说清结论从哪来（否则「盘上的」与「刚算的」分不开）。
    let args: Vec<String> = vec![
        "diag".to_string(),
        session_id.clone(),
        "--data-dir".to_string(),
        root.display().to_string(),
    ];
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(crate::run(&args, &mut out, &mut err), 0);
    let human = String::from_utf8_lossy(&out);
    assert!(
        human.contains("结论来源: 会话落盘的 diagnosis 事件"),
        "{human}"
    );
    assert!(human.contains("主因: "), "{human}");
}

/// 老会话兼容：没有 `diagnosis` 事件（M241 之前写的 JSONL，且还在旧家）→ 回落重跑仍然可用。
///
/// 与上一条的差别只在**证据**：这里盘上只有脱敏文本，所以用的是短字面量规则
/// `memory.out_of_memory`（脱敏抹不掉 `java.lang.OutOfMemoryError`）。长串证据那类规则
/// 在老会话上只能降级，这是老数据本身的代价——`conclusion_source` 会标成 `rerun`。
#[test]
fn legacy_session_without_a_stored_conclusion_falls_back_to_rerunning() {
    let temp = TempRoot::new("diag-legacy");
    let root = temp.path();
    let session_id = "20261030T101500-legacy";
    let log_path = session_index::log_path_for(session_id);
    // M188 之前的生产落点（`data/agent/sessions`，只读兼容）：老会话的「老」还多一层。
    let legacy_root = ctx_for(root).legacy_sessions_root();
    let index = session_index::index_path(&legacy_root);
    std::fs::create_dir_all(index.parent().expect("索引父目录")).expect("建目录");
    let entry = json!({
        "v": 1,
        "session_id": session_id,
        "instance_id": "inst-1",
        "instance_name": "1.20.1",
        "game_version": "1.20.1",
        "started_at": "2026-10-30T10:15:00Z",
        "ended_at": "2026-10-30T10:15:06Z",
        "duration_ms": 5000,
        "exit_code": 1,
        "exit_type": "application_error",
        "silent_failure": false,
        "window_detected": false,
        "event_count": 2,
        "log_path": log_path,
        "crash_found": false,
        "diagnosis_top": null
    });
    std::fs::write(&index, format!("{entry}\n")).expect("写索引");
    let file = session_index::event_file(&legacy_root, &log_path);
    std::fs::write(
        &file,
        concat!(
            "{\"v\":1,\"seq\":0,\"kind\":\"stderr\",\"payload\":{\"line\":\"java.lang.OutOfMemoryError: Java heap space\",\"level\":\"error\"}}\n",
            "{\"v\":1,\"seq\":1,\"kind\":\"session_end\",\"payload\":{\"reason\":\"exit\"}}\n"
        ),
    )
    .expect("写老会话事件流");

    let data = run_diag_json(root, session_id);
    assert_eq!(
        data["conclusion_source"], "rerun",
        "没有 diagnosis 事件时必须回落重跑：{data}"
    );
    assert_eq!(
        data["diagnosis"]["candidates"][0]["id"], "memory.out_of_memory",
        "回落路径仍要给出结论：{data}"
    );
}
