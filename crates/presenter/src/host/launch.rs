//! 启动全链：实例 → `LaunchRequest` → 子进程 → JSONL 事件流（GUI 版）。
//!
//! 与 CLI `ull launch` 同源同序（`core` 的 `game::resolve` / `arguments.expand` /
//! `launch::adapter::build_request` / `Launcher::spawn`），差异只有两处：
//! - **不阻塞 UI**：装配与等待都在后台线程里，进度/日志经 [`super::job::HostTask`] 回流；
//! - **身份来自 GUI 选中的账户**（`AccountStore` → `AuthPlaceholders`），凭据只在
//!   本模块内被读一次，绝不进 DTO。
//!
//! 停止：`kill_tree(pid)`（core 的三段式进程树回收），`finish()` 会照常收尾写索引。
//!
//! M298 把装配那一段（`build_request` 与它的两个私有助手）搬到 [`build`]，`pub use` 保路径：
//! 本文件只留「起一次启动 / 停一次启动 / 失败出口怎么登记现场」这条生命周期。

use unlucky_core::game::isolation::IsolationMode;
use unlucky_core::launch::crash_report::{self, CrashReport, ReportContext, SessionAttribution};
use unlucky_core::launch::launcher_log;
use unlucky_core::launch::tree_kill;
use unlucky_core::launch::{Launcher, LauncherConfig};

use crate::dto::{
    InstanceId, InstanceRuntime, LaunchFailureStage, LaunchFailureView, LaunchFinished,
    LaunchStage, LogOutcome, Progress,
};

use super::env::HostEnv;

mod accept;
mod build;
mod install;
mod java;
mod target;

use super::job::{HostTask, JobSender};
// `build_request`（自己挑一遍再装配）只给装配链的单测用；真启动走 `build_request_with`。
#[cfg(test)]
pub use build::build_request;
pub use build::build_request_with;
pub use install::spawn_install_major;
pub use java::{pick_java, JavaPick};
pub(super) use target::{resolve, Rejection};

/// 启动器名（与 CLI `meta::LAUNCHER_NAME` 同值）。
pub const LAUNCHER_NAME: &str = "Unlucky Launcher";

/// 一次启动需要的实例信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchTarget {
    /// 实例 id
    pub id: InstanceId,
    /// 实例名
    pub name: String,
    /// 绑定的版本 id
    pub version_id: String,
    /// 账户 id（`None` = 离线占位）
    pub account: Option<String>,
}

/// 起一次启动（后台线程；进度/日志/收尾全部走 `HostTask`）。
///
/// `java` 是**受理那一刻**就挑好并判过的结果（M331）：装配期直接用它，不再挑第二次——
/// 既省一次 `java -version` 探测，也保证「拦下时用的要求」与「真启动时用的 Java」是同一份。
pub fn start(env: HostEnv, tx: JobSender, target: LaunchTarget, java: JavaPick) {
    let id = target.id.clone();
    let tx_progress = tx.clone();
    super::job::spawn("launch", move || {
        // M187：启动器自己的运行日志从这里开始有落点（早于这一拍的行由 core 缓冲接住）。
        // M188 起 presenter 在构造 `HostEnv` 时已经登记过目录（缓冲那时就刷出去了），这里是
        // 幂等的一次兜底：数据根被 `redirect.json` 改过时以最后一次为准。
        let logs_dir = env.layout.logs();
        launcher_log::set_dir(&logs_dir);
        let started_at = std::time::SystemTime::now();
        // 崩溃报告里的「游戏目录」必须是**这个实例自己那个根**（M274）：官方根里的实例崩了，
        // 报告却写着 ULL 便携根的路径，等于把排障指到别人的目录上。
        let game = super::game::game_for(&env, &target.id);
        let mut report = ReportContext::new(
            &target.id,
            &target.name,
            &target.version_id,
            game.root(),
            &logs_dir,
            started_at,
            unlucky_core::platform::os_info::current().summary_line(),
        );
        let _ = tx_progress.send(HostTask::Runtime {
            id: id.clone(),
            runtime: InstanceRuntime::Busy {
                stage: LaunchStage::Launch,
                progress: Progress { done: 0, total: 1 },
            },
        });
        let (request, warnings) = match build_request_with(&env, &target, java) {
            Ok(ok) => ok,
            Err(failure) => {
                let line = format!("instance={} 装配失败：{}", target.id, failure.detail);
                launcher_log::record("launch", &line);
                // 归因是 `None`（进程没起来），但**照样用返回值**填 DTO：单一来源是结构，
                // 不是「两边碰巧都写空串」（复审 N1）。
                let session =
                    publish_before_start(&report, None, failure.stage.slug(), &failure.detail);
                let _ = tx.send(HostTask::LaunchEnd(Box::new(LaunchFinished {
                    id,
                    session,
                    outcome: LogOutcome::Crashed,
                    exit_code: None,
                    uptime_ms: 0,
                    failure: Some(failure),
                })));
                return;
            }
        };
        report.run_dir = request.dirs.game_dir.clone();
        report.version_json = Some(game.version_json(&target.version_id));
        report.java = launcher_log::java_label(&request.java.binary, request.java.major);
        for warning in &warnings {
            let _ = tx.send(HostTask::Notice(Box::new(crate::dto::NoticeView::new(
                &format!("launch-warn:{}", target.id),
                crate::dto::NoticeLevel::Info,
                "启动提示",
                warning,
            ))));
        }
        // 命令行（脱敏版）进启动器日志：事故之后「到底用什么参数起的」是第一个要看的。
        // M211/D5：`ull launch` 写同一个文件的同一行（格式在 core 的 `launcher_log` 里）。
        launcher_log::record_start(
            &target.id,
            &report.run_dir,
            Some((request.java.binary.as_path(), request.java.major)),
            request.memory.max_mb,
        );
        let config = LauncherConfig::new(env.sessions_root())
            .with_launcher(LAUNCHER_NAME, env!("CARGO_PKG_VERSION"));
        let session = match Launcher::new(config).spawn(&request) {
            Ok(session) => session,
            Err(error) => {
                let failure = LaunchFailureView::new(LaunchFailureStage::Spawn, error.to_string());
                launcher_log::record(
                    "launch",
                    &format!("instance={} spawn 失败：{error}", target.id),
                );
                let session = publish_before_start(&report, None, "spawn", &failure.detail);
                let _ = tx.send(HostTask::LaunchEnd(Box::new(LaunchFinished {
                    id,
                    session,
                    outcome: LogOutcome::Crashed,
                    exit_code: None,
                    uptime_ms: 0,
                    failure: Some(failure),
                })));
                return;
            }
        };
        let session_id = session.session_id().to_string();
        let pid = session.pid();
        // **归属的唯一一份**（M298）：收尾失败那条出口要把「这次失败属于哪一场」写进现场，
        // 收尾事件也要带同一个值——两处都从这里取，界面侧的并发守卫才可能判对。
        let session_log = session.log_path().to_path_buf();
        let attribution = SessionAttribution {
            id: &session_id,
            log: Some(session_log.as_path()),
        };
        let _ = tx.send(HostTask::LaunchStarted {
            id: id.clone(),
            session_id: session_id.clone(),
            pid,
        });
        let _ = tx.send(HostTask::Runtime {
            id: id.clone(),
            runtime: InstanceRuntime::Running { pid },
        });
        match session.finish() {
            Ok(handle) => {
                let outcome = super::identity::outcome_of(&handle);
                launcher_log::record_end(&target.id, &handle);
                report.secrets.push(session_id.clone());
                let lines: Vec<String> = super::logs::read_lines(&handle.log_path)
                    .into_iter()
                    .map(|line| line.text)
                    .collect();
                crash_report::publish(CrashReport::collect(&handle, &report, &lines));
                let _ = tx.send(HostTask::LaunchEnd(Box::new(LaunchFinished {
                    id,
                    // 与 `CrashReport::collect` 取自同一处（`handle.session_id`）。
                    session: handle.session_id.clone(),
                    outcome,
                    exit_code: Some(handle.exit.code),
                    uptime_ms: handle.uptime_ms,
                    // 跑起来再崩不算「装配失败」：结局由 `outcome` 表达，这里留 `None`。
                    failure: None,
                })));
            }
            Err(error) => {
                let failure = LaunchFailureView::new(LaunchFailureStage::Spawn, error.to_string());
                let line = format!(
                    "instance={} session={session_id} 收尾失败：{error}",
                    target.id
                );
                launcher_log::record("launch", &line);
                // **收尾失败时进程真的起来过** ⇒ 这份现场属于 `attribution` 那一场。
                // 返回值就是报告里写的那条会话 id（同一处取值），所以「报告空串 vs 事件非空」
                // 这种守卫必然误判的组合从构造上就不存在了（M298）。
                let session =
                    publish_before_start(&report, Some(&attribution), "spawn", &failure.detail);
                let _ = tx.send(HostTask::LaunchEnd(Box::new(LaunchFinished {
                    id,
                    session,
                    outcome: LogOutcome::SilentFail,
                    exit_code: None,
                    uptime_ms: 0,
                    failure: Some(failure),
                })));
            }
        }
    });
}

/// 登记一次**没有 handle** 的失败现场，并返回该写进 `LaunchFinished.session` 的值。
///
/// **会话归属的单一来源**（M298）：报告里的 `session` 与收尾事件里的 `session` 都由
/// `attribution` 这一处产出——调用方直接拿返回值填 DTO，于是两处不会再各写一遍、也不会出现
/// 「报告写空串、事件写真实会话」这种守卫必然误判的组合。
///
/// `attribution` 为 `None` = 这一场**确实**没有会话（装配失败 / spawn 失败，进程没起来）。
fn publish_before_start(
    report: &ReportContext,
    attribution: Option<&SessionAttribution<'_>>,
    stage: &str,
    detail: &str,
) -> String {
    let registered = CrashReport::before_start(report, stage, detail, stage, attribution.copied());
    let session = registered.session.clone();
    crash_report::publish(registered);
    session
}

/// 停止一次启动（按 pid 回收进程树；`finish()` 会在后台线程里照常收尾）。
pub fn stop(pid: u32) -> Result<(), String> {
    tree_kill::kill_tree(pid, std::time::Duration::from_secs(3))
        .map_err(|error| format!("停止进程 {pid} 失败：{error}"))
}

/// 隔离档位的展示串（`isolated` / `shared`）。
pub fn isolation_label(mode: IsolationMode) -> &'static str {
    mode.as_str()
}

#[cfg(test)]
mod fixtures;

#[cfg(test)]
mod guard_tests;

#[cfg(test)]
mod legacy_tests;

#[cfg(test)]
mod m334_install_tests;

#[cfg(test)]
mod m334_real_tests;

#[cfg(test)]
mod m334_tests;

#[cfg(test)]
mod revalidate_tests;

#[cfg(test)]
mod target_tests;

#[cfg(test)]
mod tests;
