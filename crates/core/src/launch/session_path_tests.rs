//! M211：写入侧与读取侧对**同一个相对路径**的往返用例（D2 / D3）。
//!
//! 这个文件存在的理由：原来的夹具（CLI `tests/support`、presenter `logs_tests`）各自
//! **手写**索引条目，于是「写入侧存 `game/<id>.jsonl`、读取侧又多 join 一层 `game/`」这个
//! bug 在测试里永远看不见——夹具自己说了第三种形状。这里跑**真的** [`SessionWriter`]，
//! 断言它写出的形状与 `session_index::event_file` 解析出的路径是同一个文件。
//!
//! 变异证明（把实现改回旧形状 ⇒ 本文件红）：
//! - `session.rs` 的 `log_path` 改回 `format!("game/{}.jsonl", …)` 之类的手写形状 ⇒
//!   `the_written_log_path_is_the_one_the_readers_resolve` 的第①③条断言红；
//! - `session_index::event_file` 里补回 `.join(GAME_LOG_DIR)` ⇒ 同一条用例的第③条红。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::launch::crash_scan;
use crate::launch::exit_analysis::{self, ExitSignals};
use crate::launch::redact::Redactor;
use crate::launch::session::{EndReason, SessionMeta, SessionWriter};
use crate::launch::session_index;

fn temp_dir(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ull-m211-path-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建夹具目录");
    root
}

fn writer(sessions_root: &Path, session_id: &str) -> SessionWriter {
    SessionWriter::create_with_id(
        sessions_root,
        session_id,
        SessionMeta::new("inst-1", "实例", "1.20.1"),
        Redactor::new(),
    )
    .expect("建会话写入器")
}

/// D2 主用例：写入侧存的形状 = 实际落点 = 读取侧解析出的路径（三者同一个文件）。
#[test]
fn the_written_log_path_is_the_one_the_readers_resolve() {
    let root = temp_dir("roundtrip");
    let mut sink = writer(&root, "s-round");
    sink.session_start().expect("写 session_start");
    let entry = sink.finish(EndReason::Exit).expect("收尾");

    // ① 写入侧存进索引的形状：**相对会话根**，带一层 `game/`。
    assert_eq!(
        entry.log_path, "game/s-round.jsonl",
        "写入侧的 log_path 形状漂了（读取侧按这个形状回找）"
    );
    // ② 事件流真的落在会话根下的 `game/`。
    let written = root.join("game").join("s-round.jsonl");
    assert!(
        written.is_file(),
        "事件流不在 {}：{}",
        written.display(),
        std::fs::read_dir(root.join("game"))
            .map(|items| items.filter_map(Result::ok).count())
            .unwrap_or(0)
    );
    // ③ 读取侧（CLI 的 `ull logs`/`ull diag` 与 presenter 的日志页共用）解析到**同一个文件**。
    assert_eq!(
        session_index::event_file(&root, &entry.log_path),
        written,
        "读取侧解析出的路径与写入侧的真落点不一致（双写 `game/`？）"
    );
    // ④ 真读一遍：读取侧拿到的路径里确实有事件。
    let text = std::fs::read_to_string(session_index::event_file(&root, &entry.log_path))
        .expect("读事件流");
    assert!(text.contains("\"session_start\""), "{text}");
    std::fs::remove_dir_all(&root).ok();
}

/// D3 主用例：干净退出（`code 0` / `error_lines 0`）⇒ 索引里 `crash_found` 必须是 `false`。
///
/// 夹具刻意按生产写：干净的一局也会写 `logs/latest.log`，所以「有 latest.log」不构成证据。
#[test]
fn a_clean_exit_is_not_indexed_as_a_crash() {
    let root = temp_dir("clean-index");
    let run_dir = temp_dir("clean-run");
    std::fs::create_dir_all(run_dir.join("logs")).expect("建 logs");
    std::fs::write(
        run_dir.join("logs/latest.log"),
        "[10:15:00] [main/INFO]: hi\n",
    )
    .expect("写 latest.log");

    let exit = exit_analysis::analyze(&ExitSignals::new(0, 60_000, &[]));
    assert_eq!(exit.code, 0);
    assert_eq!(exit.error_lines, 0, "夹具必须是干净退出");

    let artifacts = crash_scan::scan(&run_dir, SystemTime::now(), 8);
    let mut sink = writer(&root, "s-clean");
    sink.exit(&exit).expect("写 exit");
    for artifact in &artifacts {
        sink.crash_found(
            artifact.source.as_str(),
            &artifact.path,
            artifact.excerpt.as_deref(),
        )
        .expect("写 crash_found");
    }
    sink.mark_crash(crash_scan::has_crash_evidence(&artifacts, &exit));
    let entry = sink.finish(EndReason::Exit).expect("收尾");

    assert!(
        !entry.crash_found,
        "干净退出（code 0 / error_lines 0）被标成了崩溃：artifacts={artifacts:?}"
    );
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&run_dir).ok();
}

/// 对照用例：真有 `hs_err_pid*.log` 时仍然要标成崩溃（别把 D3 修成「永远 false」）。
#[test]
fn a_real_crash_scene_is_still_indexed_as_a_crash() {
    let root = temp_dir("crash-index");
    let run_dir = temp_dir("crash-run");
    std::fs::create_dir_all(run_dir.join("logs")).expect("建 logs");
    std::fs::write(run_dir.join("logs/latest.log"), "log\n").expect("写 latest.log");
    std::fs::write(run_dir.join("hs_err_pid77.log"), "SIGSEGV\n").expect("写 hs_err");

    let exit = exit_analysis::analyze(&ExitSignals::new(0, 60_000, &[]));
    let artifacts = crash_scan::scan(&run_dir, SystemTime::now(), 8);
    let mut sink = writer(&root, "s-crash");
    sink.exit(&exit).expect("写 exit");
    for artifact in &artifacts {
        sink.crash_found(
            artifact.source.as_str(),
            &artifact.path,
            artifact.excerpt.as_deref(),
        )
        .expect("写 crash_found");
    }
    sink.mark_crash(crash_scan::has_crash_evidence(&artifacts, &exit));
    let entry = sink.finish(EndReason::Exit).expect("收尾");

    assert!(entry.crash_found, "hs_err 在却没有标成崩溃：{artifacts:?}");
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&run_dir).ok();
}
