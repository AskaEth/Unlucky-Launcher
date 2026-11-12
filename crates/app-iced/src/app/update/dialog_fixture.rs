//! 启动失败弹窗的**取证夹具**（`ULL_DIALOG_FIXTURE`，只在 `--screenshot` 下生效）。
//!
//! ## 为什么要有它
//!
//! 弹窗要出两张图（「有 crash-report」/「只有捕获输出」），而真机上要复现一次崩溃得先让一个
//! 真实的 MC 进程死掉——截图通道要的是**确定性**。做法与 `ULL_ACCOUNT_STATE` 同一档：
//! 在临时目录里造一份**真的文件树**（launcher.log / 会话 JSONL / crash-reports / latest.log），
//! 于是弹窗上的路径是真路径、点「导出诊断包」真的会打出一个装得下这些文件的 zip。
//!
//! 取值：
//!
//! | 值 | 现场 |
//! |---|---|
//! | `crash-report` | 游戏写了 `crash-reports/crash-*.txt`（证据一） |
//! | `no-logs` | 没有 crash-report、没有 latest.log，只有会话 JSONL 里的 stderr（证据二） |
//! | `export` | 同 `no-logs`，并在出图前**真的跑一次导出**（第三张图：路径 + 字节数） |
//!
//! 夹具只影响这一次进程；产品路径（`host/launch.rs` 收尾 → `crash_report::publish` →
//! `refresh_view` → 弹窗）与它完全同一条，这里只是**代填事实**。

use std::path::{Path, PathBuf};

use unlucky_core::launch::crash_report::{self, ArtifactFact, CrashReport};
use unlucky_core::launch::crash_scan::{self, CrashSource};
use unlucky_presenter::dto::{LaunchFinished, LogOutcome};

use crate::app::state::App;

/// 现场根（`%TEMP%/ull-m187-fixture-<pid>`）。
fn root() -> PathBuf {
    std::env::temp_dir().join(format!("ull-m187-fixture-{}", std::process::id()))
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, text);
}

/// 出图用的夹具（`None` = 没有开关，什么都不做）。
pub(super) fn apply(app: &mut App) -> Option<String> {
    let mode = std::env::var("ULL_DIALOG_FIXTURE").ok()?;
    let mode = mode.trim().to_lowercase();
    if !app.has_screenshot() {
        return None;
    }
    let root = root();
    let _ = std::fs::remove_dir_all(&root);
    let logs = root.join("logs");
    write(
        &logs.join("launcher.log"),
        "2026-11-08T15:30:11.002Z [boot] renderer=wgpu\n\
         2026-11-08T15:30:12.441Z [launch] instance=survival spawn game_dir=… java=javaw (21)\n\
         2026-11-08T15:30:19.870Z [launch] instance=survival session=9f2c exit=1\n",
    );
    write(&logs.join("install.log"), "installed 1.20.4-fabric\n");

    let session = "9f2c4a1e-77bd-4f27-9d21-5b0a1f2c3d4e";
    let session_log = root
        .join("sessions")
        .join("game")
        .join(format!("{session}.jsonl"));
    write(
        &session_log,
        "{\"kind\":\"stdout\",\"payload\":{\"line\":\"[main/INFO]: Loading Minecraft 1.20.4\"}}\n\
         {\"kind\":\"stderr\",\"payload\":{\"line\":\"java.lang.RuntimeException: mixin injection failed\"}}\n\
         {\"kind\":\"stderr\",\"payload\":{\"line\":\"Caused by: java.lang.NoClassDefFoundError: org/lwjgl/glfw/GLFW\"}}\n\
         {\"kind\":\"exit\",\"payload\":{\"code\":1}}\n",
    );
    let tail: Vec<String> = vec![
        "[main/INFO]: Loading Minecraft 1.20.4".to_string(),
        "java.lang.RuntimeException: mixin injection failed".to_string(),
        "Caused by: java.lang.NoClassDefFoundError: org/lwjgl/glfw/GLFW".to_string(),
    ];

    let run_dir = root.join("instances").join("survival");
    let latest = run_dir.join("logs").join("latest.log");
    let version_json = root
        .join("game")
        .join("versions")
        .join("1.20.4-fabric")
        .join("1.20.4-fabric.json");
    write(&version_json, "{\"id\":\"1.20.4-fabric\"}\n");

    let mut artifacts: Vec<ArtifactFact> = Vec::new();
    if mode != "no-logs" {
        write(
            &run_dir
                .join("crash-reports")
                .join("crash-2026-11-08_15.30.19-client.txt"),
            "---- Minecraft Crash Report ----\n\
             Time: 2026-11-08 15:30:19\n\
             Description: Initializing game\n\n\
             java.lang.RuntimeException: mixin injection failed\n\
             \tat net.fabricmc.loader.impl.launch.knot.Knot.init(Knot.java:142)\n\
             \tat net.minecraft.client.main.Main.main(Main.java:51)\n",
        );
        write(&latest, "[15:30:12] [main/INFO]: Loading Minecraft 1.20.4\n[15:30:19] [Render thread/ERROR]: GLFW init failed\n");
        let since = std::time::SystemTime::now() - std::time::Duration::from_secs(120);
        artifacts = crash_scan::scan(&run_dir, since, 8)
            .into_iter()
            .map(|artifact| ArtifactFact {
                source: artifact.source.as_str().to_string(),
                path: artifact.path,
                excerpt: artifact.excerpt,
            })
            .collect();
    }

    let report = CrashReport {
        instance: "survival".to_string(),
        name: "生存服的 1.20.4".to_string(),
        version: "1.20.4-fabric".to_string(),
        session: session.to_string(),
        session_log: Some(session_log),
        version_json: Some(version_json),
        launcher_logs: Some(logs),
        run_dir: Some(run_dir),
        artifacts,
        exit_code: Some(1),
        exit_type: "application_error".to_string(),
        uptime_ms: 7_400,
        command: "javaw -Xmx4096m -Djava.library.path=<home>… ".to_string(),
        java: "C:/Program Files/Java/jdk-21/bin/javaw.exe (major 21)".to_string(),
        system: "os=windows version=10.0.26100 arch=x86_64".to_string(),
        stage: None,
        stage_detail: None,
        stderr_tail: tail,
    };
    crash_report::publish(report);
    let finished = LaunchFinished {
        id: "survival".to_string(),
        session: session.to_string(),
        outcome: LogOutcome::Crashed,
        exit_code: Some(1),
        uptime_ms: 7_400,
        failure: None,
    };
    app.open_failure_dialog(&finished);
    if mode == "export" {
        // 走**真的**那一条：与用户点「导出诊断包」同一个处理函数。
        let _ = super::dialog::on_dialog(app, crate::app::dialog::Message::Primary);
    }
    Some(mode)
}

/// 夹具里崩溃报告的来源名（与 `CrashSource::CrashReport` 同源，测试用）。
#[allow(dead_code)]
pub(super) fn crash_report_source() -> &'static str {
    CrashSource::CrashReport.as_str()
}
