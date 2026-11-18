//! 实例目录扫描：把"磁盘上有什么"变成 [`LocalFile`] 表（路径 + 字节数 + SHA-1 + 禁用位）。
//!
//! # 扫哪些目录（写死白名单，不是"排除法"）
//!
//! 默认只扫 [`DEFAULT_CONTENT_ROOTS`] 这 8 个内容目录：`mods` / `config` / `defaultconfigs` /
//! `scripts` / `resourcepacks` / `shaderpacks` / `kubejs` / `openloader`。
//!
//! 为什么用白名单而不是"排除 `versions/` `libraries/` `assets/`"：
//!
//! - 那三个目录是**启动器/游戏自己管的**（版本 JSON、依赖库、资源对象），塞进整合包只会让包
//!   变成几 GB，且与"远程可替代"完全无关；
//! - `journals` `logs/` `crash-reports/` `saves/` `screenshots/` `backups/` **不属于**整合包内容
//!   （社区导出器一致不打包它们）；
//! - 排除法是"忘了就漏"的写法：以后实例里出现新目录（`.mixin.out`、`essential`…）会被静默打进去。
//!
//! 白名单之外的东西**不是丢了**——它们本来就不该在整合包里。要打别的目录，用
//! [`crate::addon::export::ExportRequest::with_content_roots`] 显式加。
//!
//! # 内容目录里的一切都收（认不出的 mod 不能丢）
//!
//! 白名单目录里**不按扩展名过滤**：`mods/` 下的 `.jar`、`.jar.disabled`、`.jar.old`、
//! `.ds_store`、社区自造后缀，一律按文件收。丢掉"我不认识的东西"正是用户点名不许的。
//!
//! # 禁用它只看路径后缀
//!
//! `x.jar` / `x.jar.disabled` / `x.jar.old` / `x.jar.disabled.old` 四种里，含 `.disabled`
//! （中间可能夹 `.old`）的才算禁用；`.old` 单独出现只是"旧版本残留"，原样保留、语义不变。
//! 后缀**原样进包**——这是官方 Modrinth 导出的表达，也是唯一跨启动器保真的那条
//! （`env.client=optional` 不是禁用，见 `crate::addon::pack::manifest::PackFile::disabled`）。
//!
//! # 安全
//!
//! 每条相对路径都要过 [`normalize_file`]（七条规则）才进表；不过的记
//! [`SkipReason::UnsafePath`]。符号链接**不跟随**，记 [`SkipReason::SymbolicLink`]
//! （导出包里出现符号链接条目，解压端会把它当内容写出来）。

use std::path::{Path, PathBuf};

use super::report::FileReason;
use crate::addon::pack::path::{normalize_dir, normalize_file};
use crate::download::types::HashAlgo;
use crate::download::verify::digest_file;

/// 包内唯一的覆盖目录名（Modrinth 官方与 Prism 都用它，兼容面最大）。
pub const OVERRIDES_DIR: &str = "overrides";

/// 默认扫的 8 个内容目录。
pub const DEFAULT_CONTENT_ROOTS: [&str; 8] = [
    "mods",
    "config",
    "defaultconfigs",
    "scripts",
    "resourcepacks",
    "shaderpacks",
    "kubejs",
    "openloader",
];

/// 一个待判定的本地文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFile {
    /// 实例内相对路径（`/` 分隔，含 `.disabled` / `.old` 后缀，与磁盘上一致）。
    pub path: String,
    /// 磁盘上的绝对路径（导出时按这个读字节）。
    pub disk: PathBuf,
    /// 字节数。
    pub bytes: u64,
    /// 内容 SHA-1（小写十六进制）。
    pub sha1: String,
    /// 是否禁用态（路径后缀含 `.disabled`）。
    pub disabled: bool,
}

/// 一个没能进包的文件，与它为什么没进。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedLocal {
    /// 实例内相对路径（过不了规整时给原始串）。
    pub path: String,
    /// 原因。
    pub reason: FileReason,
}

/// 扫描一个实例目录，返回（可导出的文件表，跳过表），两张表都按路径排序。
///
/// 内容目录不存在不算错（空实例导出空包是合法的，只是报告里一个文件都没有）。
#[must_use]
pub fn scan_instance(root: &Path, content_roots: &[String]) -> (Vec<LocalFile>, Vec<SkippedLocal>) {
    let mut files: Vec<LocalFile> = Vec::new();
    let mut skipped: Vec<SkippedLocal> = Vec::new();

    for content_root in content_roots {
        let Ok(dir_name) = normalize_dir(content_root) else {
            skipped.push(SkippedLocal {
                path: content_root.clone(),
                reason: FileReason::UnsafePath,
            });
            continue;
        };
        let start = root.join(&dir_name);
        if !start.is_dir() {
            continue;
        }
        // 迭代而不是递归：深度由磁盘决定，别让深层目录把栈吃掉。
        let mut pending: Vec<PathBuf> = vec![start];
        while let Some(dir) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                skipped.push(SkippedLocal {
                    path: relative_name(root, &dir),
                    reason: FileReason::Unreadable,
                });
                continue;
            };
            let mut children: Vec<_> = entries.collect::<Result<Vec<_>, _>>().unwrap_or_default();
            children.sort_by_key(std::fs::DirEntry::file_name);
            for child in children {
                let path = child.path();
                let name = relative_name(root, &path);
                let Ok(file_type) = child.file_type() else {
                    skipped.push(SkippedLocal {
                        path: name,
                        reason: FileReason::Unreadable,
                    });
                    continue;
                };
                if file_type.is_symlink() {
                    skipped.push(SkippedLocal {
                        path: name,
                        reason: FileReason::SymbolicLink,
                    });
                    continue;
                }
                if file_type.is_dir() {
                    pending.push(path);
                    continue;
                }
                if !file_type.is_file() {
                    // 设备文件 / 管道 / 套接字：不是整合包内容。
                    skipped.push(SkippedLocal {
                        path: name,
                        reason: FileReason::Unreadable,
                    });
                    continue;
                }
                let Ok(normalized) = normalize_file(&name) else {
                    skipped.push(SkippedLocal {
                        path: name,
                        reason: FileReason::UnsafePath,
                    });
                    continue;
                };
                let bytes = child.metadata().map(|meta| meta.len()).unwrap_or(0);
                let Ok(sha1) = digest_file(&path, HashAlgo::Sha1) else {
                    skipped.push(SkippedLocal {
                        path: normalized,
                        reason: FileReason::Unreadable,
                    });
                    continue;
                };
                files.push(LocalFile {
                    disabled: is_disabled_path(&normalized),
                    path: normalized,
                    disk: path,
                    bytes,
                    sha1,
                });
            }
        }
    }

    files.sort_by(|left, right| left.path.cmp(&right.path));
    skipped.sort_by(|left, right| left.path.cmp(&right.path));
    (files, skipped)
}

/// 实例内相对路径（`/` 分隔）；拿不到就退回文件名。
fn relative_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 路径后缀是否带 `.disabled`（允许 `x.jar.disabled.old` 这种"禁用 + 旧残留"组合）。
///
/// M224 收敛：判据只有 [`crate::addon::mods::suffix`] 一处（启停也要用它，见
/// `DevDocs/PlanDoc/08-mod-and-recommend.md` §3）。这里保留函数名与签名，行为逐字不变。
#[must_use]
pub fn is_disabled_path(path: &str) -> bool {
    crate::addon::mods::is_disabled_path(path)
}
