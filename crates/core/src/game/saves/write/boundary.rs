//! 写操作的**边界校验**与重命名。
//!
//! # 三道校验，每次重做
//!
//! [`guard`] 是三个写操作共同的第一句：
//!
//! | 第几道 | 查什么 | 失败类别 |
//! |---|---|---|
//! | 1 | 名字是单段裸名字（`normalize_file` 之后与原串**原样相等**） | [`SavesWriteKind::UnsafeName`] |
//! | 2 | `<run_dir>/saves` 是**运行目录的直属子目录**、不是链接、是目录 | `SymbolicLink` / `NotADirectory` / `Boundary` |
//! | 3 | `saves/<name>` 是真目录（`symlink_metadata`：链接与非目录一律拒） | `SymbolicLink` / `NotADirectory` / `Boundary` |
//!
//! ## 为什么第 2 道比的是「父级」而不是字符串前缀
//!
//! `Path::starts_with` 是**逐段**比较，看起来够用；但它对「`saves/` 被换成一个指向实例外的
//! 目录联接」这种情况答不了——解析之后父级已经不是运行目录了。所以两道都比**解析后的父级**：
//! `saves` 的父级必须**就是**运行目录本身，世界目录的父级必须**就是** `saves`。这样
//! 「把 `saves` 换成链接」「把世界目录换成链接」两类越界都在同一条判据上被挡住，
//! 不依赖字符串长什么样。
//!
//! ## 为什么第 1 道要「原样相等」而不是「规整后能用」
//!
//! `normalize_file` 会**丢弃** `.` 段与空段（`a//b` → `a/b`）。对 zip 条目名那是好意，对
//! 磁盘上的目录名却是**改名**：用户输入 `a//b`，我们悄悄去动 `a/b`。所以这里要求规整结果与输入
//! 一字不差——超出的部分（`..`、`/`、`\`、`:`、NUL、绝对路径）由 `normalize_file` 一律拒掉。
//!
//! # 重命名的失败顺序
//!
//! 先校验**源**（必须是一个真世界），再校验**目标名字**，最后才碰磁盘。目标位置上
//! **存在任何东西**（目录、文件、坏链接——`symlink_metadata` 都看得见）都算冲突：
//! 「覆盖别人」这件事不在本 mission 的语义里。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::{SavesWriteError, SavesWriteKind};
use crate::game::saves::{saves_dir, SAVES_DIR};

/// 一次写操作**已校验**的目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Target {
    /// `<run_dir>/saves`（**解析后的真实路径**：后续拼名字都拿它当父级）。
    pub(super) saves: PathBuf,
    /// `<run_dir>/saves/<name>`（**解析后的真实目录**）。
    ///
    /// 它就是这一拍要动的那棵树。解析过 ⇒ 后面 `rename` / 递归删 / 打包都不必再猜路径形态。
    pub(super) world: PathBuf,
}

/// 名字 → 单段裸名字（不过规则就给具名错误）。
///
/// `anchor` 只用于错误里的路径：给「用户以为自己在动的那一条」。
pub(super) fn check_name(anchor: &Path, name: &str) -> Result<(), SavesWriteError> {
    if name.is_empty() {
        return Err(SavesWriteError::new(
            anchor,
            SavesWriteKind::UnsafeName,
            "名字为空",
        ));
    }
    // 判据复用整合包侧那一份（不在存档这边写第二套规则），再补一条「规整后必须原样相等且只有一段」。
    // `normalize_file("a/b")` 会把 `a/b` 原样返回（那是包内路径，本来就允许多段），可磁盘上的
    // **目录名**多一段就落到别处去了——所以这里还要判「只有一段」。
    match crate::addon::pack::path::normalize_file(name) {
        Ok(normalized) if normalized == name && !normalized.contains('/') => Ok(()),
        Ok(_) => Err(SavesWriteError::new(
            anchor,
            SavesWriteKind::UnsafeName,
            format!("名字 `{name}` 不是单段（`/` `\\` `:` `.` 段都不许出现）"),
        )),
        Err(error) => Err(SavesWriteError::new(
            anchor,
            SavesWriteKind::UnsafeName,
            format!("名字 `{name}` 过不了路径规则：{error}"),
        )),
    }
}

/// `<run_dir>` + 世界名 → 已校验的目标（见模块文档的三道）。
pub(super) fn guard(run_dir: &Path, name: &str) -> Result<Target, SavesWriteError> {
    check_name(run_dir, name)?;

    let run_real = real_dir(run_dir)?;
    let saves = saves_dir(run_dir);
    let saves_real = real_dir(&saves)?;
    // 第 2 道：`saves` 必须是**运行目录的直属子目录**（解析后比父级，不比字符串前缀）。
    if saves_real.parent() != Some(run_real.as_path())
        || saves_real.file_name() != Some(OsStr::new(SAVES_DIR))
    {
        return Err(SavesWriteError::new(
            &saves,
            SavesWriteKind::Boundary,
            format!(
                "{} 解析到 {}，不是 {} 的直属 {SAVES_DIR} 目录：拒绝沿它写盘",
                saves.display(),
                saves_real.display(),
                run_real.display()
            ),
        ));
    }

    let world = saves_real.join(name);
    let world_real = real_dir(&world)?;
    // 第 3 道：世界目录必须**直接**挂在 `saves/` 下面（解析后比父级）。
    if world_real.parent() != Some(saves_real.as_path()) {
        return Err(SavesWriteError::new(
            &world,
            SavesWriteKind::Boundary,
            format!(
                "{} 解析到 {}，不在 {} 之下：拒绝沿它写盘",
                world.display(),
                world_real.display(),
                saves_real.display()
            ),
        ));
    }

    Ok(Target {
        saves: saves_real,
        world: world_real,
    })
}

/// 路径 → **解析后的真实目录**（不存在 / 是链接 / 不是目录各给具名类别）。
///
/// `symlink_metadata` 先判形态，`canonicalize` 再解析：只解析不看形态的话，`canonicalize`
/// 会把链接一路跟到底，越界就变成了「正常」。
fn real_dir(path: &Path) -> Result<PathBuf, SavesWriteError> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(SavesWriteError::new(
                path,
                SavesWriteKind::NotFound,
                "目录不存在",
            ));
        }
        Err(error) => return Err(SavesWriteError::io(path, &error)),
    };
    if meta.file_type().is_symlink() {
        return Err(SavesWriteError::new(
            path,
            SavesWriteKind::SymbolicLink,
            "是符号链接（不跟随：跟下去就动到实例目录之外的东西了）",
        ));
    }
    if !meta.is_dir() {
        return Err(SavesWriteError::new(
            path,
            SavesWriteKind::NotADirectory,
            "不是目录",
        ));
    }
    std::fs::canonicalize(path).map_err(|error| SavesWriteError::io(path, &error))
}

/// 重命名一个世界目录：`saves/<from>` → `saves/<to>`。
///
/// 返回**新的世界目录**（解析后的路径）。判据顺序见模块文档：源、目标名字、目标位置，最后才动盘。
///
/// # Errors
///
/// | 情况 | 类别 |
/// |---|---|
/// | 源不是 `saves/` 里的真目录（越界 / 链接 / 不在 / 不是目录） | `Boundary` / `SymbolicLink` / `NotFound` / `NotADirectory` |
/// | 新名字过不了名字规则 | [`SavesWriteKind::UnsafeName`] |
/// | 新名字与原名字相同，或目标位置已存在任何东西 | [`SavesWriteKind::Conflict`] |
/// | `rename` 本身失败（权限 / 被占用） | [`SavesWriteKind::Io`] |
pub fn rename_world(run_dir: &Path, from: &str, to: &str) -> Result<PathBuf, SavesWriteError> {
    let target = guard(run_dir, from)?;
    // 目标名字先在**已校验的 saves 目录**上查规则（锚点用 saves，比 run_dir 更贴用户看到的那一条）。
    check_name(&target.saves, to)?;
    let dest = target.saves.join(to);
    if to == from {
        return Err(SavesWriteError::new(
            &dest,
            SavesWriteKind::Conflict,
            "新名字与原名字相同",
        ));
    }
    match std::fs::symlink_metadata(&dest) {
        // 目标位置上有东西（目录 / 文件 / 坏链接）就拒：本操作不覆盖别人。
        Ok(_) => {
            return Err(SavesWriteError::new(
                &dest,
                SavesWriteKind::Conflict,
                "目标位置已存在（先给它换个名字，或换一个名字给这个世界）",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(SavesWriteError::io(&dest, &error)),
    }
    std::fs::rename(&target.world, &dest)
        .map_err(|error| SavesWriteError::io(&target.world, &error))?;
    Ok(dest)
}
