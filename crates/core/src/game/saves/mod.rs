//! 存档 / 截图的列举与「打开文件夹」（M235 · STOREMVP）+ 三个写操作（M247 · SAVESOPS）。
//!
//! # 读的那一半（M235）
//!
//! 用户原话是「存档截图 / 导入」，M230 审计确认三层全缺（core 没有读路径，`saves/` 只在
//! `isolation.rs` 的切换探测里被计数；presenter 没有 DTO；i18n 没有键；没有页面）。
//! U3 把 M235 的边界钉成：**只读列表 + 打开文件夹，不做缩略图墙**。理由有两条：
//!
//! 1. 缩略图要引图片解码依赖，而 `AGENTS.md` §2 只允许 `palette-extract` 之外不引 image 类依赖；
//! 2. 截图/导入的产物形态还没定型，先不进产品。
//!
//! 所以 [`list`] **只读**：只调 `read_dir` / `metadata` / `symlink_metadata`，不建目录、不写文件、
//! 不改名字、不读文件内容（`icon.png` 只判存在——那是将来缩略图墙的输入，不是这一版的解码对象）。
//!
//! # 写的那一半（M247）
//!
//! 用户接着拍板：「存档按照主流的方式来」——主流第三方启动器（HMCL / PCL2）的存档区都有
//! **重命名世界 / 删除世界（二次确认）/ 备份导出**。这三个落在 [`write`]：
//!
//! | 操作 | 入口 | 边界 |
//! |---|---|---|
//! | 重命名 | [`write::rename_world`] | 只动 `saves/<old>` → `saves/<new>`；撞名拒 |
//! | 删除 | [`write::delete_world`] | 先过边界校验，再递归删；不跟随符号链接 |
//! | 备份 | [`write::backup_world`] | 流式 zip 到调用方给的目录，时戳命名，不覆盖 |
//!
//! **写操作一律自己重新校验 `run_dir` 与目标路径**（不靠调用方传对）——判据在
//! [`write`] 的模块文档里一条条列着。
//!
//! # 单根语义（D20）
//!
//! 入口是**已经派生好的运行目录**（[`crate::game::isolation::derive_game_dir`] 的结果），
//! 本模块只往下取两层：`<run_dir>/saves/` 与 `<run_dir>/screenshots/`。
//! 不自己去猜游戏根，也不做「隔离侧 + 共享侧合并」——派生是启动、mod 列表、存档列表**同一个**
//! 答案的唯一来源。与 [`crate::addon::mods`] 同一条纪律。
//!
//! # 目录不存在不是错误
//!
//! 新实例两个目录都不存在，这**不是**异常（`exists = false` + 空列表）。只有「存在但不可用」
//! （符号链接 / 不是目录 / 读不了）才是 [`SavesError`]；**并且照样不建目录**。
//! 写操作那边不同：目标不存在就是 [`write::SavesWriteKind::NotFound`]（写不下去，得说清楚）。
//!
//! # 收纳口径
//!
//! | 位置 | 收什么 | 不收什么（逐条登记原因，不静默丢） |
//! |---|---|---|
//! | `saves/` | 子目录 = 一个世界 | 符号链接（不跟随）、非目录（散落文件）、名字过不了路径规则、元数据读不出 |
//! | `screenshots/` | 扩展名是 [`IMAGE_EXTENSIONS`] 的普通文件 | 同上 + 不是图片的文件（`NotAnImage`） |
//!
//! 「不静默丢」是 M224 定下的口径：列表要能和磁盘对得上账，用户看到「跳过 3 项」时得知道
//! 那 3 项不是我们忘了。
//!
//! # 模块
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`error`] | 「目录存在但不可用」的失败形状（带 OS 原文） |
//! | [`list`] | 列 `<run_dir>/saves` 与 `<run_dir>/screenshots`（纯只读） |
//! | [`write`] | 重命名 / 删除 / 备份导出（三个写操作 + 边界校验） |
//!
//! 「打开文件夹」不在本模块：平台差异收敛在 [`crate::platform::opener::open_dir`]
//! （`AGENTS.md` §6），本模块一个 `cfg` 都不写。

mod error;
mod list;
mod write;

pub use error::SavesError;
pub use list::{
    saves_dir, scan, screenshots_dir, Area, SavesScan, SavesSkip, ScreenshotEntry, SkipKind,
    WorldEntry, IMAGE_EXTENSIONS, SAVES_DIR, SCREENSHOTS_DIR, WORLD_ICON,
};
pub use write::{
    backup_world, delete_world, rename_world, Backup, SavesWriteError, SavesWriteKind, BACKUP_EXT,
    MAX_BACKUP_ENTRIES, MAX_BACKUP_TOTAL_BYTES,
};

#[cfg(test)]
mod tests;
