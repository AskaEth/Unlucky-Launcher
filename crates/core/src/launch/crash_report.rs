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
use std::sync::{Mutex, OnceLock};

use super::crash_scan::{self, CrashArtifact, CrashSource};
use super::launch_session::SessionHandle;
use super::redact::Redactor;

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

    /// 收集一次**进程没起来**的现场（装配 / 起进程阶段失败）。
    ///
    /// 这种失败最常见的形态就是「找不到 crash-report」——游戏根本没被执行到，所以
    /// [`Evidence`] 落到 [`Evidence::Nothing`]，界面必须说「游戏还没来得及写日志」。
    #[must_use]
    pub fn before_start(ctx: &ReportContext, stage: &str, detail: &str, exit_type: &str) -> Self {
        let redactor = ctx.redactor();
        Self {
            instance: ctx.instance.clone(),
            name: ctx.name.clone(),
            version: ctx.version.clone(),
            session: String::new(),
            session_log: None,
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

/// [`CrashReport::collect`] / [`CrashReport::before_start`] 的输入（全是启动侧已有的东西）。
///
/// 字段是**拥有的**：构造一次、在四个失败出口上反复用（借用版会让每个出口都要重算一遍
/// 主目录与脱敏器，而这条路径低频、克隆代价可以忽略）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportContext {
    /// 实例 id。
    pub instance: String,
    /// 实例名。
    pub name: String,
    /// 版本 id。
    pub version: String,
    /// 实例运行目录。
    pub run_dir: PathBuf,
    /// 启动器日志目录（`.ULL/logs`）。
    pub launcher_logs: PathBuf,
    /// 本次会话的开始时刻（现场文件的时间闸）。
    pub started_at: std::time::SystemTime,
    /// 该实例绑定的版本 JSON。
    pub version_json: Option<PathBuf>,
    /// Java 一行说明。
    pub java: String,
    /// 系统一行说明。
    pub system: String,
    /// 要额外抹掉的串（会话 id、access token 等）。
    pub secrets: Vec<String>,
}

impl ReportContext {
    /// 建一份（`java` 可以后补：装配阶段还没选 Java）。
    #[must_use]
    pub fn new(
        instance: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        run_dir: impl Into<PathBuf>,
        launcher_logs: impl Into<PathBuf>,
        started_at: std::time::SystemTime,
        system: impl Into<String>,
    ) -> Self {
        Self {
            instance: instance.into(),
            name: name.into(),
            version: version.into(),
            run_dir: run_dir.into(),
            launcher_logs: launcher_logs.into(),
            started_at,
            version_json: None,
            java: String::new(),
            system: system.into(),
            secrets: Vec::new(),
        }
    }

    fn redactor(&self) -> Redactor {
        let mut redactor = Redactor::new();
        for key in ["USERPROFILE", "HOME"] {
            if let Ok(home) = std::env::var(key) {
                if !home.trim().is_empty() {
                    redactor = redactor.with_home_str(home);
                }
            }
        }
        for secret in &self.secrets {
            redactor = redactor.with_secret(secret.clone());
        }
        redactor
    }
}

fn scrub_tail(lines: &[String], redactor: &Redactor) -> Vec<String> {
    lines
        .iter()
        .map(|line| redactor.scrub(line).into_owned())
        .collect()
}

fn fact(artifact: &CrashArtifact, redactor: &Redactor) -> ArtifactFact {
    let excerpt = artifact.excerpt.as_ref().map(|text| {
        let scrubbed = redactor.scrub(text);
        let trimmed: String = scrubbed.chars().take(MAX_EXCERPT_CHARS).collect();
        trimmed
    });
    ArtifactFact {
        source: artifact.source.as_str().to_string(),
        path: artifact.path.clone(),
        excerpt,
    }
}

fn slot() -> &'static Mutex<Option<CrashReport>> {
    static SLOT: OnceLock<Mutex<Option<CrashReport>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// 登记一次失败启动的现场（启动线程收尾时调用）。
pub fn publish(report: CrashReport) {
    if let Ok(mut guard) = slot().lock() {
        *guard = Some(report);
    }
}

/// 读最近一次失败启动的现场。
#[must_use]
pub fn failing_launch() -> Option<CrashReport> {
    slot().lock().ok().and_then(|guard| guard.clone())
}

/// 清空槽位（测试用）。
pub fn clear() {
    if let Ok(mut guard) = slot().lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests;
