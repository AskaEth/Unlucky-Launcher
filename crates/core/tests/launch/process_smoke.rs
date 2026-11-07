//! 进程冒烟测试：真实 spawn 一个 shell，验证泵、退出码、进程树 kill。
//!
//! 这些用例跑的是真进程（不是 mock），是“事件流 + 退出分析 + 树 kill”端到端的最小证据。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::prelude::*;

use unlucky_core::launch::process::{self, ProcessConfig, RunningProcess};
use unlucky_core::launch::pump::{LineSink, NullSink};

fn shell_command(script: &str) -> AssembledCommand {
    #[cfg(windows)]
    let (program, args) = (
        "cmd".to_string(),
        vec!["/C".to_string(), script.to_string()],
    );
    #[cfg(not(windows))]
    let (program, args) = (
        "/bin/sh".to_string(),
        vec!["-c".to_string(), script.to_string()],
    );
    AssembledCommand {
        program: PathBuf::from(program),
        args,
        classpath: Vec::new(),
        encoding: "UTF-8".to_string(),
        memory: MemoryOptions::default(),
        env: Vec::new(),
        warnings: Vec::new(),
        main_class: String::new(),
    }
}

fn config() -> ProcessConfig {
    ProcessConfig {
        graceful_stop: Duration::from_millis(200),
        ..ProcessConfig::default()
    }
}

#[test]
fn spawn_collects_both_streams_and_exit_code() {
    let root = temp_sessions_root("smoke");
    let meta = SessionMeta::new("inst-1", "smoke", "1.20.1");
    let writer =
        SessionWriter::create_with_id(&root, "smoke-1", meta, Redactor::new()).expect("创建会话");
    let path = writer.path().to_path_buf();
    let sink = Arc::new(SessionSink::new(writer));

    let mut child = RunningProcess::spawn(
        &shell_command("echo hello-from-stdout & echo hello-from-stderr 1>&2"),
        None,
        sink.clone(),
        config(),
    )
    .expect("spawn 应成功");

    let status = child.wait().expect("wait 应成功");
    assert_eq!(process::exit_code(&status), 0);
    assert!(child.pid() > 0);

    let verdict = child.supervise();
    assert!(verdict.is_complete(), "两条流都应在预算内结束");

    let texts: Vec<String> = child.lines().into_iter().map(|line| line.text).collect();
    assert!(
        texts.iter().any(|text| text.contains("hello-from-stdout")),
        "stdout 未收集到：{texts:?}"
    );
    assert!(
        texts.iter().any(|text| text.contains("hello-from-stderr")),
        "stderr 未收集到：{texts:?}"
    );

    // 事件流里应有 stdout/stderr 行事件。
    let events = read_events(&path);
    let stdout_lines = events
        .iter()
        .filter(|event| event["kind"] == "stdout")
        .count();
    let stderr_lines = events
        .iter()
        .filter(|event| event["kind"] == "stderr")
        .count();
    assert!(stdout_lines >= 1, "应有 stdout 事件");
    assert!(stderr_lines >= 1, "应有 stderr 事件");
}

#[test]
fn stop_kills_process_tree() {
    #[cfg(windows)]
    let script = "ping -n 30 127.0.0.1 >nul";
    #[cfg(not(windows))]
    let script = "sleep 30";

    let mut child =
        RunningProcess::spawn(&shell_command(script), None, Arc::new(NullSink), config())
            .expect("spawn 应成功");
    assert!(child.is_running());

    child.stop().expect("停止应成功");
    let status = child.wait().expect("wait 应成功");
    assert_ne!(
        process::exit_code(&status),
        0,
        "被结束的进程不应返回退出码 0"
    );
    assert!(!child.is_running());
}

#[cfg(target_os = "linux")]
#[test]
fn spawn_puts_child_in_its_own_process_group() {
    /// `/proc/<pid>/stat` 里的 `pgrp` 字段。
    fn pgid(pid: u32) -> u32 {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).expect("读 /proc stat");
        let tail = &stat[stat.rfind(')').expect("stat 应有 comm 括号") + 1..];
        tail.split_whitespace()
            .nth(2)
            .expect("stat 应有 pgrp 字段")
            .parse()
            .expect("pgrp 应是数字")
    }

    let mut child = RunningProcess::spawn(
        &shell_command("sleep 30"),
        None,
        Arc::new(NullSink),
        config(),
    )
    .expect("spawn 应成功");
    let pid = child.pid();
    let own_pgid = pgid(std::process::id());

    assert_eq!(
        pgid(pid),
        pid,
        "子进程应是自己的进程组组长（process_group(0)）"
    );
    assert_ne!(pgid(pid), own_pgid, "子进程不能挂在测试进程的进程组上");
    assert_eq!(
        unlucky_core::launch::tree_kill::group_kill_target(pid),
        Some(pid),
        "独立组的组长才是合法的组杀目标"
    );

    child.stop().expect("停止应成功");
    let _ = child.wait();
}

#[test]
fn spawn_reports_missing_program() {
    let mut command = shell_command("echo hi");
    command.program = PathBuf::from("definitely-not-a-real-binary-ull");
    let error = RunningProcess::spawn(&command, None, Arc::new(NullSink), config())
        .expect_err("程序不存在必须报错");
    assert!(matches!(error, LaunchError::Spawn { .. }));
}

#[test]
fn line_sink_trait_object_is_usable() {
    let sink: Arc<dyn LineSink> = Arc::new(NullSink);
    sink.on_line(&LogLine::new(Stream::Stdout, "ignored"));
}
