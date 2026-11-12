//! 失败卡片的三个动作：导出诊断包 / 打开文件夹 / 复制路径。
//!
//! 与 [`super`] 分开是因为职责不同：那边管**弹窗与事实**，这边管**把事实带出界面**。
//! 打包本体在 `unlucky_core::launch::crash_bundle`（脱敏、上限、清单都在那一处），
//! 本文件只负责「放哪、叫什么名字、怎么打开」。

use std::path::PathBuf;

use unlucky_core::launch::crash_bundle::{self, BundleEntry, BundleSpec};
use unlucky_core::launch::crash_report::CrashReport;
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;

/// 一次导出的结论（**成功也带事实**：路径、字节数、有没有文件被截断）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Exported {
    /// 给用户看的那一句（已本地化）。
    pub text: String,
    /// 是否成功。
    pub ok: bool,
    /// 有几个文件被截断（`>0` 时界面要明说「证据是残的」）。
    pub truncated: usize,
}

/// 导出诊断包：打到「启动器日志目录 / exports / 一场一个文件名」。
///
/// 返回的那句话说清**落到哪、多少字节**（用户要的第一件事就是「文件在哪」）；
/// 有文件被截断时再补一句——别让用户以为包里的证据是完整的。
pub(crate) fn export(report: &CrashReport, lang: Lang) -> Exported {
    let catalog = Catalog::of(lang);
    let Some(logs) = &report.launcher_logs else {
        return Exported {
            text: catalog.render(
                Key::CrashDialogExportFailed,
                &[("error", "no launcher log dir")],
            ),
            ok: false,
            truncated: 0,
        };
    };
    let stamp =
        unlucky_core::launch::time::format_rfc3339_millis(unlucky_core::launch::time::now())
            .replace([':', '.', 'T'], "-")
            .trim_end_matches('Z')
            .to_string();
    let name = if report.session.is_empty() {
        &report.instance
    } else {
        &report.session
    };
    let target = crash_bundle::default_target_dir(logs).join(format!("crash-{name}-{stamp}.zip"));
    match crash_bundle::write_bundle(&spec_of(report), &target) {
        Ok(bundle) => {
            let mut text = catalog.render(
                Key::CrashDialogExportDone,
                &[
                    ("bytes", &bundle.bytes.to_string()),
                    ("path", &bundle.path.display().to_string()),
                ],
            );
            if bundle.truncated > 0 {
                text.push('\n');
                text.push_str(&catalog.render(
                    Key::CrashDialogExportTruncated,
                    &[("count", &bundle.truncated.to_string())],
                ));
            }
            Exported {
                text,
                ok: true,
                truncated: bundle.truncated,
            }
        }
        Err(error) => Exported {
            text: catalog.render(Key::CrashDialogExportFailed, &[("error", &error)]),
            ok: false,
            truncated: 0,
        },
    }
}

/// 现场 → 诊断包清单（**进包的东西必须与弹窗上列出的路径一致**）。
fn spec_of(report: &CrashReport) -> BundleSpec {
    let mut entries: Vec<BundleEntry> = Vec::new();
    let push = |entries: &mut Vec<BundleEntry>, group: &'static str, path: &std::path::Path| {
        if path.exists() {
            entries.push(BundleEntry::new(group, path));
        }
    };
    if let Some(logs) = &report.launcher_logs {
        push(&mut entries, "launcher", &logs.join("launcher.log"));
        push(&mut entries, "launcher", &logs.join("launcher.1.log"));
        push(&mut entries, "launcher", &logs.join("install.log"));
    }
    if let Some(session) = &report.session_log {
        // 源文件名就是会话 id：钉成 `session.jsonl`，包内名与清单名同时不再泄漏 id，
        // 也不会出现「清单写 <redacted:token> 而包里是原始 id」（r1 复审 P2-2）。
        if session.exists() {
            entries.push(BundleEntry::named("session", session, "session.jsonl"));
        }
    }
    for artifact in &report.artifacts {
        // `latest.log` 在下面单独收一次（它同时是扫描结果与固定项，重复收会变成两份）。
        if artifact.source != "latest_log" {
            push(&mut entries, "game", &artifact.path);
        }
    }
    if let Some(latest) = report.latest_log_path() {
        push(&mut entries, "game", &latest);
    }
    if let Some(json) = &report.version_json {
        push(&mut entries, "instance", json);
    }
    let mut secrets = vec![report.session.clone()];
    secrets.retain(|secret| !secret.is_empty());
    BundleSpec {
        instance: report.name.clone(),
        version: report.version.clone(),
        session: report.session.clone(),
        exit_code: report.exit_code,
        exit_type: report.exit_type.clone(),
        uptime_ms: report.uptime_ms,
        command: report.command.clone(),
        java: report.java.clone(),
        system: report.system.clone(),
        notes: report.note_lines(),
        missing: report.missing_lines(),
        entries,
        secrets,
        home: std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .ok()
            .map(PathBuf::from),
    }
}

/// 打开「日志文件夹」：有现场就开实例运行目录，否则开启动器日志目录。
///
/// 平台差异只在这里一处（`explorer` / `open` / `xdg-open`），不往业务代码里散写 `cfg`。
pub(crate) fn open_folder(report: &CrashReport, lang: Lang) -> String {
    let catalog = Catalog::of(lang);
    let target = report
        .run_dir
        .clone()
        .filter(|dir| dir.is_dir())
        .or_else(|| report.launcher_logs.clone().filter(|dir| dir.is_dir()));
    let Some(dir) = target else {
        return catalog.render(
            Key::CrashDialogOpenFailed,
            &[("error", "no folder on disk")],
        );
    };
    match open_dir(&dir) {
        Ok(()) => String::new(),
        Err(error) => catalog.render(Key::CrashDialogOpenFailed, &[("error", &error)]),
    }
}

/// 打开一个目录（M188：设置页/日志页的「打开日志文件夹」也走这里，**不再另写一套 `cfg`**）。
///
/// 失败文本带完整路径：调用方把它显示/记日志出来，用户至少能照着路径去找。
pub(crate) fn open_dir(dir: &std::path::Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(format!("{}（目录不存在）", dir.display()));
    }
    spawn_opener(dir).map_err(|error| format!("{}（打开失败：{error}）", dir.display()))
}

/// 平台各自的「打开这个目录」实现。
///
/// ## 为什么这三处**故意**不走 `unlucky_core::platform::child::quiet`（M217）
///
/// 这一处 spawn 的**目的就是让桌面上出现一个可见的东西**（文件管理器窗口 / Finder 窗口 /
/// 桌面环境的文件管理器）。`quiet` 的语义是「把子进程的窗口抑制掉」，用在这里是把目的本身
/// 抑制掉——改错了用户点「打开文件夹」就只看到「打不开」的报错。
///
/// 而且从窗口角度也不需要它：`explorer.exe` / `open` / `xdg-open` 自己都是 GUI 侧程序，由
/// 无控制台的 GUI 父进程 spawn 时 Windows 不会给它们新建控制台。所以这三处在
/// `crates/core/tests/child_quiet_guard.rs` 的 `ALLOWED_ROLES` 里登记成「目录打开器」角色，
/// **不是**漏网。别把它们改成 `quiet`。
#[cfg(target_os = "windows")]
fn spawn_opener(dir: &std::path::Path) -> Result<(), String> {
    // `explorer` 不用 shell：路径里的空格与 `&` 不会被当成命令分隔符。
    std::process::Command::new("explorer")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn spawn_opener(dir: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn spawn_opener(dir: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// 「复制路径」的一段文本（每条一行，标签 + 路径）。
pub(crate) fn paths_text(report: &CrashReport) -> String {
    report
        .path_lines()
        .into_iter()
        .map(|(label, path)| format!("{label}: {}", path.display()))
        .collect::<Vec<_>>()
        .join("\n")
}
