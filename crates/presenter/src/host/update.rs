//! 自更新在宿主层的**装配与翻译**（M159）。
//!
//! # 分工
//!
//! ```text
//! core::update::Job   ← 检查/应用的做法（版本比较、渠道准入、双检、原子替换全在 core）
//!         ▲
//!         │ 本模块只做四件事
//! Factory             ← 用哪个源、暂存放哪、替换谁（生产：GitHub + 本进程 exe）
//! outcome_of          ← core 的结论 → DTO（GUI 与 CLI 共用同一条判据）
//! Paths / Migration   ← 两个开关的落点，以及旧 `cache/update/prefs.json` 的迁移（M161）
//! ```
//!
//! 「GUI 与 CLI 行为一致」由结构保证：两边都只构造 `core::update::Job` 并调用 `check` / `apply`，
//! 没有第二处版本比较、也没有第二个下载/校验流程（CLI 侧见 `crates/cli/src/commands/update.rs`）。
//!
//! # 静默检查的边界
//!
//! 启动时的检查能被关掉（`update.auto_check`）：关掉之后 host 的装配路径**根本不派发**
//! （判据在 `CoreHost::refresh_all` 的那一行 `if self.update.should_check_on_start()`）。
//!
//! # 两个开关住在主配置里（M161）
//!
//! `update.auto_check` / `update.channel` 是 `crates/core/src/setting/keys.rs` 的 schema 键，
//! 落在 `<数据根>/config/launcher.json`——与「所有配置走 `core::setting`」的约定一致，
//! 于是 `ull config get update.auto_check` 读到的是**同一个值**。
//!
//! M159 曾把这两项临时放进更新域自己的文件 `<数据根>/cache/update/prefs.json`（当时
//! `crates/core/src/setting/**` 不在那条 mission 的 scope），代价是「GUI 里关掉、CLI 仍报 true」。
//! 存量值的迁移在 [`unlucky_core::setting::update_prefs`]：幂等（源指纹闸门）、不丢 `false`、
//! 新值胜出、旧文件改名为 `prefs.json.migrated` 后不再有任何读路径。`Runtime::with_factory`
//! 在建会话时跑一次并记住结局（[`Runtime::migration`]，`null` = 没有遗留文件）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::dto::{UpdateCandidateView, UpdateFailureView, UpdateOutcome};
use unlucky_core::download::DownloadEngine;
use unlucky_core::launch::time::{format_rfc3339_millis, now};
use unlucky_core::setting::update_prefs;
use unlucky_core::update::{
    default_source, staging_dir, Applied, CheckOutcome, DirSource, Job, Target, UpdateError,
    UpdateSource, Version,
};

use super::engine;
use super::env::HostEnv;
use super::logs::display_time;

/// 本进程版本。整个 workspace 同版本（根 `Cargo.toml` 的 `[workspace.package]`）。
#[must_use]
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 更新域的三个落点（都由数据根派生，core 不猜布局）。
#[derive(Debug, Clone)]
pub struct Paths {
    /// `<数据根>/config/launcher.json`（两个开关的真相源）
    pub launcher: PathBuf,
    /// `<数据根>/cache/update/prefs.json`（M159 的旧文件；迁移后改名 `.migrated`）
    pub legacy: PathBuf,
    /// `<数据根>/config/update-prefs-migration.json`（迁移收据）
    pub receipt: PathBuf,
}

impl Paths {
    /// 按数据根装配。
    #[must_use]
    pub fn for_env(env: &HostEnv) -> Self {
        Self {
            launcher: env.launcher_config(),
            legacy: env.layout.cache().join("update").join("prefs.json"),
            receipt: env.layout.config().join(update_prefs::RECEIPT_FILE),
        }
    }

    /// 迁移要的三个路径。
    #[must_use]
    pub fn targets(&self) -> update_prefs::Targets<'_> {
        update_prefs::Targets {
            launcher: &self.launcher,
            legacy: &self.legacy,
            receipt: &self.receipt,
        }
    }
}

/// 怎么造一个 [`Job`]（源 / 暂存 / 替换目标）。
///
/// 每派发一次就造一个新的（`Job` 里装的源是 `Box<dyn UpdateSource>`，不共享、不复用）：
/// 这样「上一次用的源」不会跟着下一次跑，测试也能各用例各给一份本地目录。
#[derive(Clone)]
pub enum Factory {
    /// 生产/spec 形态：GitHub Releases（**带 `update-test-source` feature 的验收构建**里，两档
    /// 环境变量给全时是本地目录源，见 `unlucky_core::update::job` 的 `test_source_root`），
    /// 引擎懒建一次。
    Default {
        /// 下载引擎（镜像/地区/健康度策略都在里面）。
        engine: Arc<DownloadEngine>,
        /// 目标平台。
        target: Target,
        /// 暂存目录。
        staging: PathBuf,
        /// 替换目标。
        dest: PathBuf,
    },
    /// 单测/取证：钉死一个本地目录源。
    Dir {
        /// 源根目录（`<root>/stable.json` + 产物）。
        root: PathBuf,
        /// 目标平台。
        target: Target,
        /// 暂存目录。
        staging: PathBuf,
        /// 替换目标。
        dest: PathBuf,
    },
}

impl std::fmt::Debug for Factory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Factory")
            .field("dir", &self.dir())
            .field("dest", &self.dest())
            .finish()
    }
}

impl Factory {
    /// 生产装配：数据根的缓存做暂存，替换目标是**本进程的 exe**。
    #[must_use]
    pub fn for_env(env: &HostEnv) -> Self {
        Self::Default {
            engine: engine::engine(env),
            target: Target::current(),
            staging: staging_dir(&env.layout.cache()),
            dest: unlucky_core::update::process_dest(),
        }
    }

    /// 换替换目标（单测专用：测试**绝不**替换真的测试二进制）。
    #[must_use]
    pub fn with_dest(self, dest: impl Into<PathBuf>) -> Self {
        let dest = dest.into();
        match self {
            Self::Default {
                engine,
                target,
                staging,
                ..
            } => Self::Default {
                engine,
                target,
                staging,
                dest,
            },
            Self::Dir {
                root,
                target,
                staging,
                ..
            } => Self::Dir {
                root,
                target,
                staging,
                dest,
            },
        }
    }

    /// 换暂存目录（单测：落在临时目录里）。
    #[must_use]
    pub fn with_staging(self, staging: impl Into<PathBuf>) -> Self {
        let staging = staging.into();
        match self {
            Self::Default {
                engine,
                target,
                dest,
                ..
            } => Self::Default {
                engine,
                target,
                staging,
                dest,
            },
            Self::Dir {
                root, target, dest, ..
            } => Self::Dir {
                root,
                target,
                staging,
                dest,
            },
        }
    }

    /// 换成本地目录源（单测/取证）。
    #[must_use]
    pub fn with_dir(self, root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        match self {
            Self::Default {
                target,
                staging,
                dest,
                ..
            } => Self::Dir {
                root,
                target,
                staging,
                dest,
            },
            Self::Dir {
                target,
                staging,
                dest,
                ..
            } => Self::Dir {
                root,
                target,
                staging,
                dest,
            },
        }
    }

    /// 本地目录源根（`None` = 走 GitHub）。
    #[must_use]
    pub fn dir(&self) -> Option<&Path> {
        match self {
            Self::Default { .. } => None,
            Self::Dir { root, .. } => Some(root),
        }
    }

    /// 替换目标。
    #[must_use]
    pub fn dest(&self) -> &Path {
        match self {
            Self::Default { dest, .. } | Self::Dir { dest, .. } => dest,
        }
    }

    /// 造一个 [`Job`]。
    #[must_use]
    pub fn job(&self) -> Job {
        match self {
            Self::Default {
                engine,
                target,
                staging,
                dest,
            } => Job::new(
                default_source(Arc::clone(engine)),
                *target,
                staging.clone(),
                dest.clone(),
            ),
            Self::Dir {
                root,
                target,
                staging,
                dest,
            } => Job::new(
                Box::new(DirSource::new(root.clone())) as Box<dyn UpdateSource>,
                *target,
                staging.clone(),
                dest.clone(),
            ),
        }
    }
}

/// 后台线程 → host 的**更新**回报（只含 DTO：`job.rs` 不必认识 core 类型）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// 检查结论
    Checked(UpdateOutcome),
    /// 应用结局
    Applied(ApplyReport),
}

/// 应用结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyReport {
    /// 成功：新版本已就位
    Installed {
        /// 新版本号
        version: String,
        /// 替换后的路径
        path: String,
        /// 旧版本备份路径（`None` = 原本没有文件 / 已清掉）
        backup: Option<String>,
    },
    /// 复查时已经没有可装的更新
    Nothing(UpdateOutcome),
    /// 失败（分类 + 人话）
    Failed(UpdateFailureView),
}

impl Report {
    /// 应用结局 → 回报（映射只在这一处）。
    #[must_use]
    pub fn applied(result: Result<Applied, UpdateError>) -> Self {
        match result {
            Ok(Applied::Installed { receipt, version }) => Self::Applied(ApplyReport::Installed {
                version,
                path: receipt.dest.display().to_string(),
                backup: receipt.backup.map(|backup| backup.display().to_string()),
            }),
            Ok(Applied::Nothing(outcome)) => {
                Self::Applied(ApplyReport::Nothing(outcome_of(Ok(outcome))))
            }
            Err(error) => Self::Applied(ApplyReport::Failed(failure_of(&error))),
        }
    }
}

/// core 的检查结论 → DTO（**唯一**的映射点）。
///
/// 三条不许折叠的边界在这里体现：`UpToDate` 与 `Failed` 是两个变体；
/// 「有更高版本但没有本平台产物」单独一个变体（[`UpdateOutcome::MissingArtifact`]）。
#[must_use]
pub fn outcome_of(result: Result<CheckOutcome, UpdateError>) -> UpdateOutcome {
    match result {
        Ok(CheckOutcome::UpToDate { latest, .. }) => UpdateOutcome::UpToDate {
            latest: latest.map(|version| version.to_string()),
        },
        Ok(CheckOutcome::Available(candidate)) => {
            UpdateOutcome::Available(Box::new(UpdateCandidateView {
                version: candidate.release.version.to_string(),
                size: candidate.artifact.size,
                notes: notes_summary(candidate.release.notes.as_deref()),
                forced: candidate.urgency == unlucky_core::update::Urgency::Forced,
            }))
        }
        Ok(CheckOutcome::MissingArtifact { release }) => UpdateOutcome::MissingArtifact {
            version: release.version.to_string(),
        },
        Err(error) => UpdateOutcome::Failed(failure_of(&error)),
    }
}

/// core 的错误 → 失败视图（分类 id + 人话 + 可重试）。
#[must_use]
pub fn failure_of(error: &UpdateError) -> UpdateFailureView {
    UpdateFailureView::new(error.kind.as_str(), error.to_string(), error.is_retryable())
}

/// 更新说明摘要的字符上限（提示条上只放一段；整篇说明在 Release 页）。
const NOTES_LIMIT: usize = 160;

fn notes_summary(notes: Option<&str>) -> String {
    let text = notes.unwrap_or("").trim();
    if text.chars().count() <= NOTES_LIMIT {
        return text.to_string();
    }
    let mut short: String = text.chars().take(NOTES_LIMIT).collect();
    short.push('…');
    short
}

/// 当前时刻的人读戳（`MM-DD HH:MM`，与日志页同一个格式函数）。
pub(super) fn stamp() -> String {
    display_time(&format_rfc3339_millis(now()))
}

/// 把版本号解析成 core 的 [`Version`]（`None` = 编译进来的版本号不合法，理论上不可能）。
#[must_use]
pub(super) fn parse_version(text: &str) -> Option<Version> {
    Version::parse(text).ok()
}
