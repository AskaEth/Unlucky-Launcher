//! 启停一个 mod：**在 `mods/` 里改名**（`.disabled` 后缀就是禁用位）。
//!
//! # 为什么是改名而不是写配置文件
//!
//! 游戏本身不读任何"禁用列表"——它只加载 `mods/*.jar`。`.disabled` 后缀让文件**不再匹配**
//! 加载器的 glob，于是"禁用"是**文件系统状态**：换启动器、换机器、手动 `git checkout` 之后
//! 依然成立。这也是 Modrinth 官方导出表达禁用位的唯一方式（见
//! [`crate::addon::pack::manifest::PackFile::disabled`]）。
//!
//! # 五条守卫（都不猜、都不覆盖）
//!
//! | 守卫 | 理由 |
//! |---|---|
//! | 只收**裸文件名**（过七条路径规则且不含 `/`） | 启停不接受相对路径，否则 UI 能指挥落点 |
//! | `mods/` 是符号链接就拒 | 改名会落到实例目录之外 |
//! | 目标必须是**普通文件**（非目录 / 非链接） | 改一个目录或链接不是"启停一个 mod" |
//! | 目标名已存在就拒（[`ModError::Collision`]） | 覆盖掉的是**另一个 mod**，用户没同意 |
//! | rename 不做跨盘兜底（不 copy+delete） | copy 半截失败会留下两个文件，语义比报错更差 |
//!
//! # 幂等
//!
//! 「已经启用还要启用」不是错误：返回 `renamed = false` 的成功结果（UI 的开关重复点不会
//! 变成红字）。这一条由 [`super::suffix::renamed_for`] 的 `None` 表达。

use std::path::{Path, PathBuf};

use super::error::ModError;
use super::list::mods_dir;
use super::suffix::{is_disabled_path, renamed_for};

/// 一次启停的结果（`from == to` 且 `renamed == false` ⇒ 本来就是那个状态）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModToggle {
    /// 原文件名的磁盘名（含后缀）。
    pub from: String,
    /// 改名后的磁盘名（没改名时与 `from` 相同）。
    pub to: String,
    /// 这一步之后的启用状态。
    pub enabled: bool,
    /// 是否真的在磁盘上改了名。
    pub renamed: bool,
}

/// 把 `mods/` 下的某个文件切到 `enabled` 状态（改名）。
///
/// # Errors
///
/// 见模块文档的五条守卫：[`ModError::UnsafeName`] / [`ModError::ModsDirUnusable`] /
/// [`ModError::NotFound`] / [`ModError::NotARegularFile`] / [`ModError::Collision`] /
/// [`ModError::Rename`]。
pub fn set_enabled(run_dir: &Path, file: &str, enabled: bool) -> Result<ModToggle, ModError> {
    let name = bare_name(file)?;
    let dir = mods_dir(run_dir);
    usable_dir(&dir)?;

    let from_path = dir.join(&name);
    let meta = std::fs::symlink_metadata(&from_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ModError::NotFound {
                file: name.clone(),
                dir: dir.clone(),
            }
        } else {
            ModError::NotARegularFile {
                file: name.clone(),
                detail: error.to_string(),
            }
        }
    })?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(ModError::NotARegularFile {
            file: name.clone(),
            detail: "不是普通文件（符号链接 / 目录 / 设备都不动）".to_string(),
        });
    }

    let Some(target) = renamed_for(&name, enabled) else {
        // 已经是目标状态：幂等返回，不产生多余 rename（Windows 上同大小写改名还可能失败）。
        return Ok(ModToggle {
            enabled: !is_disabled_path(&name),
            from: name.clone(),
            to: name,
            renamed: false,
        });
    };

    let to_path = dir.join(&target);
    if std::fs::symlink_metadata(&to_path).is_ok() {
        return Err(ModError::Collision {
            file: name,
            target,
            dir,
        });
    }

    std::fs::rename(&from_path, &to_path).map_err(|error| ModError::Rename {
        file: name.clone(),
        target: target.clone(),
        detail: error.to_string(),
    })?;

    Ok(ModToggle {
        from: name,
        to: target,
        enabled,
        renamed: true,
    })
}

/// 文件名必须是**裸文件名**：先过整合包侧的七条路径规则，再要求没有 `/`。
fn bare_name(file: &str) -> Result<String, ModError> {
    let trimmed = file.trim();
    let normalized = crate::addon::pack::path::normalize_file(trimmed).map_err(|error| {
        ModError::UnsafeName {
            file: file.to_string(),
            detail: error.to_string(),
        }
    })?;
    if normalized.contains('/') || normalized != trimmed {
        return Err(ModError::UnsafeName {
            file: file.to_string(),
            detail: "只接受 mods/ 下的裸文件名（不含路径分隔符、不做归一）".to_string(),
        });
    }
    Ok(normalized)
}

/// `mods/` 必须存在、是目录、且**不是符号链接**。
fn usable_dir(dir: &Path) -> Result<(), ModError> {
    let meta = std::fs::symlink_metadata(dir).map_err(|error| ModError::ModsDirUnusable {
        dir: dir.to_path_buf(),
        detail: if error.kind() == std::io::ErrorKind::NotFound {
            "不存在（这个实例还没有 mods 目录）".to_string()
        } else {
            error.to_string()
        },
    })?;
    if meta.file_type().is_symlink() {
        return Err(ModError::ModsDirUnusable {
            dir: dir.to_path_buf(),
            detail: "是符号链接（不跟随：改名会落到实例目录之外）".to_string(),
        });
    }
    if !meta.is_dir() {
        return Err(ModError::ModsDirUnusable {
            dir: dir.to_path_buf(),
            detail: "不是目录".to_string(),
        });
    }
    Ok(())
}

/// 建 `mods/` 目录（**只在装 mod 时用**；列目录/启停不建）。
///
/// # Errors
///
/// 建不出来 / 建出来是符号链接以外的怪形状时给 [`ModError::ModsDirUnusable`]。
pub fn ensure_dir(run_dir: &Path) -> Result<PathBuf, ModError> {
    let dir = mods_dir(run_dir);
    std::fs::create_dir_all(&dir).map_err(|error| ModError::ModsDirUnusable {
        dir: dir.clone(),
        detail: error.to_string(),
    })?;
    usable_dir(&dir)?;
    Ok(dir)
}
