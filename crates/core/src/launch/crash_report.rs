//! 一次失败启动的**现场**（M187）：路径 + 捕获输出 + 退出事实，供失败弹窗与诊断包共用。
//!
//! ## 为什么是「事实」而不是「结论」
//!
//! 弹窗要说三件事——**是哪一场失败**（装配阶段 / 起进程后崩溃）、**退出码**、**证据在哪**。
//! 这三件都是启动器已经知道的事实，不含任何推断。崩溃原因的智能诊断是另一条线（诊断引擎
//! `diagnose` / AI），本模块只把事实摆齐，让用户能一键把证据带走。
//!
//! ## 两种失败形态必须分开（用户投诉的原点）
//!
//! | 形态 | 判据 | 弹窗该说什么 |
//! |---|---|---|
//! | 游戏自己写了崩溃报告 | [`CrashReport::has_game_crash_report`] 为真 | 「游戏写出了崩溃报告」，贴 Description/栈片段 |
//! | 游戏**没来得及写** | 没有 crash-report，但有捕获输出 | 「这次没有生成崩溃报告，下面是捕获的最后若干行」 |
//! | 什么输出都没抓到 | 两者皆无 | 「游戏还没来得及写日志」+ 最可能原因分类（缺 Java / 内存 / 参数 / 库） |
//!
//! 第二、三种**不许**显示成「找不到日志」——日志在启动器这里，只是游戏没写它自己那份。
//! 进程级槽位只存**已脱敏**的文本（[`CrashReport::collect`] 的出口就是脱敏边界）。

use std::path::PathBuf;

use super::crash_scan::{self, CrashSource};
use super::launch_session::SessionHandle;

mod context;
mod slot;

pub use context::ReportContext;
pub use slot::{clear, failing_launch, publish};

use context::{fact, scrub_tail};

/// 弹窗里贴「捕获输出」的行数上限（够看清 JVM 报错，又不把弹窗撑破）。
pub const TAIL_LINES: usize = 30;
/// 现场文件的扫描条数上限。
pub const MAX_ARTIFACTS: usize = 8;
/// 弹窗里贴崩溃报告摘要的字符上限。
pub const MAX_EXCERPT_CHARS: usize = 1200;

/// 一个现场文件（来源 + 路径 + 人读摘要）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactFact {
    /// 来源 slug（`crash_report` / `hs_err` / `latest_log`）。
    pub source: String,
    /// 文件路径。
    pub path: PathBuf,
    /// 摘要（崩溃报告的 Description + 栈片段；`latest.log` 与 `hs_err` 不给）。
    pub excerpt: Option<String>,
}

/// 一次失败启动的现场。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrashReport {
    /// 实例 id。
    pub instance: String,
    /// 实例名。
    pub name: String,
    /// 绑定的版本 id。
    pub version: String,
    /// 会话 id（空 = 进程根本没起来）。
    pub session: String,
    /// 会话事件流 JSONL（**游戏侧证据的真正落点**）。
    pub session_log: Option<PathBuf>,
    /// 该实例绑定的版本 JSON（诊断包要带上它，别人才能复现装配）。
    pub version_json: Option<PathBuf>,
    /// 启动器自己的日志目录（`.ULL/logs`）。
    pub launcher_logs: Option<PathBuf>,
    /// 实例运行目录（`crash-reports/` 与 `logs/` 的父目录）。
    pub run_dir: Option<PathBuf>,
    /// 崩溃现场文件（按修改时间倒序）。
    pub artifacts: Vec<ArtifactFact>,
    /// 退出码（`None` = 进程没起来 / 平台没给）。
    pub exit_code: Option<i32>,
    /// 退出判定 slug（`application_error` / `jvm_error` / `normal` …；进程没起来时是 `spawn`）。
    pub exit_type: String,
    /// 存活毫秒数。
    pub uptime_ms: u64,
    /// 脱敏后的命令行一行。
    pub command: String,
    /// Java 一行说明（二进制 + 主版本）。
    pub java: String,
    /// 系统一行说明。
    pub system: String,
    /// 装配阶段失败的 slug（`version_manifest` / `spawn` …）；`None` = 进程起来过。
    pub stage: Option<String>,
    /// 装配阶段的原始错误文本（不翻译，可能含路径）。
    pub stage_detail: Option<String>,
    /// 启动器捕获的最后若干行（已脱敏、已截断到 [`TAIL_LINES`]）。
    pub stderr_tail: Vec<String>,
}

/// 「进程确实起来过」的那一场会话归因（[`CrashReport::before_start`] 的可选入参）。
///
/// 只带两样东西：**会话 id**（必须与收尾事件里的 `session` 是同一个值）与事件流落点。
/// 收尾失败时进程真的起来过，所以这份现场**有主人**；写成空串等于说「这场失败不属于任何人」，
/// 界面侧只能把它丢掉（M298）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionAttribution<'a> {
    /// 会话 id。
    pub id: &'a str,
    /// 该会话的事件流 JSONL（`None` = 拿不到落点，仍然记得住归属）。
    pub log: Option<&'a std::path::Path>,
}

/// 证据形态（界面按它选 i18n key，**不许**再自己判断）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// 游戏自己写了 `crash-reports/crash-*.txt`。
    GameCrashReport,
    /// 游戏没写崩溃报告，但启动器抓到了输出。
    CapturedOutput,
    /// 连输出都没抓到（JVM 都没起来 / 立刻失败）。
    Nothing,
}

impl Evidence {
    /// 稳定 slug（测试与日志用；不是文案）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::GameCrashReport => "game_crash_report",
            Self::CapturedOutput => "captured_output",
            Self::Nothing => "nothing",
        }
    }
}

impl CrashReport {
    /// 游戏侧证据的形态（界面按它选文案，**不许**再自己判断）。
    #[must_use]
    pub fn evidence(&self) -> Evidence {
        if self.has_game_crash_report() {
            Evidence::GameCrashReport
        } else if self.stderr_tail.is_empty() {
            Evidence::Nothing
        } else {
            Evidence::CapturedOutput
        }
    }

    /// 是否找到游戏自己写的崩溃报告。
    #[must_use]
    pub fn has_game_crash_report(&self) -> bool {
        self.artifacts
            .iter()
            .any(|a| a.source == CrashSource::CrashReport.as_str())
    }

    /// 进程是否起来过（决定标题写「启动失败」还是「游戏崩溃」）。
    #[must_use]
    pub fn started(&self) -> bool {
        self.stage.is_none()
    }

    /// 该实例运行目录下的 `crash-reports/`。
    #[must_use]
    pub fn crash_reports_dir(&self) -> Option<PathBuf> {
        self.run_dir.as_ref().map(|dir| dir.join("crash-reports"))
    }

    /// 该实例的 `logs/latest.log`。
    #[must_use]
    pub fn latest_log_path(&self) -> Option<PathBuf> {
        self.run_dir
            .as_ref()
            .map(|dir| dir.join("logs").join("latest.log"))
    }

    /// 供「复制路径」用的（标签, 路径）列表（提示词在前、目录在后）。
    #[must_use]
    pub fn path_lines(&self) -> Vec<(String, PathBuf)> {
        let mut lines = Vec::new();
        if let Some(log) = &self.session_log {
            lines.push(("session".to_string(), log.clone()));
        }
        if let Some(dir) = &self.launcher_logs {
            lines.push(("launcher_logs".to_string(), dir.clone()));
        }
        if let Some(path) = self.latest_log_path() {
            lines.push(("latest_log".to_string(), path));
        }
        if let Some(dir) = self.crash_reports_dir() {
            lines.push(("crash_reports".to_string(), dir));
        }
        if let Some(dir) = &self.run_dir {
            lines.push(("run_dir".to_string(), dir.clone()));
        }
        lines
    }

    /// 收集一次**已经起来过**的启动的现场。
    ///
    /// `session` 由调用方在 `finish()` 之后给出；`lines` 是该会话的事件流行（**调用方读**，
    /// 因为读 JSONL 属于 host 的活）。只有最后 [`TAIL_LINES`] 行进弹窗。
    #[must_use]
    pub fn collect(session: &SessionHandle, ctx: &ReportContext, lines: &[String]) -> Self {
        let run_dir = ctx.run_dir.clone();
        let artifacts = crash_scan::scan(&run_dir, ctx.started_at, MAX_ARTIFACTS)
            .into_iter()
            .map(|artifact| fact(&artifact, &ctx.redactor()))
            .collect();

        let raw: Vec<String> = lines.iter().rev().take(TAIL_LINES).rev().cloned().collect();

        Self {
            instance: ctx.instance.clone(),
            name: ctx.name.clone(),
            version: ctx.version.clone(),
            session: session.session_id.clone(),
            session_log: Some(session.log_path.clone()),
            version_json: ctx.version_json.clone(),
            launcher_logs: Some(ctx.launcher_logs.clone()),
            run_dir: Some(run_dir),
            artifacts,
            exit_code: Some(session.exit.code),
            exit_type: session.exit.exit_type.as_str().to_string(),
            uptime_ms: session.uptime_ms,
            command: ctx
                .redactor()
                .scrub(session.redacted_display().as_str())
                .into_owned(),
            java: ctx.java.clone(),
            system: ctx.system.clone(),
            stage: None,
            stage_detail: None,
            stderr_tail: scrub_tail(&raw, &ctx.redactor()),
        }
    }

    /// 收集一次**没有 `SessionHandle`** 的现场。
    ///
    /// 两个用法，判据是同一个参数（`session`）：
    ///
    /// | 出口 | `session` | 为什么 |
    /// |---|---|---|
    /// | 装配失败 / spawn 失败 | `None` | 进程根本没起来，这一场**确实**没有会话 |
    /// | 收尾失败（`finish()` 报错） | `Some(归因)` | 进程起来过，这次失败属于那一场 |
    ///
    /// 第二种是 M298 修的缺口：此前两个出口都写空串，而界面侧的并发守卫
    /// （`CrashReport::session != LaunchFinished::session`）会因此把「自己这一场」判成
    /// 「别人的报告」而**静默不弹**。归因必须由调用方给（只有它知道是哪一场），本函数只负责
    /// 把它原样记进报告——两边同源，守卫才可能判对。
    ///
    /// 两个用法的**证据形态不同，判据同一个**（[`CrashReport::evidence`] 只看有没有
    /// crash-report / 有没有捕获输出，**不**看 `stage`）：
    ///
    /// | 用法 | 常见形态 | 界面该说什么 |
    /// |---|---|---|
    /// | `session: None`（装配 / spawn 失败） | 「找不到 crash-report」，游戏根本没被执行到 ⇒ [`Evidence::Nothing`] | 「游戏还没来得及写日志」 |
    /// | `session: Some`（收尾失败） | 进程起来过、事件流有落点，只是拿不到 `SessionHandle` | 按真实证据说（多半有捕获输出） |
    ///
    /// 别把这张表读成「本函数只描述没起来的那种」——M298 r0 的注释正是这么写的，复审 N3 抓到。
    #[must_use]
    pub fn before_start(
        ctx: &ReportContext,
        stage: &str,
        detail: &str,
        exit_type: &str,
        session: Option<SessionAttribution<'_>>,
    ) -> Self {
        let redactor = ctx.redactor();
        let session_id = session.map_or_else(String::new, |it| it.id.to_string());
        let session_log = session.and_then(|it| it.log.map(PathBuf::from));
        Self {
            instance: ctx.instance.clone(),
            name: ctx.name.clone(),
            version: ctx.version.clone(),
            session: session_id,
            session_log,
            version_json: ctx.version_json.clone(),
            launcher_logs: Some(ctx.launcher_logs.clone()),
            run_dir: Some(ctx.run_dir.clone()),
            artifacts: Vec::new(),
            exit_code: None,
            exit_type: exit_type.to_string(),
            uptime_ms: 0,
            command: String::new(),
            java: ctx.java.clone(),
            system: ctx.system.clone(),
            stage: Some(stage.to_string()),
            stage_detail: Some(redactor.scrub(detail).into_owned()),
            stderr_tail: Vec::new(),
        }
    }

    /// 诊断包清单里要写明的「没有的东西」（**不许静默省略**）。
    #[must_use]
    pub fn missing_lines(&self) -> Vec<String> {
        let mut missing = Vec::new();
        if !self.has_game_crash_report() {
            missing.push("crash-reports/crash-*.txt: 本次没有（游戏没来得及写）".to_string());
        }
        if let Some(path) = self.latest_log_path() {
            if !path.is_file() {
                missing.push(format!("logs/latest.log: 不存在（{}）", path.display()));
            }
        }
        if self.session_log.is_none() {
            missing.push("会话 JSONL: 不存在（进程没起来）".to_string());
        }
        missing
    }

    /// 备注（清单里「这次发生了什么」那一段）。
    #[must_use]
    pub fn note_lines(&self) -> Vec<String> {
        let mut notes = match self.evidence() {
            Evidence::GameCrashReport => vec!["游戏写出了崩溃报告（crash-reports/）".to_string()],
            Evidence::CapturedOutput => {
                vec!["本次没有生成崩溃报告；证据是启动器捕获的输出".to_string()]
            }
            Evidence::Nothing => vec![
                "本次没有崩溃报告，也没捕获到输出（最可能：缺 Java / 内存不足 / 参数错误 / 库缺失）"
                    .to_string(),
            ],
        };
        if let Some(stage) = &self.stage {
            notes.push(format!("进程没有起来，失败在：{stage}"));
        }
        notes
    }
}

#[cfg(test)]
mod tests;
