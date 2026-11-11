//! platform 层错误类型与结果别名。
//!
//! 设计出处：`DevDocs/PlanDoc/04-data-and-config.md` §1（三模式 + 迁移收据）、
//! `DevDocs/analyse/07-settings-packaging-update.md` §1.6（保存通道）、§1.8（遗留迁移收据）。

use std::fmt;
use std::io;
use std::path::PathBuf;

/// platform 层的结果别名。
pub type PlatformResult<T> = Result<T, PlatformError>;

/// 数据目录解析、文件系统原语、目录迁移可能出的问题。
#[derive(Debug)]
pub enum PlatformError {
    /// 文件系统操作失败（带出错路径）。
    Io {
        /// 出错的文件或目录。
        path: PathBuf,
        /// 底层 IO 错误。
        source: io::Error,
    },
    /// Portable 模式拿不到可执行文件所在目录。
    NoExecutableDir,
    /// Standard 模式拿不到宿主标准数据目录（环境变量缺失）。
    NoStandardDataDir {
        /// 平台名（`windows` / `macos` / `linux` / `other`）。
        platform: &'static str,
    },
    /// Custom 模式没给目录，或给的不是绝对路径。
    InvalidCustomRoot {
        /// 用户给的目录。
        root: PathBuf,
    },
    /// 实例 id 非法（只允许 `[A-Za-z0-9_-]`，挡掉 `../` 路径穿越）。
    InvalidInstanceId {
        /// 用户/档案里给的 id。
        id: String,
    },
    /// `redirect.json` 链超过跳数上限或成环。
    RedirectCycle {
        /// 起始目录。
        dir: PathBuf,
        /// 实际跳数。
        hops: usize,
    },
    /// `redirect.json` 内容损坏。
    RedirectCorrupt {
        /// 指针文件路径。
        path: PathBuf,
        /// 原因。
        reason: String,
    },
    /// 拒绝迁移（目标已有数据、源不存在等），原因写在消息里。
    MigrationRefused {
        /// 拒绝原因。
        reason: String,
    },
    /// 迁移失败且回滚也失败——需要人工介入，不能当成普通失败吞掉。
    RollbackFailed {
        /// 原始失败 + 回滚失败串起来的原因。
        reason: String,
    },
    /// 迁移收据损坏。
    ReceiptCorrupt {
        /// 收据文件路径。
        path: PathBuf,
        /// 原因。
        reason: String,
    },
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "IO 失败 @ {}: {source}", path.display()),
            Self::NoExecutableDir => write!(f, "Portable 模式无法定位可执行文件所在目录"),
            Self::NoStandardDataDir { platform } => {
                write!(
                    f,
                    "Standard 模式在 {platform} 上拿不到标准数据目录（环境变量缺失）"
                )
            }
            Self::InvalidCustomRoot { root } => {
                write!(
                    f,
                    "Custom 数据目录不合法（必须是绝对路径）: {}",
                    root.display()
                )
            }
            Self::InvalidInstanceId { id } => {
                write!(f, "实例 id 非法（只允许 [A-Za-z0-9_-]）: {id:?}")
            }
            Self::RedirectCycle { dir, hops } => write!(
                f,
                "redirect.json 跳转成环或超限（{hops} 跳）@ {}",
                dir.display()
            ),
            Self::RedirectCorrupt { path, reason } => {
                write!(f, "redirect.json 损坏 @ {}: {reason}", path.display())
            }
            Self::MigrationRefused { reason } => write!(f, "拒绝迁移: {reason}"),
            Self::RollbackFailed { reason } => write!(f, "迁移回滚失败，需人工介入: {reason}"),
            Self::ReceiptCorrupt { path, reason } => {
                write!(f, "迁移收据损坏 @ {}: {reason}", path.display())
            }
        }
    }
}

impl std::error::Error for PlatformError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// 把 `io::Error` 绑上路径，给 `map_err` 用。
pub(crate) fn io_at<P: Into<PathBuf>>(path: P) -> impl FnOnce(io::Error) -> PlatformError {
    let path = path.into();
    move |source| PlatformError::Io { path, source }
}
