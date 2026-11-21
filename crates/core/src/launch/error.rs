//! launch 模块统一错误类型。
//!
//! 设计依据：`DevDocs/PlanDoc/06-launch-and-diagnostics.md` §1、`analyse/02` §8.4-3
//! （缺库/缺组件必须结构化报错并显式失败，而不是交给运行时 `NoClassDefFoundError`）。

use std::fmt;
use std::path::PathBuf;

/// launch 相关操作的统一结果类型。
pub type Result<T> = std::result::Result<T, LaunchError>;

/// 缺失的组件（库 / natives / 资产），供 UI 与 agent 直接展示与修复。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingComponent {
    /// 逻辑名，例如 `org.lwjgl:lwjgl:3.3.1` 或 `libraries/foo/bar.jar`。
    pub name: String,
    /// 期望落盘路径（未知时为 `None`）。
    pub path: Option<PathBuf>,
}

impl MissingComponent {
    /// 只按名字构造（路径未知）。
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: None,
        }
    }

    /// 带期望路径构造。
    pub fn at_path(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: Some(path.into()),
        }
    }
}

/// 命令行组装阶段的失败原因（全部是“启动前就能确定”的错误）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssemblyError {
    /// 清单里没有主类，无法启动（HMCL 在此直接抛异常）。
    MissingMainClass,
    /// classpath 为空（一个库和主 jar 都没有）。
    EmptyClasspath,
    /// Java 可执行文件路径为空。
    EmptyJavaPath,
    /// 组件不完整：启动前显式失败，附结构化清单。
    MissingComponents(Vec<MissingComponent>),
    /// 清单参数畸形（例如空参数项）。
    MalformedArgument {
        /// 原始参数项文本。
        entry: String,
        /// 原因说明。
        reason: &'static str,
    },
    /// natives 解压失败（jar 缺失/损坏、目录不可写）。
    ///
    /// 归到「启动前失败」而不是运行时错误：native 库没落地时游戏只会给出
    /// `UnsatisfiedLinkError`，那时候已经离「到底哪个 jar 坏了」太远了。
    NativesExtraction {
        /// 人读原因（含失败路径）。
        message: String,
    },
}

impl fmt::Display for AssemblyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingMainClass => f.write_str("版本清单缺少主类（mainClass）"),
            Self::EmptyClasspath => f.write_str("classpath 为空：没有任何库或主 jar"),
            Self::EmptyJavaPath => f.write_str("Java 可执行文件路径为空"),
            Self::MissingComponents(items) => {
                write!(f, "组件缺失 {} 项", items.len())?;
                for item in items.iter().take(8) {
                    write!(f, "; {}", item.name)?;
                }
                Ok(())
            }
            Self::MalformedArgument { entry, reason } => {
                write!(f, "参数畸形（{reason}）：{entry}")
            }
            Self::NativesExtraction { message } => {
                write!(f, "natives 解压失败：{message}")
            }
        }
    }
}

impl std::error::Error for AssemblyError {}

/// launch 模块的统一错误。
#[derive(Debug)]
pub enum LaunchError {
    /// 命令行组装失败（纯计算，可单测）。
    Assembly(AssemblyError),
    /// 文件系统操作失败。
    Io {
        /// 操作名（`create_session` / `read_argfile` 等）。
        op: &'static str,
        /// 相关路径。
        path: PathBuf,
        /// 底层错误。
        source: std::io::Error,
    },
    /// 进程派生失败（Java 不存在、权限不足等）。
    Spawn {
        /// 被执行的程序。
        program: PathBuf,
        /// 底层错误。
        source: std::io::Error,
    },
    /// 会话事件流写入/解析失败。
    Session {
        /// 会话文件路径。
        path: PathBuf,
        /// 原因说明。
        reason: String,
    },
    /// 进程树 kill 失败。
    Kill {
        /// 目标进程号。
        pid: u32,
        /// 底层错误。
        source: std::io::Error,
    },
    /// 看门狗到点**停止等待**（D6）：进程仍在运行，我们只是不再等它了。
    Watchdog {
        /// 目标进程号（没有被结束）。
        pid: u32,
        /// 判据 id：`startup_timeout` / `shutdown_timeout`。
        reason: &'static str,
        /// 已经等了多久（毫秒）。
        waited_ms: u64,
        /// 判据上限（毫秒）。
        limit_ms: u64,
        /// 会话事件流（现场）。
        log_path: PathBuf,
    },
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Assembly(err) => write!(f, "命令行组装失败：{err}"),
            Self::Io { op, path, source } => {
                write!(f, "{op} 失败（{}）：{source}", path.display())
            }
            Self::Spawn { program, source } => {
                write!(f, "启动进程失败（{}）：{source}", program.display())
            }
            Self::Session { path, reason } => {
                write!(f, "会话事件流错误（{}）：{reason}", path.display())
            }
            Self::Kill { pid, source } => {
                write!(f, "结束进程树失败（pid {pid}）：{source}")
            }
            Self::Watchdog {
                pid,
                reason,
                waited_ms,
                limit_ms,
                log_path,
            } => {
                let what = if *reason == "shutdown_timeout" {
                    "游戏已开始关闭但一直没退出"
                } else {
                    "启动后一直没有任何日志、也没出现游戏窗口"
                };
                write!(
                    f,
                    "{what}：等了 {:.0}s（上限 {:.0}s）就停止等待了。进程 {pid} 仍在运行、没有被结束\
                     （可能只是加载得慢，也可能真卡住了）；事件流见 {}。放宽上限：设环境变量 \
                     ULL_LAUNCH_STARTUP_TIMEOUT_SECS / ULL_LAUNCH_SHUTDOWN_GRACE_SECS（秒，0 = 不限）",
                    *waited_ms as f64 / 1000.0,
                    *limit_ms as f64 / 1000.0,
                    log_path.display()
                )
            }
        }
    }
}

impl std::error::Error for LaunchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Assembly(err) => Some(err),
            Self::Io { source, .. } | Self::Spawn { source, .. } | Self::Kill { source, .. } => {
                Some(source)
            }
            Self::Session { .. } => None,
            Self::Watchdog { .. } => None,
        }
    }
}

impl From<AssemblyError> for LaunchError {
    fn from(err: AssemblyError) -> Self {
        Self::Assembly(err)
    }
}
