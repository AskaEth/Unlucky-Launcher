//! D6 看门狗：有界等待（真进程，不是 mock）。
//!
//! 四条用例各自钉一个判据：
//!
//! 1. 没窗口也没退出 → 立刻停止等待并报错，**进程还活着**（不杀）；
//! 2. 窗口出现过 → 慢慢加载不会被判死，正常退出照常给结果；
//! 3. 出现关闭标记后仍不退出 → 判「卡在关闭」（实机 D6 的形态）；
//! 4. 短命的正常退出 → 不会被超时误判成错误。
//!
//! 子进程一律是**脚本文件**（`.bat` / `.sh`），不把 `&` / 重定向塞进 `cmd /C` 的单个参数：
//! 那样 cmd 的引号剥离规则会让脚本变成另一条命令（第一版就是这么翻车的）。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::prelude::*;

use unlucky_core::launch::tree_kill;
use unlucky_core::launch::watchdog::WatchdogConfig;

fn launcher(sessions_root: &Path, startup: Duration, grace: Duration) -> Launcher {
    let mut config = LauncherConfig::new(sessions_root);
    config.platform = PlatformCtx::new(OsFamily::Windows, "x86_64");
    config.process = ProcessConfig {
        graceful_stop: Duration::from_millis(200),
        // 泵的预算压到 200ms：放弃等待之后不该再为「等日志落盘」多等默认的 10 秒。
        watchdog: WatchdogConfig {
            pump_timeout: Duration::from_millis(200),
        },
        startup_timeout: startup,
        shutdown_grace: grace,
        ..ProcessConfig::default()
    };
    Launcher::new(config)
}

/// 写一个伪装成 java 的脚本，返回它的路径。
fn script(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
    #[cfg(windows)]
    let (file, shebang) = (dir.join(format!("{name}.bat")), "@echo off");
    #[cfg(not(windows))]
    let (file, shebang) = (dir.join(format!("{name}.sh")), "#!/bin/sh");
    let body = format!("{shebang}\n{}\n", lines.join("\n"));
    std::fs::write(&file, body).expect("写测试脚本");
    file
}

/// 用脚本文件当 java（命令行组装、事件流、退出判定都是真的，只有 JVM 是假的）。
fn request(script_path: &Path, run_dir: &Path) -> LaunchRequest {
    let (mut request, _) = legacy_forge_request();
    #[cfg(windows)]
    {
        request.wrapper = vec![
            "cmd".to_string(),
            "/C".to_string(),
            script_path.display().to_string(),
        ];
    }
    #[cfg(not(windows))]
    {
        request.wrapper = vec!["/bin/sh".to_string(), script_path.display().to_string()];
    }
    request.java = JavaSelection::new("fake-java", 17, 64);
    request.manifest.legacy_arguments = None;
    request.dirs.game_dir = run_dir.to_path_buf();
    request.dirs.natives_dir = run_dir.join("natives");
    request.user.jvm = Vec::new();
    request.user.game = Vec::new();
    request
}

/// 进程是否还活着（看门狗「不杀进程」的判据）。
fn is_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .expect("tasklist 应可执行");
        String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

/// 一条「活着但什么也不干」的脚本（约 3 秒）。
fn linger_lines() -> Vec<&'static str> {
    #[cfg(windows)]
    {
        vec!["ping -n 4 127.0.0.1 >nul"]
    }
    #[cfg(not(windows))]
    {
        vec!["sleep 3"]
    }
}

/// 先打一行窗口标记，再活 1 秒左右。
fn window_then_exit_lines() -> Vec<&'static str> {
    #[cfg(windows)]
    {
        vec!["echo LWJGL Version: 3.3.1", "ping -n 2 127.0.0.1 >nul"]
    }
    #[cfg(not(windows))]
    {
        vec!["echo 'LWJGL Version: 3.3.1'", "sleep 1"]
    }
}

/// 每秒打一行、总共约 4 秒的脚本（**有**日志、**没有**窗口标记）。
#[cfg(windows)]
fn chatty_lines() -> Vec<&'static str> {
    // `ping` 每秒打一行；不重定向就是为了让它真的输出。
    vec!["ping -n 5 127.0.0.1"]
}

/// 同上（POSIX）。
#[cfg(not(windows))]
fn chatty_lines() -> Vec<&'static str> {
    vec!["i=0; while [ $i -lt 5 ]; do echo \"tick $i\"; sleep 1; i=$((i+1)); done"]
}

#[test]
fn a_chatty_slow_load_is_not_mistaken_for_a_hang() {
    // 一直没有窗口，但每秒都有一行日志：静默预算 1.5s，总时长 4s+ ⇒ 不能被判死。
    let sessions = temp_sessions_root("watchdog-chatty");
    let run_dir = temp_dir("watchdog-chatty-run");
    let path = script(&run_dir, "chatty", &chatty_lines());
    let launcher = launcher(
        &sessions,
        Duration::from_millis(1500),
        Duration::from_secs(60),
    );
    let session = launcher
        .spawn(&request(&path, &run_dir))
        .expect("spawn 应成功");
    let started = Instant::now();
    let handle = session.finish().expect("还在打日志就不算卡死");
    assert!(
        started.elapsed() > Duration::from_millis(1500),
        "这条用例要真的越过静默预算，否则什么都没验到：{:?}",
        started.elapsed()
    );
    assert_eq!(handle.exit.code, 0);
    assert_eq!(handle.exit.exit_type, ExitType::Normal);
    assert!(
        !handle
            .index
            .as_ref()
            .is_some_and(|entry| entry.window_detected),
        "这条脚本不该命中窗口启发式"
    );
}

/// 先打关闭标记，再挂住不走（约 9 秒）。
fn shutdown_then_hang_lines() -> Vec<&'static str> {
    #[cfg(windows)]
    {
        vec![
            "echo [Client thread/INFO]: Stopping!",
            "ping -n 10 127.0.0.1 >nul",
        ]
    }
    #[cfg(not(windows))]
    {
        vec!["echo '[Client thread/INFO]: Stopping!'", "sleep 9"]
    }
}

#[test]
fn startup_timeout_stops_waiting_and_never_kills_the_process() {
    let sessions = temp_sessions_root("watchdog-startup");
    let run_dir = temp_dir("watchdog-startup-run");
    let path = script(&run_dir, "linger", &linger_lines());
    let launcher = launcher(
        &sessions,
        Duration::from_millis(300),
        Duration::from_secs(60),
    );
    let session = launcher
        .spawn(&request(&path, &run_dir))
        .expect("spawn 应成功");
    let pid = session.pid();
    let log_path = session.log_path().to_path_buf();
    let started = Instant::now();
    let error = session
        .finish()
        .expect_err("没窗口也没退出：必须停止等待并报错，不能无限等");
    let spent = started.elapsed();
    assert!(
        spent < Duration::from_secs(5),
        "超时后应立即返回：{spent:?}"
    );
    match &error {
        LaunchError::Watchdog {
            pid: reported,
            reason,
            limit_ms,
            ..
        } => {
            assert_eq!(*reported, pid);
            assert_eq!(*reason, "startup_timeout");
            assert_eq!(*limit_ms, 300, "上限要如实回报（可配 + 可见）");
        }
        other => panic!("应报看门狗超时，实际 {other:?}"),
    }
    let text = error.to_string();
    assert!(text.contains("没有被结束"), "{text}");
    // M220 nit1：这句话直接打到终端，残留的 `**` 标记在终端里只会原样显示成星号。
    assert!(
        !text.contains("**"),
        "人读错误串里不该有 Markdown 标记：{text}"
    );
    assert!(text.contains("ULL_LAUNCH_STARTUP_TIMEOUT_SECS"), "{text}");

    // 会话照样收尾（历史里看得见这一局），原因是 still_running 而不是 exit。
    let events = read_events(&log_path);
    let last = events.last().expect("至少写了 session_start");
    assert_eq!(last["kind"], "session_end");
    assert_eq!(last["payload"]["reason"], "still_running");

    // 关键断言：进程还活着 —— 看门狗只停止等待，绝不结束进程。
    assert!(is_alive(pid), "看门狗不该结束进程（pid {pid} 已经不在了）");
    let _ = tree_kill::force_kill(pid);
}

#[test]
fn a_seen_window_keeps_the_wait_alive_for_slow_loads() {
    // 启动预算是 300ms，但窗口标记在几十毫秒内就到了：慢加载不是卡死。
    let sessions = temp_sessions_root("watchdog-window");
    let run_dir = temp_dir("watchdog-window-run");
    let path = script(&run_dir, "windowed", &window_then_exit_lines());
    let launcher = launcher(
        &sessions,
        Duration::from_millis(300),
        Duration::from_secs(60),
    );
    let session = launcher
        .spawn(&request(&path, &run_dir))
        .expect("spawn 应成功");
    let started = Instant::now();
    let handle = session
        .finish()
        .expect("窗口出现过之后就该一直等到它自己退出");
    assert!(
        started.elapsed() > Duration::from_millis(300),
        "这条用例要真的越过启动预算，否则什么都没验到"
    );
    assert_eq!(
        handle.exit.exit_type,
        ExitType::Normal,
        "{}",
        handle.summary()
    );
    assert_eq!(handle.exit.code, 0);
    assert!(
        handle
            .index
            .as_ref()
            .is_some_and(|entry| entry.window_detected),
        "窗口检测要落进索引（GUI/诊断都看这一位）"
    );
}

#[test]
fn a_shutdown_marker_followed_by_hang_is_reported_as_stuck_on_close() {
    // 实机 D6：关窗后 java.exe 仍存活。游戏自己打了「Stopping!」，之后就再没动静。
    let sessions = temp_sessions_root("watchdog-shutdown");
    let run_dir = temp_dir("watchdog-shutdown-run");
    let path = script(&run_dir, "hanging", &shutdown_then_hang_lines());
    let launcher = launcher(
        &sessions,
        Duration::from_secs(60),
        Duration::from_millis(400),
    );
    let session = launcher
        .spawn(&request(&path, &run_dir))
        .expect("spawn 应成功");
    let pid = session.pid();
    let started = Instant::now();
    let error = session
        .finish()
        .expect_err("关闭标记之后挂住：必须停止等待并报错");
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "宽限一到就要返回：{:?}",
        started.elapsed()
    );
    match &error {
        LaunchError::Watchdog { reason, .. } => assert_eq!(*reason, "shutdown_timeout"),
        other => panic!("应报卡在关闭，实际 {other:?}"),
    }
    assert!(error.to_string().contains("已开始关闭"), "{error}");
    let _ = tree_kill::force_kill(pid);
}

#[test]
fn a_quick_clean_exit_is_never_punished_by_the_watchdog() {
    let sessions = temp_sessions_root("watchdog-quick");
    let run_dir = temp_dir("watchdog-quick-run");
    let path = script(&run_dir, "quick", &["echo hello"]);
    let launcher = launcher(&sessions, Duration::from_secs(3), Duration::from_secs(3));
    let session = launcher
        .spawn(&request(&path, &run_dir))
        .expect("spawn 应成功");
    let handle = session.finish().expect("正常退出不该报错");
    assert_eq!(handle.exit.code, 0);
    assert_eq!(handle.exit.exit_type, ExitType::Normal);
    assert!(!handle.exit.terminated_externally);
}

/// M220 nit1：`launcher.log` 与会话输出里的这句「看门狗生效值」是给人看的，
/// 残留的 `**` 标记在终端里只会原样显示成星号（旧文案就是「停止等待**不结束进程**」）。
#[test]
fn the_watchdog_summary_is_plain_text() {
    let summary = LauncherConfig::new(std::env::temp_dir()).watchdog_summary();
    assert!(summary.contains("不结束进程"), "{summary}");
    assert!(
        !summary.contains("**"),
        "人读串里不该有 Markdown 标记：{summary}"
    );
}
