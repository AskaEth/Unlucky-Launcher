//! 存档的**三个写操作**（M247 · SAVESOPS）：重命名世界、删除世界、备份导出（zip）。
//!
//! # 这一版做到哪
//!
//! 用户原话是「存档按照主流的方式来」——HMCL / PCL2 的存档区都有**重命名 / 删除（二次确认）/
//! 备份导出**。M235 只做了只读列表，本模块补写的那一半。
//!
//! | 操作 | 入口 | 落盘形态 |
//! |---|---|---|
//! | 重命名世界 | [`rename_world`] | `saves/<old>` → `saves/<new>`（一次 `rename`，不搬字节） |
//! | 删除世界 | [`delete_world`] | 递归删 `saves/<name>`（**不跟随符号链接**） |
//! | 备份导出 | [`backup_world`] | `<dest>/<世界名>-YYYYMMDD-HHMMSS.zip`（流式 zip） |
//!
//! # 边界校验是**每次自己重做**的，不靠调用方传对
//!
//! 三个入口的第一件事都是 [`boundary::guard`]：它重新解析 `<run_dir>`、重新确认
//! `<run_dir>/saves` 是**直属子目录且不是链接**、重新确认目标那条世界目录是**真的目录而不是链接**。
//! 调用方（presenter host）拿到的是「实例 id + 世界名」这种可被 UI 传坏的东西，所以
//! **r1 复审那种「信任上游」在这里一律不成立**：
//!
//! ```text
//! 名字   → 必须是单段裸名字（`crate::addon::pack::path::normalize_file` 后原样相等）
//! saves/ → 必须是 <run_dir> 的真实直属子目录（解析后比父级，不拿字符串前缀当判据）
//! 世界   → 必须是 saves/ 下的真目录（symlink_metadata：符号链接、非目录一律拒）
//! ```
//!
//! 三条各自对应 [`SavesWriteKind`] 里的一个具名类别，测试逐条钉住（见 `write/tests.rs`）。
//!
//! # 为什么写操作不走 `saves::scan` 的结果
//!
//! 扫描结果是**上一拍**的事实。用户在文件管理器里换了名字、别的程序删了一个目录，扫描里都还是
//! 旧值。所以写路径上不「先查快照再下手」，而是把边界与目标重新查一遍——快照只用来画界面。
//!
//! # 不跟随符号链接（`AGENTS.md` 的写盘纪律）
//!
//! | 位置 | 判据 | 为什么 |
//! |---|---|---|
//! | `saves/` 本身是链接 | 拒绝（[`SavesWriteKind::SymbolicLink`]） | 跟下去就写到实例目录之外了 |
//! | 世界目录是链接 | 拒绝 | 同上；删除更危险（会把链接指向的整棵树删掉） |
//! | 世界**内部**的链接 | 删除：只删链接本身；备份：跳过并计数 | 两种都不跟随，但都不静默丢（`skipped` 会计数） |
//!
//! # 失败形状
//!
//! [`SavesWriteError`] = **具名类别 + 出事的那个路径 + OS 原文**。不拼人读句子（M161 纪律：
//! 文案由 app 层按 i18n key 现求）。UI 上每一种都有话可说——见 [`SavesWriteKind`] 每一个变体的文档。
//!
//! # 模块
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`boundary`] | 名字规则 + `<run_dir>/saves/<name>` 的三道校验 + 重命名 |
//! | [`remove`] | 递归删除（不跟随链接，层级有上限） |
//! | [`backup`] | 流式 zip（限额预检 → 写临时文件 → 原子改名） |

use std::fmt;
use std::path::PathBuf;

mod backup;
mod boundary;
mod remove;

#[cfg(test)]
mod tests;

pub use backup::{backup_world, Backup, BACKUP_EXT, MAX_BACKUP_ENTRIES, MAX_BACKUP_TOTAL_BYTES};
pub use boundary::rename_world;
pub use remove::delete_world;

/// 写操作失败的类别（**结构**；文案由 app 层按 i18n key 现求）。
///
/// 每个变体在 UI 上都有一句对应的话，没有一种是「静默失败」：
///
/// | 类别 | 用户看到什么 |
/// |---|---|
/// | [`Boundary`](Self::Boundary) | 这个路径落在 `saves/` 之外（或层级深得不像一个世界） |
/// | [`UnsafeName`](Self::UnsafeName) | 名字里有不能用的字符（`/` `\` `:` `..` …） |
/// | [`SymbolicLink`](Self::SymbolicLink) | 这个位置是符号链接，不跟随 |
/// | [`NotFound`](Self::NotFound) | 那个世界（或 `saves/` 目录）已经不在了 |
/// | [`NotADirectory`](Self::NotADirectory) | 那个名字底下不是目录 |
/// | [`Conflict`](Self::Conflict) | 同名世界已存在 / 备份文件名撞满 |
/// | [`TooLarge`](Self::TooLarge) | 备份超过体积或条目上限（附上限数值） |
/// | [`Io`](Self::Io) | 其它系统错误（权限 / 被占用），OS 原文进括号 |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SavesWriteKind {
    /// 目标越出 `<run_dir>/saves`（名字可穿越、或解析后落在外面、或层级异常深）。
    Boundary,
    /// 名字过不了路径规则（空串、`.`、`..`、含 `/` `\` `:` NUL，或规整后与原串不等）。
    UnsafeName,
    /// 目标（`saves/` 那一层或世界那一条）是符号链接：不跟随。
    SymbolicLink,
    /// 目标不在了（`run_dir` / `saves/` / 世界目录三者之一）。
    NotFound,
    /// 目标存在但不是目录。
    NotADirectory,
    /// 目标已存在（重命名撞名、备份文件名撞满）。
    Conflict,
    /// 备份超过体积 / 条目上限。
    TooLarge,
    /// 其它 IO 失败（权限、被别的进程占用……OS 原文进 `detail`）。
    Io,
}

/// 一次写操作失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavesWriteError {
    /// 出事的那个路径（能确定时给最内层的那一条）。
    pub path: PathBuf,
    /// 失败类别。
    pub kind: SavesWriteKind,
    /// 人读原因（OS 原文或判据本身），**只进括号**，不进主句。
    pub detail: String,
}

impl SavesWriteError {
    pub(super) fn new(
        path: &std::path::Path,
        kind: SavesWriteKind,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            path: path.to_path_buf(),
            kind,
            detail: detail.into(),
        }
    }

    /// 一个 IO 错误：类别 [`SavesWriteKind::Io`]，`detail` 是 OS 原文。
    pub(super) fn io(path: &std::path::Path, error: &std::io::Error) -> Self {
        Self::new(path, SavesWriteKind::Io, error.to_string())
    }
}

impl fmt::Display for SavesWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}（{}）：{}",
            self.path.display(),
            kind_label(self.kind),
            self.detail
        )
    }
}

impl std::error::Error for SavesWriteError {}

/// 类别的中文短名（**日志与测试诊断用**，不是界面文案：界面按 i18n key 现求）。
fn kind_label(kind: SavesWriteKind) -> &'static str {
    match kind {
        SavesWriteKind::Boundary => "路径越界",
        SavesWriteKind::UnsafeName => "名字不可用",
        SavesWriteKind::SymbolicLink => "是符号链接",
        SavesWriteKind::NotFound => "已不在",
        SavesWriteKind::NotADirectory => "不是目录",
        SavesWriteKind::Conflict => "同名冲突",
        SavesWriteKind::TooLarge => "超出上限",
        SavesWriteKind::Io => "系统错误",
    }
}
