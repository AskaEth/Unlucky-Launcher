//! 落地事务：**临时目录 + rename**，合并态用 journal 回滚（回滚细节在 [`commit`]）。
//!
//! # 两种起点，两条事务路径
//!
//! | 起点 | 做法 | 失败后的样子 |
//! |------|------|--------------|
//! | 目标目录不存在 / 是空目录（全新导入） | 在**同卷兄弟目录**里把整个实例建好，最后一次 `rename` 到位 | 目标目录**从未出现**（rename 是原子的） |
//! | 目标目录已有内容（升级 / 重导） | 逐文件 `rename` 进目标；被替换的旧文件先改名成 `<dest>.ullbak-<pid>` 当备份 | journal 逆序回滚：新文件删掉、旧备份改回原名 |
//!
//! 为什么全新导入不逐文件搬：逐文件搬意味着"搬到一半失败"会留下一个**看起来存在**的
//! 半个实例，而 `rename` 一个目录是单步原子操作——失败就只有"没有"这一种结果。
//!
//! # 取消落在这层怎么办（M195）
//!
//! | 起点 | 取消点 | 收场 |
//! |------|--------|------|
//! | 全新导入 | **只在 `rename` 之前** | 删暂存目录，目标目录从未出现 |
//! | 合并态 | 每条搬动之前 | journal **逆序**撤回（新文件删掉、旧备份改回原名） |
//!
//! 合并态逐文件报进度（`ImportStage::Commit`）：既是界面该有的动静，也是取消能落下的地方。
//!
//! # 两道路径检查都在这层再走一遍
//!
//! 第一道是 M110 的 [`normalize_file`]（七条写死规则：空名 / `\0` / `\` / `:` / 前导 `/` /
//! 任何 `..` 段 / 规整后为空），第二道是**落盘前的 `starts_with` 复核**（Modrinth 官方安装器
//! 同款做法：`install_mrpack.rs:169-170` 按 `strip_prefix(instance_root)` 再判一次）。
//! 模型可能来自外部构造，`zip` 侧的双检挡不住"调用方直接塞了个 `../x` 进来"。

pub mod commit;

pub use commit::{CommitFailure, CommitGuard, CommitStop, RollbackOutcome};

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::super::pack::path::normalize_file;
use super::super::pack::PackError;
use super::cancel::{ImportCancel, Stopped};
use super::error::ImportError;
use super::report::tell;
use super::report::{ImportProgressSink, ImportStage};
use crate::platform::atomic::write_atomic;

use crate::download::atomic::{META_SUFFIX, PART_SUFFIX};
use commit::{rollback_merge, Applied};

/// 临时目录名里的进程内序号（同一进程并发导入不撞名）。
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 目标目录的起点形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// 全新：整目录 rename。
    Fresh,
    /// 合并：逐文件 rename + journal。
    Merge,
}

/// 一个进行中的落地事务。
#[derive(Debug)]
pub struct Staging {
    target: PathBuf,
    temp: PathBuf,
    mode: Mode,
    /// 本次是否由我们创建了目标目录的父目录（回滚时顺手清掉，避免"空壳目录"残留）。
    created_parent: bool,
}

impl Staging {
    /// 开一个事务（临时目录立刻建好，目标目录此刻一个字节都没动）。
    ///
    /// # Errors
    ///
    /// 目标路径是文件、父目录建不出来、临时目录建不出来时返回 `pack_io`。
    pub fn begin(target: &Path) -> Result<Self, ImportError> {
        if target.is_file() {
            return Err(io_error(format!(
                "目标路径是个文件，不是目录：{}",
                target.display()
            )));
        }
        let parent = target
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let created_parent = !parent.exists();
        std::fs::create_dir_all(&parent)
            .map_err(|error| io_error(format!("建 {} 失败：{error}", parent.display())))?;

        let name = target.file_name().map_or_else(
            || "instance".to_string(),
            |name| name.to_string_lossy().to_string(),
        );
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let temp = parent.join(format!(".{name}.ull-import-{}-{seq}", std::process::id()));
        if temp.exists() {
            std::fs::remove_dir_all(&temp).map_err(|error| {
                io_error(format!("清旧临时目录 {} 失败：{error}", temp.display()))
            })?;
        }
        std::fs::create_dir_all(&temp)
            .map_err(|error| io_error(format!("建临时目录 {} 失败：{error}", temp.display())))?;

        let mode = match std::fs::read_dir(target) {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    Mode::Merge
                } else {
                    Mode::Fresh
                }
            }
            Err(_) => Mode::Fresh,
        };
        Ok(Self {
            target: target.to_path_buf(),
            temp,
            mode,
            created_parent,
        })
    }

    /// 临时目录根（调试 / 测试断言用）。
    #[must_use]
    pub fn temp(&self) -> &Path {
        &self.temp
    }

    /// 目标目录。
    #[must_use]
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// 是否合并态（目标目录已有内容）。
    #[must_use]
    pub const fn is_merge(&self) -> bool {
        matches!(self.mode, Mode::Merge)
    }

    /// 规划一个暂存路径：走**两道**路径检查后返回可写的绝对路径。
    ///
    /// # Errors
    ///
    /// 路径穿越 / 空路径 / 规整后越出临时目录时返回 `pack_traversal`。
    pub fn dest_for(&self, relative: &str) -> Result<PathBuf, ImportError> {
        let normalized = normalize_file(relative)?;
        let path = self.temp.join(&normalized);
        // 第二检：规整之后仍要确认它真的在临时目录下面（不靠 `join` 的语义"讲道理"）。
        if !path.starts_with(&self.temp) {
            return Err(PackError::traversal(format!("暂存路径越出临时目录：{relative:?}")).into());
        }
        Ok(path)
    }

    /// 写一个暂存文件（自动建父目录）。
    ///
    /// # Errors
    ///
    /// 路径不安全、写盘失败。
    pub fn write(&self, relative: &str, bytes: &[u8]) -> Result<PathBuf, ImportError> {
        let dest = self.dest_for(relative)?;
        write_atomic(&dest, bytes)
            .map_err(|error| io_error(format!("写 {} 失败：{error}", dest.display())))?;
        Ok(dest)
    }

    /// 删掉一个暂存文件（例如发现内容与磁盘一致，不必落地）。
    pub fn discard(&self, relative: &str) -> Result<(), ImportError> {
        let dest = self.dest_for(relative)?;
        match std::fs::remove_file(&dest) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error(format!("删 {} 失败：{error}", dest.display()))),
        }
    }

    /// 放弃事务：临时目录整个删掉，目标目录保持导入前的样子。
    pub fn abort(self) {
        self.abort_inner();
    }

    fn abort_inner(&self) {
        let _ = std::fs::remove_dir_all(&self.temp);
        if self.created_parent {
            // 只删空目录：非空说明有别人用了，别动。
            let _ = std::fs::remove_dir(self.temp.parent().unwrap_or(&self.temp));
        }
    }

    /// 提交事务：返回一个**必须显式 `finish`** 的守卫（析构即回滚）。
    ///
    /// `cancel` 是取消令牌，`progress` 是进度回调（合并态**逐文件**报一条：几千文件的合并本来
    /// 就该看得见动静，而"取消"也正需要文件之间的落点）。
    ///
    /// # Errors
    ///
    /// - [`CommitStop::Cancelled`]：取消落在提交前 / 合并中途。**回滚已经在这条路径上做过**，
    ///   结果一并返回（调用方把它写进报告的 `rollback` 机器位）；
    /// - [`CommitStop::Failed`]：`rename` 失败、合并态逐文件搬动失败。回滚同样已经做过，其结果
    ///   随错误返回（[`CommitFailure::rollback`]）：调用方不许假设"失败 = 已回滚干净"。
    ///
    /// 全新导入只有一次 `rename`：那是单步原子操作，**取消点只在它之前**——之后不存在"半个
    /// 实例"这种中间态，也就没有"取消"这一说。
    pub fn commit(
        self,
        cancel: &ImportCancel,
        progress: &Option<ImportProgressSink>,
    ) -> Result<CommitGuard, CommitStop> {
        match self.mode {
            Mode::Fresh => {
                if cancel.is_cancelled() {
                    // 还没 rename：目标目录一个字节都没动，删掉暂存目录就是零残留。
                    self.abort_inner();
                    return Err(CommitStop::Cancelled(RollbackOutcome::default()));
                }
                match std::fs::rename(&self.temp, &self.target) {
                    Ok(()) => Ok(CommitGuard::fresh(self.target.clone(), self.temp.clone())),
                    Err(error) => {
                        self.abort_inner();
                        Err(CommitStop::Failed(CommitFailure::io(
                            format!(
                                "把 {} 改名成 {} 失败：{error}",
                                self.temp.display(),
                                self.target.display()
                            ),
                            RollbackOutcome::default(),
                        )))
                    }
                }
            }
            Mode::Merge => {
                let mut applied: Vec<Applied> = Vec::new();
                match self.apply_merge(&mut applied, cancel, progress) {
                    Ok(()) => Ok(CommitGuard::merge(
                        self.target.clone(),
                        self.temp.clone(),
                        applied,
                    )),
                    Err(Stopped::Cancelled) => {
                        let rollback = rollback_merge(&mut applied);
                        self.abort_inner();
                        Err(CommitStop::Cancelled(rollback))
                    }
                    Err(Stopped::Failed(error)) => {
                        let rollback = rollback_merge(&mut applied);
                        self.abort_inner();
                        Err(CommitStop::Failed(CommitFailure::new(error, rollback)))
                    }
                }
            }
        }
    }

    /// 删掉暂存目录里的**下载引擎中间产物**（续传 part 与 meta），返回删掉的个数。
    ///
    /// 为什么必须在提交前删：引擎在下载失败时**故意**保留 `<name>.ullpart-<session>-<seq>`
    /// 与 `<name>.ullmeta.json` 供下次续传（`core::download::atomic` 的设计）。这些文件躺在
    /// 暂存目录里，提交阶段会跟内容一起搬进实例——实例里冒出 `.ullpart-…` 既不是整合包内容，
    /// 又会被下次导入的基线/冲突判定当成"用户文件"，从此谁也说不清。
    ///
    /// 冲突口径：整合包里真有一个叫 `x.ullpart` 的文件几乎不可能；真出现时以"不入实例"为准。
    ///
    /// # Errors
    ///
    /// 遍历或删除失败返回 `pack_io`。
    pub fn purge_engine_artifacts(&self) -> Result<usize, ImportError> {
        let mut files: Vec<PathBuf> = Vec::new();
        collect_files(&self.temp, &mut files)?;
        let mut removed = 0_usize;
        for path in files {
            if !is_engine_artifact(&path) {
                continue;
            }
            std::fs::remove_file(&path).map_err(|error| {
                io_error(format!("删中间产物 {} 失败：{error}", path.display()))
            })?;
            removed += 1;
        }
        Ok(removed)
    }

    /// 合并态：把临时目录里的文件逐个搬进目标目录（每个都带备份）。
    ///
    /// **逐文件取消点**：每条搬动之前查一次令牌，已取消 ⇒ `Err(Stopped::Cancelled)`，调用方
    /// 按 journal 逆序回滚（新文件删掉、旧备份改回原名）。
    fn apply_merge(
        &self,
        applied: &mut Vec<Applied>,
        cancel: &ImportCancel,
        progress: &Option<ImportProgressSink>,
    ) -> Result<(), Stopped> {
        let mut staged: Vec<PathBuf> = Vec::new();
        collect_files(&self.temp, &mut staged)?;
        staged.sort();
        let total = staged.len();
        for (index, source) in staged.into_iter().enumerate() {
            cancel.stopped()?;
            let relative = source
                .strip_prefix(&self.temp)
                .map_err(|_| io_error(format!("暂存路径不在临时目录里：{}", source.display())))?;
            let relative_text = relative.to_string_lossy().replace('\\', "/");
            tell(
                progress,
                ImportStage::Commit,
                index,
                total,
                Some(relative_text.clone()),
            );
            // 搬动前再规整一次（第三道，防"临时目录里被塞了奇怪的名字"）。
            let normalized = normalize_file(&relative_text)?;
            let dest = self.target.join(&normalized);
            if !dest.starts_with(&self.target) {
                return Err(PackError::traversal(format!(
                    "落地路径越出实例目录：{relative_text:?}"
                ))
                .into());
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| io_error(format!("建 {} 失败：{error}", parent.display())))?;
            }
            let backup = if dest.exists() {
                let backup = commit::backup_path(&dest);
                std::fs::rename(&dest, &backup).map_err(|error| {
                    io_error(format!(
                        "备份 {} → {} 失败：{error}",
                        dest.display(),
                        backup.display()
                    ))
                })?;
                Some(backup)
            } else {
                None
            };
            applied.push(Applied {
                dest: dest.clone(),
                backup,
                moved: false,
            });
            std::fs::rename(&source, &dest).map_err(|error| {
                io_error(format!(
                    "把 {} 搬到 {} 失败：{error}",
                    source.display(),
                    dest.display()
                ))
            })?;
            if let Some(last) = applied.last_mut() {
                last.moved = true;
            }
        }
        Ok(())
    }
}

/// 引擎中间产物的文件名判据（`.ullpart` 在中段、`.ullmeta.json` 在结尾）。
#[must_use]
pub fn is_engine_artifact(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.ends_with(META_SUFFIX) || name.contains(PART_SUFFIX)
}

/// 递归收集目录下的全部文件（绝对路径）。
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ImportError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| io_error(format!("读 {} 失败：{error}", dir.display())))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| io_error(format!("读 {} 失败：{error}", dir.display())))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| io_error(format!("读 {} 类型失败：{error}", path.display())))?;
        if file_type.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// 本地 IO 失败（统一包成 M110 的 `pack_io`，不再造新码）。
pub(super) fn io_error(message: impl Into<String>) -> ImportError {
    PackError::io(message).into()
}
