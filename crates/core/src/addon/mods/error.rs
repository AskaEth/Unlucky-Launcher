//! 本地 Mod 读写的失败形状（CLI/UI 据此给具名原因，不折叠成一句「失败」）。

use std::fmt;
use std::path::PathBuf;

/// 一次列目录 / 启停失败。
#[derive(Debug)]
pub enum ModError {
    /// 文件名不是**裸文件名**（含分隔符 / `..` / 盘符 / NUL）。
    ///
    /// 判据直接复用整合包侧的七条路径规则（[`crate::addon::pack::path::normalize_file`]），
    /// 再要求结果里没有 `/`：启停只接受 `mods/` 下**直接**的一个文件，
    /// 不收相对路径——那等于让 UI 决定落到哪个子目录。
    UnsafeName {
        /// 调用方给的名字。
        file: String,
        /// 人读原因。
        detail: String,
    },
    /// `mods` 目录本身不可用：是符号链接 / 不是目录 / 读不了。
    ///
    /// 符号链接**不跟随**：`<run>/mods` 指到别处时，改名会落到实例目录之外。
    ModsDirUnusable {
        /// 那个目录。
        dir: PathBuf,
        /// 人读原因。
        detail: String,
    },
    /// 要启停的文件不在 `mods/` 里。
    NotFound {
        /// 文件名。
        file: String,
        /// 找过的目录。
        dir: PathBuf,
    },
    /// 目标存在但不是普通文件（目录 / 符号链接 / 设备）。
    NotARegularFile {
        /// 文件名。
        file: String,
        /// 人读原因。
        detail: String,
    },
    /// 目标名字已被占用（**不覆盖**：那可能是另一个 mod）。
    Collision {
        /// 原名字。
        file: String,
        /// 想要的（已存在的）目标名。
        target: String,
        /// 目录。
        dir: PathBuf,
    },
    /// 改名本身失败（占用 / 权限 / 磁盘异常）。
    Rename {
        /// 原名字。
        file: String,
        /// 目标名。
        target: String,
        /// 底层原因。
        detail: String,
    },
}

impl fmt::Display for ModError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsafeName { file, detail } => {
                write!(f, "文件名不可用于启停（{file:?}）：{detail}")
            }
            Self::ModsDirUnusable { dir, detail } => {
                write!(f, "mods 目录不可用（{}）：{detail}", dir.display())
            }
            Self::NotFound { file, dir } => {
                write!(f, "mods 目录里没有 {file}（{}）", dir.display())
            }
            Self::NotARegularFile { file, detail } => {
                write!(f, "{file} 不是普通文件：{detail}")
            }
            Self::Collision { file, target, dir } => write!(
                f,
                "{file} 改不成 {target}：同名文件已存在（{}）——不覆盖，请先处理那个文件",
                dir.join(target).display()
            ),
            Self::Rename {
                file,
                target,
                detail,
            } => write!(f, "{file} → {target} 改名失败：{detail}"),
        }
    }
}

impl std::error::Error for ModError {}
