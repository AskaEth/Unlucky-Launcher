//! 顶层启动管线测试：dry-run（D18）、完整会话、停止、组装失败、崩溃现场。
//!
//! 真进程来自 shell（`cmd /C` 或 `sh -c`），所以这些是端到端用例：
//! 命令行组装 → 事件流落盘 → 退出判定 → 诊断挂钩 → 索引。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use crate::prelude::*;

/// 记录调用并返回固定结论的诊断挂钩。
#[derive(Default)]
struct RecordingHook {
    calls: Mutex<Vec<String>>,
}

impl DiagnosisHook for RecordingHook {
    fn analyze(&self, request: &DiagnosisRequest) -> Option<serde_json::Value> {
        self.calls.lock().unwrap().push(request.summary());
        Some(json!({
            "source": "rules",
            "needs_more_info": false,
            "candidates": [{ "id": "stub_candidate", "severity": "high", "confidence": 0.9 }],
        }))
    }
}

fn launcher(sessions_root: &std::path::Path, hook: Option<Arc<RecordingHook>>) -> Launcher {
    let mut config = LauncherConfig::new(sessions_root);
    // 固定平台上下文，让命令行断言与运行平台无关。
    config.platform = PlatformCtx::new(OsFamily::Windows, "x86_64");
    config.process = ProcessConfig {
        graceful_stop: Duration::from_millis(200),
        ..ProcessConfig::default()
    };
    let launcher = Launcher::new(config);
    match hook {
        Some(hook) => launcher.with_diagnosis_hook(hook),
        None => launcher,
    }
}

/// 用 shell 冒充 java：只为验证管线，不验证 JVM 语义。
fn shell_request(script: &str, run_dir: &std::path::Path) -> (LaunchRequest, LaunchRequest) {
    let (mut request, ctx) = legacy_forge_request();
    let _ = ctx;
    #[cfg(windows)]
    {
        request.wrapper = vec!["cmd".to_string(), "/C".to_string(), script.to_string()];
    }
    #[cfg(not(windows))]
    {
        request.wrapper = vec!["/bin/sh".to_string(), "-c".to_string(), script.to_string()];
    }
    request.java = JavaSelection::new("fake-java", 17, 64);
    request.manifest.legacy_arguments = None;
    request.dirs.game_dir = run_dir.to_path_buf();
    // natives 解压是管线第 6 段，会真写盘——指到临时目录，绝不碰 `C:/mc`。
    request.dirs.natives_dir = run_dir.join("natives");
    request.user.jvm = Vec::new();
    request.user.game = Vec::new();
    (request.clone(), request)
}

fn echo_script() -> String {
    // 命令里带上 access_token 明文，用来验证“落盘前脱敏”。
    #[cfg(windows)]
    {
        "echo LWJGL Version: 3.3.1 SECRET-TOKEN-123456".to_string()
    }
    #[cfg(not(windows))]
    {
        "echo 'LWJGL Version: 3.3.1 SECRET-TOKEN-123456'".to_string()
    }
}

fn long_running_script(dir: &std::path::Path) -> String {
    #[cfg(windows)]
    let (file, body) = (
        dir.join("linger.bat"),
        "@echo off\r\nping -n 30 127.0.0.1 >nul\r\n",
    );
    #[cfg(not(windows))]
    let (file, body) = (dir.join("linger.sh"), "#!/bin/sh\nsleep 30\n");
    std::fs::write(&file, body).expect("写长跑脚本");
    file.display().to_string()
}

#[test]
fn dry_run_returns_command_without_spawning_or_writing() {
    let sessions = temp_sessions_root("dry");
    let run_dir = temp_dir("dry-run");
    let launcher = launcher(&sessions, None);
    let (request, _) = shell_request(&echo_script(), &run_dir);

    let dry: DryRun = launcher.dry_run(&request).expect("dry-run 应成功");
    assert!(!dry.argv.is_empty());
    assert!(
        dry.display.contains("fake-java")
            || dry.display.contains("cmd")
            || dry.display.contains("/bin/sh")
    );
    assert!(
        dry.display.contains("<redacted:token>"),
        "命令行必须脱敏：{}",
        dry.display
    );
    assert!(!dry.display.contains("SECRET-TOKEN-123456"));
    assert_eq!(dry.classpath_entries, 3);
    assert_eq!(dry.line(), dry.display.as_str());
    // 不 spawn、不写会话文件。
    assert!(!sessions.exists(), "dry-run 不能创建会话目录");
}

#[test]
fn pipeline_records_session_events_diagnosis_and_index() {
    let sessions = temp_sessions_root("pipeline");
    let run_dir = temp_dir("pipeline-run");
    let hook = Arc::new(RecordingHook::default());
    let launcher = launcher(&sessions, Some(hook.clone()));
    let (request, _) = shell_request(&echo_script(), &run_dir);

    let handle: SessionHandle = launcher.launch(&request).expect("启动应成功");
    assert_eq!(handle.exit.exit_type, ExitType::Normal);
    assert_eq!(handle.exit.code, 0);
    // 存活 <30s → 静默失败 + 需要诊断。
    assert!(handle.exit.silent_failure.is_some());
    assert!(handle.needs_diagnosis());
    assert_eq!(hook.calls.lock().unwrap().len(), 1, "诊断挂钩应被调用一次");
    assert_eq!(
        handle
            .diagnostics
            .as_ref()
            .and_then(|value| value["candidates"][0]["id"].as_str()),
        Some("stub_candidate")
    );
    assert!(
        handle.event_write_errors.is_empty(),
        "{:?}",
        handle.event_write_errors
    );
    // 对外展示的 argv 已是脱敏版本（脚本里带着 token 明文）。
    assert!(handle
        .redacted_argv()
        .iter()
        .any(|arg| arg.contains("<redacted:token>")));
    assert!(!handle
        .redacted_argv()
        .iter()
        .any(|arg| arg.contains("SECRET-TOKEN-123456")));
    assert!(handle.redacted_display().contains("<redacted:token>"));
    assert!(handle.index.is_some());
    assert!(handle.uptime_ms < 30_000);

    let events = read_events(&handle.log_path);
    let kinds: Vec<&str> = events
        .iter()
        .filter_map(|event| event["kind"].as_str())
        .collect();
    for expected in [
        "session_start",
        "cmd_built",
        "proc_start",
        "stdout",
        "window_detected",
        "exit",
        "diagnosis",
        "session_end",
    ] {
        assert!(kinds.contains(&expected), "缺少事件 {expected}：{kinds:?}");
    }

    // 日志行里的 access_token 必须已脱敏。
    let raw = std::fs::read_to_string(&handle.log_path).expect("读会话文件");
    assert!(!raw.contains("SECRET-TOKEN-123456"), "事件流不得含凭据明文");

    // 索引落盘且字段正确。
    let index = session_index::read_all(&session_index::index_path(&sessions)).expect("读索引");
    assert_eq!(index.len(), 1);
    assert_eq!(index[0].session_id, handle.session_id);
    assert!(index[0].silent_failure);
    assert!(index[0].window_detected);
    assert_eq!(index[0].exit_code, Some(0));
    assert_eq!(index[0].diagnosis_top.as_deref(), Some("stub_candidate"));
}

#[test]
fn pipeline_stop_records_killed_exit() {
    let sessions = temp_sessions_root("killed");
    let run_dir = temp_dir("killed-run");
    let launcher = launcher(&sessions, None);
    let (request, _) = shell_request(&long_running_script(&run_dir), &run_dir);

    let mut session = launcher.spawn(&request).expect("spawn 应成功");
    assert!(session.pid() > 0);
    session.stop().expect("停止应成功");
    let handle = session.finish().expect("收尾应成功");

    assert_eq!(handle.exit.exit_type, ExitType::Killed);
    assert!(!handle.succeeded());
    let events = read_events(&handle.log_path);
    let kinds: Vec<&str> = events
        .iter()
        .filter_map(|event| event["kind"].as_str())
        .collect();
    assert!(kinds.contains(&"killed"), "应记录 killed 事件：{kinds:?}");
    let index = session_index::read_all(&session_index::index_path(&sessions)).expect("读索引");
    assert_eq!(index.len(), 1);
    assert_eq!(index[0].exit_type.as_deref(), Some("killed"));
}

#[test]
fn a_kill_from_another_thread_is_recorded_as_killed() {
    // D10：GUI 的 `host::launch::stop(pid)` 是从别的线程直接 `kill_tree`，不经过
    // `LaunchSession::stop()`。这条路以前会记成 `application_error`（Windows 退出码 1）。
    let sessions = temp_sessions_root("external-kill");
    let run_dir = temp_dir("external-kill-run");
    let launcher = launcher(&sessions, None);
    let (request, _) = shell_request(&long_running_script(&run_dir), &run_dir);

    let session = launcher.spawn(&request).expect("spawn 应成功");
    let pid = session.pid();
    // `kill_tree` 的返回值不代表「进程还在」：温和阶段可能已经关掉了它（taskkill 报 128）。
    let _ = unlucky_core::launch::tree_kill::kill_tree(pid, Duration::from_millis(200));
    let handle = session.finish().expect("收尾应成功");

    assert_eq!(
        handle.exit.exit_type,
        ExitType::Killed,
        "我们杀的要判成 killed：{}",
        handle.exit.summary()
    );
    assert!(
        !handle.exit.terminated_externally,
        "有登记作据就不该只是「疑似」：{}",
        handle.exit.summary()
    );
}

#[test]
fn pipeline_assembly_failure_still_writes_evidence() {
    let sessions = temp_sessions_root("assembly-fail");
    let run_dir = temp_dir("assembly-fail-run");
    let launcher = launcher(&sessions, None);
    let (mut request, _) = shell_request(&echo_script(), &run_dir);
    request.missing_components = vec![MissingComponent::named("org.lwjgl:lwjgl:3.3.1")];

    let error = launcher.spawn(&request).expect_err("缺组件必须启动前失败");
    assert!(matches!(
        error,
        LaunchError::Assembly(AssemblyError::MissingComponents(_))
    ));

    let game_dir = sessions.join("game");
    let files: Vec<PathBuf> = std::fs::read_dir(&game_dir)
        .expect("会话目录应存在")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().map(|ext| ext == "jsonl").unwrap_or(false))
        .filter(|path| {
            path.file_stem()
                .map(|stem| stem != "index")
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(files.len(), 1, "应留下一个会话文件");
    let events = read_events(&files[0]);
    let kinds: Vec<&str> = events
        .iter()
        .filter_map(|event| event["kind"].as_str())
        .collect();
    assert_eq!(
        kinds,
        vec![
            "session_start",
            "natives_extracted",
            "assembly_failed",
            "session_end"
        ]
    );
    let missing = &events[2]["payload"]["missing_components"][0]["name"];
    assert_eq!(missing.as_str(), Some("org.lwjgl:lwjgl:3.3.1"));

    let index = session_index::read_all(&session_index::index_path(&sessions)).expect("读索引");
    assert_eq!(index.len(), 1);
    assert!(index[0].exit_type.is_none(), "没有进程就没有退出判定");
}

#[test]
fn pipeline_records_crash_report_artifact() {
    let sessions = temp_sessions_root("crash");
    let run_dir = temp_dir("crash-run");
    let crash_dir = run_dir.join("crash-reports");
    std::fs::create_dir_all(&crash_dir).expect("建 crash-reports");
    std::fs::write(
        crash_dir.join("crash-2026-10-29.txt"),
        "---- Minecraft Crash Report ----\nDescription: Rendering screen\n",
    )
    .expect("写崩溃报告");

    let launcher = launcher(&sessions, None);
    let (request, _) = shell_request(&echo_script(), &run_dir);
    let handle = launcher.launch(&request).expect("启动应成功");

    assert!(
        handle
            .artifacts
            .iter()
            .any(|artifact| artifact.source == CrashSource::CrashReport),
        "应发现崩溃报告：{:?}",
        handle.artifacts
    );
    let events = read_events(&handle.log_path);
    let crash = events
        .iter()
        .find(|event| event["kind"] == "crash_found")
        .expect("应有 crash_found 事件");
    let path = crash["payload"]["path"].as_str().unwrap_or_default();
    assert!(path.ends_with("crash-2026-10-29.txt"));
    assert!(crash["payload"]["excerpt"]
        .as_str()
        .unwrap_or_default()
        .contains("Minecraft Crash Report"));
}

#[test]
fn pipeline_spawn_failure_writes_session_end() {
    let sessions = temp_sessions_root("spawn-fail");
    let run_dir = temp_dir("spawn-fail-run");
    let launcher = launcher(&sessions, None);
    let (mut request, _) = shell_request(&echo_script(), &run_dir);
    // 包装器就是 program：给一个不存在的可执行文件，走派生失败分支。
    request.wrapper = vec!["definitely-not-a-real-binary-ull".to_string()];

    let error = launcher.spawn(&request).expect_err("程序不存在必须失败");
    assert!(
        matches!(error, LaunchError::Spawn { .. }),
        "错误类型：{error:?}"
    );

    let game_dir = sessions.join("game");
    let files: Vec<PathBuf> = std::fs::read_dir(&game_dir)
        .expect("会话目录应存在")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().map(|ext| ext == "jsonl").unwrap_or(false))
        .filter(|path| {
            path.file_stem()
                .map(|stem| stem != "index")
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(files.len(), 1, "应留下一个会话文件");
    let events = read_events(&files[0]);
    let kinds: Vec<&str> = events
        .iter()
        .filter_map(|event| event["kind"].as_str())
        .collect();
    assert_eq!(
        kinds,
        vec![
            "session_start",
            "natives_extracted",
            "cmd_built",
            "session_end"
        ]
    );
    assert_eq!(events[3]["payload"]["reason"], "spawn_failed");

    let index = session_index::read_all(&session_index::index_path(&sessions)).expect("读索引");
    assert_eq!(index.len(), 1, "派生失败也要进索引");
    assert!(index[0].exit_type.is_none());
}
