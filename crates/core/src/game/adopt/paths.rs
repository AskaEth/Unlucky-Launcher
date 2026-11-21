//! 路径判据：版本 id 能不能当路径用、已有条目是不是跑出了游戏根（符号链接 / 目录联接 / 祖先链接）。
//!
//! 两条判据都与「登记只读」直接相关：id 不可信 ⇒ `versions/<id>/` 可能落到别人的目录上；
//! 越界 ⇒ 我们以为在游戏根里的东西其实在外面（读它就等于读别的启动器的数据）。
//! 判据写成纯函数，好在这一层单独变异。

use std::path::{Path, PathBuf};

use super::super::layout::GameLayout;
use super::candidate::AdoptProblem;

/// 游戏根的「真实」形式（符号链接越界判据的比较基准）。
///
/// 拿不到（根还不存在 / 无权限）时退回原始路径：那种情况下不会有已存在的符号链接能通过元数据检查，
/// 越界判据仍能给出「不在根内」的结论，而不是静默放行。
pub(super) fn root_of(game: &GameLayout) -> PathBuf {
    game.root()
        .canonicalize()
        .unwrap_or_else(|_| game.root().to_path_buf())
}

/// 边界判据：**已存在**的路径，其真实（解析符号链接/目录联接之后）位置必须在游戏根内。
///
/// 为什么不是 `is_symlink` 一刀切：
/// - `is_symlink` 看不到 Windows 的**目录联接（junction）**和大部分重解析点——而 junction 不需要
///   管理员权限就能建，正是「把别人的版本目录挂进来」最容易的形态；
/// - 祖先链接同样要算：`versions/` 本身是 junction 时，每个 `versions/<id>` 都解析到根外，
///   单看 `<id>` 这一层还是普通目录。
///
/// 所以这里一律 `canonicalize` 后与**真实游戏根**做前缀比较：根内的链接照旧放行（合法的省盘做法），
/// 根外的一律给出 [`AdoptProblem::OutsideGameRoot`]。路径不存在 ⇒ `None`（存在性由调用方的
/// 「清单缺失」判据负责，别把「还没装」说成「越界」）。
///
/// **已知边界**：判据是「检查那一刻」的快照，检查与读取之间若有人把目录换成链接（TOCTOU），
/// 本函数看不到——本模块不是并发/对抗入口，不做跨进程锁。
pub(super) fn ensure_inside(path: &Path, root: &Path) -> Option<AdoptProblem> {
    if std::fs::symlink_metadata(path).is_err() {
        return None;
    }
    match path.canonicalize() {
        Ok(target) if target.starts_with(root) => None,
        Ok(target) => Some(AdoptProblem::OutsideGameRoot {
            path: path.to_path_buf(),
            target,
            root: root.to_path_buf(),
        }),
        Err(error) => Some(AdoptProblem::SymlinkUnresolvable {
            path: path.to_path_buf(),
            message: error.to_string(),
        }),
    }
}

/// 版本 id 的路径安全判据（空 / 分隔符 / 盘符 / NUL / 结尾点号空格都拒绝）。
///
/// 结尾的点号与空格必须拒绝：Win32 会在建目录时把它们**静默剥掉**，于是
/// `versions/1.20.1.` 与 `versions/1.20.1` 会落到同一个目录上——两个「不同 id」指向一份文件。
pub(super) fn validate_version_id(version_id: &str) -> Result<(), AdoptProblem> {
    let invalid = |reason: &'static str| {
        Err(AdoptProblem::VersionIdInvalid {
            version_id: version_id.to_owned(),
            reason,
        })
    };
    if version_id.trim().is_empty() {
        return invalid("为空");
    }
    if version_id.contains('/') || version_id.contains('\\') {
        return invalid("含路径分隔符");
    }
    if version_id.contains(':') {
        return invalid("含冒号（盘符 / NTFS 数据流）");
    }
    if version_id.contains('\0') || version_id.chars().any(|ch| ch.is_control()) {
        return invalid("含控制字符");
    }
    if version_id == "." || version_id == ".." {
        return invalid("是相对路径标记");
    }
    if version_id.ends_with('.') || version_id.ends_with(' ') {
        return invalid("以点号或空格结尾（Win32 会剥掉，落到别的目录上）");
    }
    Ok(())
}
