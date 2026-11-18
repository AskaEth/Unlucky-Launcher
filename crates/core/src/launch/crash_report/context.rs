//! 现场的**输入**与脱敏（M298 从 `crash_report.rs` 拆出，纯搬移）。
//!
//! 这个文件只回答两件事：①收集一次现场要知道哪几样事实（[`ReportContext`]）；
//! ②它们在被写进 [`CrashReport`](super::CrashReport) 之前必须先过哪一道脱敏闸
//! （[`fact`] / [`scrub_tail`]）。
//!
//! 脱敏器**不缓存**：每次 [`ReportContext::redactor`] 都重新读一遍主目录环境变量。这条路径
//! 只在启动收尾时走一次，缓存省不下什么，却会引入「进程启动后 HOME 变了」的第二份真相。

use std::path::PathBuf;

use super::super::crash_scan::CrashArtifact;
use super::super::redact::Redactor;
use super::{ArtifactFact, MAX_EXCERPT_CHARS};

/// [`CrashReport::collect`](super::CrashReport::collect) /
/// [`CrashReport::before_start`](super::CrashReport::before_start) 的输入（全是启动侧已有的东西）。
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

    /// 本次现场用的脱敏器（主目录 + `secrets` 两把刀）。
    pub(super) fn redactor(&self) -> Redactor {
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

/// 捕获输出尾巴的脱敏（逐行过同一把刀）。
pub(super) fn scrub_tail(lines: &[String], redactor: &Redactor) -> Vec<String> {
    lines
        .iter()
        .map(|line| redactor.scrub(line).into_owned())
        .collect()
}

/// 现场文件 → 报告里的一条事实（摘要抹平 + 截断到 [`MAX_EXCERPT_CHARS`]）。
pub(super) fn fact(artifact: &CrashArtifact, redactor: &Redactor) -> ArtifactFact {
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
