//! 实例**存档 / 截图的快照**与写操作的 IPC 面（M235 · STOREMVP，M247 · SAVESOPS 加写路径）。
//!
//! # 三层里的第二层
//!
//! 这一版是「只读列表 + 打开文件夹」**加上**三个写操作：`core::game::saves`（读盘/写盘，唯一的
//! 真相）→ 本模块（IPC 形状）→ `crates/app-iced/src/pages/instances/saves.rs`（画）。
//!
//! | 方向 | 类型 |
//! |---|---|
//! | app → host | [`SavesCommand`]（列目录 / 打开文件夹 / 重命名 / 删除 / 备份） |
//! | host → app | [`SavesListView`]（整份快照 + 上一次写操作的[回执](SavesOped)）或 [`SavesFailureView`] |
//!
//! # 写操作为什么走同一条回程
//!
//! 三个写操作的结果都是**一份新快照**（`Ok`）+ 一个[回执](SavesOped)：写完之后磁盘变了，
//! 界面必须跟着变；而回执是「这次动的是哪一条、动成了什么」。失败则走同一个事件的 `Err` 一支，
//! 带上一个**具名类别**（重命名撞名 / 删除越界 / 备份超限……），见 [`SavesFailureKind`]。
//! 两条都不静默：成功有回执、失败有类别 + OS 原文。
//!
//! # 为什么另开一族，不并进 `ModListView`
//!
//! 两者形状相近（都是"列一个实例的某个目录"），但语义不同：mod 有启停（看游戏在不在跑），
//! 存档这边有三个写操作（重命名 / 删除 / 备份）。合并会让两边的 `pending` 字段互相变成
//! 永远为假的分支（M224 拒绝给内容页加第五个 `ContentKind` 是同一条理由：别让穷尽 match 为
//! 不相干的字段服务）。
//!
//! # 文案纪律（M161）
//!
//! 本模块**不带人读散文**：失败给的是结构（[`SavesFailureKind`] + 底层 `detail` 串）、
//! 回执给的是事实（谁改成什么、备份落到哪个路径、几个字节、跳过几个链接），
//! 时间戳给的是**毫秒数**（UTC 毫秒，UI 自己格式化）。文案一律由 app 层按 i18n key 现求。
//!
//! # 时间戳为什么是 `Option<u64>` 的毫秒
//!
//! 取不到（早于 1970 / 文件系统不给）就是 `None`——**不拿 0 或当前时间填**：那会让"时间未知"
//! 看起来像"1970 年"或"刚刚"。格式化交给 UI（`dto::logs` 的时戳同理）。

use serde::{Deserialize, Serialize};

use crate::dto::InstanceId;

mod preview;

pub use preview::{ScreenshotPreview, ScreenshotThumbFailure, ScreenshotThumbKind, ThumbnailView};

/// app → host 的存档指令。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SavesCommand {
    /// 列某个实例的 `saves/` 与 `screenshots/`（进详情页时发、点「重试」时再发）。
    List(InstanceId),
    /// 用系统文件管理器打开存档目录。
    ///
    /// 打开**哪个**目录由 host 决定（`saves/` → `screenshots/` → 运行目录，依次退化），
    /// 因为那是"磁盘上到底有什么"的事实，不该让 UI 猜。
    OpenFolder {
        /// 实例 id
        id: InstanceId,
    },
    /// 重命名一个世界（M247）：`saves/<dir>` → `saves/<to>`。
    ///
    /// 名字规则、撞名、边界校验全在 core（host 只转发）：UI 传坏了也不会落到 `saves/` 之外。
    RenameWorld {
        /// 实例 id
        id: InstanceId,
        /// 现在的磁盘目录名
        dir: String,
        /// 新名字（单段；不许带 `/` `\` `:` 与 `..`）
        to: String,
    },
    /// 删除一个世界（M247）：递归删 `saves/<dir>`。
    ///
    /// **UI 必须先二次确认**：这是本页唯一不可逆的动作。host 侧再走一遍边界校验
    /// （越界 / 符号链接一律拒），不靠确认框把调用方「教好」。
    DeleteWorld {
        /// 实例 id
        id: InstanceId,
        /// 磁盘目录名
        dir: String,
    },
    /// 备份一个世界（M247）：把一个世界导出成 zip。
    ///
    /// `dest` 是**父目录**（不是文件）：`None` = 启动器自己的 `<data>/exports/saves/`。
    /// 文件名由 core 按「世界名 + UTC 时戳」生成，撞名顺延，**绝不覆盖**上一份。
    BackupWorld {
        /// 实例 id
        id: InstanceId,
        /// 磁盘目录名
        dir: String,
        /// 导出到哪个父目录（`None` = 启动器自己的 exports）
        dest: Option<String>,
    },
}

/// 一次写操作成功后的事实（**不含人读文案**：文案由 app 按 i18n key 现求）。
///
/// 为什么成功也要有回执：重命名 / 删除的结果在列表上自己会说话（行改名、行消失），
/// 而**备份成功没有话可说**——zip 落到了文件系统某处，用户必须知道完整路径 + 字节数，
/// 否则「点了备份然后什么也没发生」就是一次静默失败。
///
/// 回执挂在快照上（[`SavesListView::last_op`]）：下一次普通 `List` 会让它消失，
/// 所以它表达的是「最近这一次操作」，不是一段历史。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SavesOped {
    /// 改名成功。
    Renamed {
        /// 原磁盘名
        from: String,
        /// 新磁盘名
        to: String,
    },
    /// 删除成功。
    Deleted {
        /// 被删掉的磁盘名
        dir: String,
    },
    /// 备份成功。
    BackedUp {
        /// 被备份的世界（磁盘名）
        dir: String,
        /// 落点文件名
        file: String,
        /// 落点完整路径
        path: String,
        /// zip 的字节数
        bytes: u64,
        /// 被跳过的符号链接数（不跟随；0 = 一个都没跳）
        skipped: usize,
    },
}

/// 一个世界目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldView {
    /// 磁盘上的目录名（这一版它就是展示名：`LevelName` 在 `level.dat` 里，本版不解析 NBT）。
    pub dir: String,
    /// 有没有 `icon.png`（只判存在，不解码）。
    ///
    /// M251 之后**世界图标仍未解**：缩略图墙（[`ScreenshotPreview`]）只覆盖 `screenshots/`，
    /// 这是 D54 的范围。这行字眼原来写的是「缩略图墙不在 U3 范围内」，那句话已失真。
    pub icon: bool,
    /// 目录 mtime（UTC 毫秒；`None` = 取不到）。
    pub modified_ms: Option<u64>,
}

/// 一张截图（元数据 + **预览**）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenshotView {
    /// 磁盘文件名（含扩展名）。
    pub file: String,
    /// 字节数（UI 自己格式化，不在这里拼 `1.2 MiB`）。
    pub bytes: u64,
    /// 文件 mtime（UTC 毫秒；`None` = 取不到）。
    pub modified_ms: Option<u64>,
    /// 预览（缩略图像素 / 没生成 / 具名失败原因），见 [`ScreenshotPreview`]。
    ///
    /// 像素**不落盘**（D54 第 2 条）：解码在 host 的后台线程里做完，随这份快照一次性交给
    /// 壳层。`(file, modified_ms, bytes)` 同时也是 host 侧缓存键的三段——三者任一变了就是
    /// 另一张图，必须重解。
    pub preview: ScreenshotPreview,
}

/// 被跳过的条目来自哪个目录。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SavesArea {
    /// `<run_dir>/saves/`
    Saves,
    /// `<run_dir>/screenshots/`
    Screenshots,
}

/// 没收进列表的原因（与 `core::game::saves::SkipKind` 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SavesSkipKind {
    /// 符号链接（不跟随）。
    SymbolicLink,
    /// `saves/` 下的非目录。
    NotADirectory,
    /// `screenshots/` 下的非普通文件。
    NotAFile,
    /// 名字过不了路径规则。
    UnsafeName,
    /// 元数据读不出来。
    Unreadable,
    /// 不是认得的图片扩展名。
    NotAnImage,
}

/// 一个没能进列表的条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavesSkipView {
    /// 来自哪个目录。
    pub area: SavesArea,
    /// 磁盘名字。
    pub file: String,
    /// 为什么没收。
    pub reason: SavesSkipKind,
}

/// 一次列举的完整快照（**整体替换**，不做增量）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavesListView {
    /// 实例 id
    pub instance: InstanceId,
    /// 被列的 `saves/` 目录（绝对路径）。
    pub dir: String,
    /// `saves/` 存不存在（`false` = 这实例还没存过档，不是错误）。
    pub exists: bool,
    /// 世界列表（按名字排序）。
    pub worlds: Vec<WorldView>,
    /// 被列的 `screenshots/` 目录（绝对路径）。
    pub screenshots_dir: String,
    /// `screenshots/` 存不存在。
    pub screenshots_exists: bool,
    /// 截图列表（**新的在前**）。
    pub screenshots: Vec<ScreenshotView>,
    /// 跳过的条目（UI 只需计数 + 一句"为什么"）。
    pub skipped: Vec<SavesSkipView>,
    /// 最近一次写操作的回执（`None` = 这一拍只是列举，没写东西）。
    pub last_op: Option<SavesOped>,
}

impl SavesListView {
    /// 世界数。
    #[must_use]
    pub fn world_count(&self) -> usize {
        self.worlds.len()
    }

    /// 截图数。
    #[must_use]
    pub fn screenshot_count(&self) -> usize {
        self.screenshots.len()
    }

    /// 跳过数。
    #[must_use]
    pub fn skipped_count(&self) -> usize {
        self.skipped.len()
    }

    /// 两个目录都不存在（新实例的常态：还没进过游戏）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.worlds.is_empty() && self.screenshots.is_empty()
    }
}

/// 一次失败（app 按 key 现求文案）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavesFailureView {
    /// 实例 id（迟到/串台的结果靠它判）。
    pub instance: InstanceId,
    /// 涉及的那个世界（写操作才有；读路径给空串）。
    ///
    /// 为什么要这一格：失败那句话是「重命名 **A** 失败：同名世界已存在」——没有名字，
    /// 用户不知道该改哪一行（与 `ModFailureView::file` 同一条理由）。
    pub file: String,
    /// 这是哪一类**写**操作（`None` = 读路径 / 打开文件夹的失败）。
    pub op: Option<SavesFailureOp>,
    /// 失败大类。
    pub kind: SavesFailureKind,
    /// 底层原文（OS 错误 / 路径细节）；只进括号，不进主句。
    pub detail: String,
}

/// 失败出自哪一类写操作（决定失败那句主句用哪个动词）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SavesFailureOp {
    /// 重命名。
    Rename,
    /// 删除。
    Delete,
    /// 备份。
    Backup,
}

/// 失败大类（**每一种在 UI 上都有一句话**，没有静默失败）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SavesFailureKind {
    /// 实例不在了（快照里找不到），或隔离档位派生不出运行目录。
    InstanceUnavailable,
    /// `saves/` / `screenshots/` 存在但不可用（符号链接 / 不是目录 / 读不了）。
    DirUnusable,
    /// 「打开文件夹」失败（目录不存在 / 拉不起文件管理器）。
    OpenFailed,
    /// 写操作：名字过不了规则（空、`.`、`..`、含 `/` `\` `:` NUL）。
    WriteUnsafeName,
    /// 写操作：目标或路径上的一段是符号链接（不跟随）。
    WriteSymlink,
    /// 写操作：那个世界（或 `saves/`）已经不在了。
    WriteMissing,
    /// 写操作：那个名字底下不是目录。
    WriteNotADirectory,
    /// 写操作：同名世界已存在 / 备份文件名撞满。
    WriteConflict,
    /// 写操作：路径落在 `saves/` 之外（含层级异常深）。
    WriteBoundary,
    /// 写操作：备份超过体积或条目上限。
    WriteTooLarge,
    /// 写操作：实例正在启动/运行中，**一个字节都没碰盘**（见 `host::saves::dispatch`）。
    ///
    /// 单独一类而不是折进 [`WriteFailed`](Self::WriteFailed)：Linux 上删一个正在跑的世界**会成功**，
    /// 所以这不是"写盘失败了"，是"我们主动没写"。界面必须说「游戏正在运行，先退出再操作」。
    GameRunning,
    /// 写操作：其它系统错误（权限 / 被别的进程占用），OS 原文进 `detail`。
    WriteFailed,
}

#[cfg(test)]
mod tests;
