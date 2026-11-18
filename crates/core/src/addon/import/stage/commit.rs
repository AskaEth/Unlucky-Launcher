//! 提交守卫、回滚与**回滚本身失败的记账**：`Drop` 即回滚，除非显式 `finish`。
//!
//! 靠调用方在每个错误分支手动清理，早晚会漏一条——所以把"回滚"绑在析构上：任何提前
//! `return`、`?` 冒泡、panic 展开都不会留下半个实例。
//!
//! 回滚分两种：
//!
//! - **全新导入**：目标目录整个是本事务建的 ⇒ 删干净就是零残留；
//! - **合并态**：按 journal **逆序**撤回——新文件删掉、被替换的旧文件从 `.ullbak-<pid>` 改回原名。
//!
//! # 回滚也可能失败（复审 r5 P2-2）
//!
//! 早期实现把每一步写成 `let _ = …`，失败被吞，报告却照样宣称"目标目录与导入前一致"。
//! 现在每次回滚都返回 [`RollbackOutcome`]：调用的**显式**回滚路径（`CommitGuard::rollback`）
//! 由导入管线把结果写进报告（`rolled_back` 只在**完整**回滚时为 `true`，否则列出失败项）；
//! `Drop` 那条兜底路径拿不到调用方，只能尽力而为——所以它是"防漏"而不是"承诺"，文档与
//! 报告口径都按这个写。

use std::path::{Path, PathBuf};

use super::io_error;
use crate::addon::import::ImportError;

/// 备份后缀（合并态的 journal 备份文件）。
const BACKUP_TAG: &str = "ullbak";

/// 一次回滚的结果（**失败项要能被报告出来**）。
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackOutcome {
    /// 尝试过的回滚动作数（删新文件 + 恢复备份）。
    pub attempted: usize,
    /// 失败项的人读描述（路径 + 原因）；空表 = 完整回滚。
    pub failed: Vec<String>,
}

impl RollbackOutcome {
    /// 是否完整回滚（没有失败项）。
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.failed.is_empty()
    }

    /// 失败项数。
    #[must_use]
    pub fn failure_count(&self) -> usize {
        self.failed.len()
    }
}

/// 一条已应用的搬动记录。
#[derive(Debug)]
pub struct Applied {
    /// 目标路径。
    pub(super) dest: PathBuf,
    /// 被替换下来的旧文件备份。
    pub(super) backup: Option<PathBuf>,
    /// 新文件是否已经搬到位。
    pub(super) moved: bool,
}

/// 提交守卫：**析构即回滚**，除非显式 [`CommitGuard::finish`]。
#[derive(Debug)]
pub struct CommitGuard {
    inner: Option<Inner>,
}

#[derive(Debug)]
struct Inner {
    target: PathBuf,
    temp: PathBuf,
    /// 本次是否**新建**了目标目录（全新导入）。
    created_target: bool,
    applied: Vec<Applied>,
    finished: bool,
}

impl CommitGuard {
    /// 全新导入留下的守卫（目标目录已是 rename 后的成品）。
    pub(super) fn fresh(target: PathBuf, temp: PathBuf) -> Self {
        Self {
            inner: Some(Inner {
                target,
                temp,
                created_target: true,
                applied: Vec::new(),
                finished: false,
            }),
        }
    }

    /// 合并态留下的守卫（`applied` 是已搬进去的 journal）。
    pub(super) fn merge(target: PathBuf, temp: PathBuf, applied: Vec<Applied>) -> Self {
        Self {
            inner: Some(Inner {
                target,
                temp,
                created_target: false,
                applied,
                finished: false,
            }),
        }
    }

    /// 目标目录。
    #[must_use]
    pub fn target(&self) -> &Path {
        self.inner
            .as_ref()
            .map_or(Path::new(""), |inner| inner.target.as_path())
    }

    /// 确认提交：删备份、清临时目录。
    pub fn finish(mut self) {
        if let Some(inner) = self.inner.as_mut() {
            for applied in &inner.applied {
                if let Some(backup) = &applied.backup {
                    let _ = std::fs::remove_file(backup);
                }
            }
            let _ = std::fs::remove_dir_all(&inner.temp);
            inner.finished = true;
        }
    }

    /// 显式回滚，并**返回结果**（失败的项由调用方写进报告）。
    ///
    /// `Drop` 也会回滚，但那条路径没法上报——因此真正需要"宣称一致"的地方必须调这个。
    pub fn rollback(mut self) -> RollbackOutcome {
        match self.inner.as_mut() {
            Some(inner) => rollback_inner(inner),
            None => RollbackOutcome::default(),
        }
    }
}

impl Drop for CommitGuard {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.as_mut() {
            if !inner.finished {
                // 兜底：拿不到调用方，失败只能丢在这里（所以别依赖 Drop 去做"宣称"）。
                let _ = rollback_inner(inner);
            }
        }
    }
}

/// 回滚一次提交。
fn rollback_inner(inner: &mut Inner) -> RollbackOutcome {
    let mut outcome = RollbackOutcome::default();
    if inner.created_target {
        // 全新导入：目标目录整个是本事务建的，删干净 = 零残留。
        outcome.attempted += 1;
        if let Err(error) = std::fs::remove_dir_all(&inner.target) {
            if error.kind() != std::io::ErrorKind::NotFound {
                outcome.failed.push(format!(
                    "删除本次新建的实例目录 {} 失败：{error}",
                    inner.target.display()
                ));
            }
        }
    } else {
        outcome = rollback_merge(&mut inner.applied);
    }
    let _ = std::fs::remove_dir_all(&inner.temp);
    inner.finished = true;
    outcome
}

/// 合并态回滚：新文件删掉、备份改回原名（**失败逐项记下来**）。
pub(super) fn rollback_merge(applied: &mut [Applied]) -> RollbackOutcome {
    let mut outcome = RollbackOutcome::default();
    for entry in applied.iter().rev() {
        if entry.moved {
            outcome.attempted += 1;
            match std::fs::remove_file(&entry.dest) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => outcome
                    .failed
                    .push(format!("删除 {} 失败：{error}", entry.dest.display())),
            }
        }
        if let Some(backup) = &entry.backup {
            if backup.exists() {
                outcome.attempted += 1;
                if let Err(error) = std::fs::rename(backup, &entry.dest) {
                    outcome.failed.push(format!(
                        "把备份 {} 改回 {} 失败：{error}",
                        backup.display(),
                        entry.dest.display()
                    ));
                }
            }
        }
    }
    outcome
}

/// 备份路径：`<dest>.ullbak-<pid>`。
pub(super) fn backup_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().map_or_else(
        || std::ffi::OsString::from("file"),
        std::ffi::OsStr::to_os_string,
    );
    name.push(format!(".{BACKUP_TAG}-{}", std::process::id()));
    dest.with_file_name(name)
}

/// 提交阶段的两条停止路线（M195）：**取消不是失败**，所以不共用 [`CommitFailure`]。
///
/// 两条路都**已经把回滚做过**（全新导入是 `abort`，合并态是 journal 逆序撤回）：
/// 调用方拿到它们只管写报告，不必再自己清一遍。
#[derive(Debug)]
pub enum CommitStop {
    /// 用户取消（回滚结果一并交回，可能不完整）。
    Cancelled(RollbackOutcome),
    /// 真失败（回滚结果连同错误一起交回）。
    Failed(CommitFailure),
}

/// 一次**失败的回滚**（提交阶段失败时连同错误一起交回调用方）。
///
/// `io_error` 让"把一个 IO 原因包成 `pack_io`"只在一处定义。
#[derive(Debug)]
pub struct CommitFailure {
    /// 提交阶段本身的错误。
    pub error: ImportError,
    /// 回滚结果（可能不完整）。
    pub rollback: RollbackOutcome,
}

impl CommitFailure {
    /// 用一条 IO 原因建失败结果。
    #[must_use]
    pub(super) fn io(message: impl Into<String>, rollback: RollbackOutcome) -> Self {
        Self {
            error: io_error(message),
            rollback,
        }
    }

    /// 用已有的导入错误建失败结果。
    #[must_use]
    pub(super) fn new(error: ImportError, rollback: RollbackOutcome) -> Self {
        Self { error, rollback }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(dest: PathBuf, backup: Option<PathBuf>, moved: bool) -> Applied {
        Applied {
            dest,
            backup,
            moved,
        }
    }

    #[test]
    fn complete_rollback_has_no_failures() {
        let root = std::env::temp_dir().join(format!("ull-rb-ok-{}", std::process::id()));
        std::fs::create_dir_all(root.join("config")).expect("建目录");
        let dest = root.join("config/a.toml");
        let backup = root.join("config/a.toml.ullbak-test");
        std::fs::write(&dest, b"new").expect("写新文件");
        std::fs::write(&backup, b"old").expect("写备份");
        let mut journal = vec![applied(dest.clone(), Some(backup.clone()), true)];

        let outcome = rollback_merge(&mut journal);

        assert!(outcome.is_complete(), "{outcome:?}");
        assert_eq!(outcome.attempted, 2);
        assert_eq!(
            std::fs::read(&dest).expect("读回"),
            b"old",
            "备份要改回原名"
        );
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rollback_failures_are_reported_not_swallowed() {
        // 目标位置上是个**非空目录**：`remove_file` 与随后的备份改回都会失败
        // （Windows / Unix 一致），于是"回滚失败"这条路径被确定性地打到。
        let root = std::env::temp_dir().join(format!("ull-rb-fail-{}", std::process::id()));
        let dest = root.join("a.toml");
        std::fs::create_dir_all(dest.join("occupied")).expect("用非空目录占住目标");
        let backup = root.join("a.toml.ullbak-test");
        std::fs::write(&backup, b"old").expect("写备份");
        let mut journal = vec![applied(dest.clone(), Some(backup), true)];

        let outcome = rollback_merge(&mut journal);

        assert!(!outcome.is_complete(), "失败必须被记下来：{outcome:?}");
        assert_eq!(outcome.failure_count(), 2, "{outcome:?}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
