//! 递归删除一个世界目录（**不跟随符号链接**，层级有上限）。
//!
//! # 为什么不直接 `std::fs::remove_dir_all`
//!
//! 两个理由，都不是洁癖：
//!
//! 1. **要能说清「删了什么」**。`remove_dir_all` 是一次黑盒调用，失败时只给「最后一个错误」；
//!    这里逐条走，出错时能指认是哪一条路径（[`SavesWriteError::path`] 就是它）。
//! 2. **要有一条自己写死的不跟随判据**。标准库文档保证 `remove_dir_all` 不跟随符号链接，
//!    但它是「实现保证」而不是「本模块的判据」——本模块把这件事写进自己的循环里：
//!    `symlink_metadata` 判形态，链接只删链接本身（[`remove_link`]），绝不递归进去。
//!    unix 下 `remove_file` 对「指向目录的链接」也有效（unlink 删的是链接），
//!    Windows 下目录链接要 `remove_dir`——两个都试一次，这就是 `remove_link` 的全部内容。
//!
//! # 层级上限
//!
//! 超过 [`MAX_DEPTH`] 层就报 [`SavesWriteKind::Boundary`]：真实的存档是
//! `world/{region,DIM-1/region,...}` 这种三到四层，六十层只可能是「这不是一个世界目录」。
//! 没有这道闸时，一次误传的路径会在删除上耗掉用户不想要的长时间（而删除是不可逆的）。
//!
//! # 边界
//!
//! 判据是「检查那一刻」的快照：检查与 `read_dir` 之间被人换掉组件（TOCTOU）看不到。
//! 本命令不是并发/对抗入口，不做跨进程锁（与 `game::adopt::run_dir` 同一条已知边界）。

use std::path::{Path, PathBuf};

use super::boundary::guard;
use super::{SavesWriteError, SavesWriteKind};

/// 递归删除的最大层级（见模块文档）。
pub const MAX_DEPTH: usize = 64;

/// 删除一个世界目录（递归），返回被删掉的那个目录（**解析后的真实路径**）。
///
/// 第一句永远是 [`guard`]：源必须是 `<run_dir>/saves` 下的真目录。越界、符号链接、
/// 不存在的目标各自给具名类别，**一个字节都不删**。
///
/// # Errors
///
/// | 情况 | 类别 |
/// |---|---|
/// | 源越界 / 是链接 / 不在 / 不是目录 | `Boundary` / `SymbolicLink` / `NotFound` / `NotADirectory` |
/// | 目录层级超过 [`MAX_DEPTH`] | [`SavesWriteKind::Boundary`] |
/// | 删某一条失败（权限 / 被占用） | [`SavesWriteKind::Io`]（`path` 指出卡在哪一条） |
pub fn delete_world(run_dir: &Path, name: &str) -> Result<PathBuf, SavesWriteError> {
    let target = guard(run_dir, name)?;
    remove_tree(&target.world, 0)?;
    Ok(target.world)
}

/// 删掉 `dir` 这一棵（`depth` 从 0 起算）。
fn remove_tree(dir: &Path, depth: usize) -> Result<(), SavesWriteError> {
    if depth > MAX_DEPTH {
        return Err(SavesWriteError::new(
            dir,
            SavesWriteKind::Boundary,
            format!("目录层级超过 {MAX_DEPTH} 层，不像一个世界目录：拒绝继续往下删"),
        ));
    }
    let read = std::fs::read_dir(dir).map_err(|error| SavesWriteError::io(dir, &error))?;
    for entry in read {
        let entry = entry.map_err(|error| SavesWriteError::io(dir, &error))?;
        let path = entry.path();
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            // 刚被判据挡住又在两步之间消失：目标已经没了，继续删剩下的。
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(SavesWriteError::io(&path, &error)),
        };
        let file_type = meta.file_type();
        if file_type.is_symlink() {
            // **只删链接本身**：链接指向的东西不在本操作的范围里。
            remove_link(&path)?;
        } else if file_type.is_dir() {
            remove_tree(&path, depth + 1)?;
        } else {
            std::fs::remove_file(&path).map_err(|error| SavesWriteError::io(&path, &error))?;
        }
    }
    std::fs::remove_dir(dir).map_err(|error| SavesWriteError::io(dir, &error))
}

/// 删一条符号链接自身（unix 用 `remove_file`，Windows 的目录链接要用 `remove_dir`）。
fn remove_link(path: &Path) -> Result<(), SavesWriteError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(first) => std::fs::remove_dir(path).map_err(|_| SavesWriteError::io(path, &first)),
    }
}
