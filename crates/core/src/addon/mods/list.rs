//! 列一个实例 `mods/` 目录（**只读**、只看这一个目录）。
//!
//! # 单根语义（D20）
//!
//! 入口是**已经派生好的运行目录**（`game::isolation::derive_game_dir` 的结果），本模块只往下
//! 取一层 `mods/`：不自己去猜游戏根、不做「隔离侧 + 共享侧合并」。派生是启动与列表**同一个**
//! 答案的唯一来源，第二处派生就是第二套真相。
//!
//! # 收哪些条目
//!
//! 与导出扫描同一条纪律：`mods/` 下**不按扩展名过滤**（`.jar` / `.jar.disabled` / `.zip` /
//! 认不出的后缀一律列出）。丢掉"我不认识的东西"正是用户点名不许的——启停开关对认不出的
//! 文件同样有意义（禁用它 = 让游戏别加载它）。
//!
//! 不收的东西只有三类，且都**逐条登记原因**（不是静默跳过）：
//! 符号链接（不跟随）、非普通文件（目录 / 设备）、名字过不了路径规则（[`crate::addon::pack::path`]）。

use std::path::{Path, PathBuf};

use super::error::ModError;
use super::suffix::{display_name, suffix_variant, SuffixVariant};

/// `mods` 目录名（游戏与社区一致的口径）。
pub const MODS_DIR: &str = "mods";

/// 某个运行目录下的 `mods/`（不建、不检查——纯路径拼接）。
#[must_use]
pub fn mods_dir(run_dir: &Path) -> PathBuf {
    run_dir.join(MODS_DIR)
}

/// 列表里的一条。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModEntry {
    /// 磁盘上的文件名（含 `.disabled` / `.old` 后缀，原样）。
    pub file: String,
    /// 展示名（去掉两个后缀，不做 `-版本号` 猜解）。
    pub name: String,
    /// 是否启用（`.disabled` 缺席即启用）。
    pub enabled: bool,
    /// 字节数。
    pub bytes: u64,
    /// 后缀变体（UI 用它区分"旧残留"）。
    pub variant: SuffixVariant,
}

/// 没能进列表的一条。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModSkip {
    /// 文件名（读不到时给能拿到的原样串）。
    pub file: String,
    /// 为什么没列。
    pub reason: SkipReason,
}

/// 没列出来的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// 符号链接（不跟随：跟随会把实例外的东西算成实例内容）。
    SymbolicLink,
    /// 不是普通文件（目录 / 设备 / 管道）。
    NotAFile,
    /// 名字过不了路径规则（含 `\`、`:`、`..` 之类）。
    UnsafeName,
    /// 元数据读不出来。
    Unreadable,
}

/// 一次列目录的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModScan {
    /// 被列的那个 `mods/` 目录。
    pub dir: PathBuf,
    /// 目录存不存在（`false` = 这个实例还没装过 mod，不是错误）。
    pub exists: bool,
    /// 列表（按展示名排序，大小写不敏感）。
    pub entries: Vec<ModEntry>,
    /// 跳过的条目与原因。
    pub skipped: Vec<ModSkip>,
}

impl ModScan {
    /// 启用中的条数。
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.enabled).count()
    }

    /// 禁用中的条数。
    #[must_use]
    pub fn disabled_count(&self) -> usize {
        self.entries.len() - self.enabled_count()
    }
}

/// 列一个运行目录下的 `mods/`。
///
/// 目录不存在 → `exists = false` 的空结果（空实例不是错误）。
///
/// # Errors
///
/// `mods` 存在但不可用（符号链接 / 不是目录 / 读不了）时给 [`ModError::ModsDirUnusable`]。
pub fn scan(run_dir: &Path) -> Result<ModScan, ModError> {
    let dir = mods_dir(run_dir);
    let meta = match std::fs::symlink_metadata(&dir) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ModScan {
                dir,
                exists: false,
                entries: Vec::new(),
                skipped: Vec::new(),
            });
        }
        Err(error) => {
            return Err(ModError::ModsDirUnusable {
                dir,
                detail: error.to_string(),
            });
        }
    };
    if meta.file_type().is_symlink() {
        return Err(ModError::ModsDirUnusable {
            dir,
            detail: "是符号链接（不跟随：改名会落到实例目录之外）".to_string(),
        });
    }
    if !meta.is_dir() {
        return Err(ModError::ModsDirUnusable {
            dir,
            detail: "不是目录".to_string(),
        });
    }

    let read = std::fs::read_dir(&dir).map_err(|error| ModError::ModsDirUnusable {
        dir: dir.clone(),
        detail: error.to_string(),
    })?;
    let mut children: Vec<_> =
        read.collect::<Result<Vec<_>, _>>()
            .map_err(|error| ModError::ModsDirUnusable {
                dir: dir.clone(),
                detail: error.to_string(),
            })?;
    children.sort_by_key(std::fs::DirEntry::file_name);

    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    for child in children {
        let raw = child.file_name().to_string_lossy().to_string();
        let Ok(file_type) = child.file_type() else {
            skipped.push(ModSkip {
                file: raw,
                reason: SkipReason::Unreadable,
            });
            continue;
        };
        if file_type.is_symlink() {
            skipped.push(ModSkip {
                file: raw,
                reason: SkipReason::SymbolicLink,
            });
            continue;
        }
        if !file_type.is_file() {
            skipped.push(ModSkip {
                file: raw,
                reason: SkipReason::NotAFile,
            });
            continue;
        }
        if crate::addon::pack::path::normalize_file(&raw).is_err() || raw.contains('/') {
            skipped.push(ModSkip {
                file: raw,
                reason: SkipReason::UnsafeName,
            });
            continue;
        }
        let bytes = child.metadata().map(|meta| meta.len()).unwrap_or(0);
        entries.push(ModEntry {
            name: display_name(&raw).to_string(),
            enabled: !suffix_variant(&raw).is_disabled(),
            variant: suffix_variant(&raw),
            file: raw,
            bytes,
        });
    }

    // 展示名排序（大小写不敏感；同名时按磁盘名字典序兜底，保证两帧之间不抖）。
    entries.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.file.cmp(&right.file))
    });
    skipped.sort_by(|left, right| left.file.cmp(&right.file));

    Ok(ModScan {
        dir,
        exists: true,
        entries,
        skipped,
    })
}
