//! 列一个运行目录下的 `saves/`（世界）与 `screenshots/`（图片），**纯只读**。
//!
//! 只碰三种系统调用：`symlink_metadata`（判目录形态）、`read_dir`（取子项）、`metadata`
//! （取大小/时间）。没有 `create_dir`、没有 `write`、没有 `rename`、没有 `remove`——`saves/`
//! 不存在时也**不替玩家建**（建出来的空目录会让「这实例还没玩过」这件事再也看不出来）。
//!
//! 回看「调用前后版本树不变」的判据见 `tests::scan_does_not_touch_the_disk`：
//! 它在调用前后各拍一次 `(相对路径, 是否目录, 字节数, mtime)` 的指纹并逐条比对。

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::error::SavesError;

/// `saves` 目录名（游戏与社区一致的口径）。
pub const SAVES_DIR: &str = "saves";

/// `screenshots` 目录名（原版游戏把 F2 截图放这里）。
pub const SCREENSHOTS_DIR: &str = "screenshots";

/// 世界图标文件名（**只判存在**，不解码：U3 不做缩略图墙）。
pub const WORLD_ICON: &str = "icon.png";

/// 认作截图的扩展名（大小写不敏感）。
///
/// 原版游戏只写 `.png`；其余几个是玩家手动塞进去/其它 mod 的常见形态。**不收** `.gif` /
/// 视频——它们是"别的东西"，与"我们忘了列"要区分开（见 [`SkipKind::NotAnImage`]）。
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

/// 某个运行目录下的 `saves/`（纯路径拼接，不建、不检查）。
#[must_use]
pub fn saves_dir(run_dir: &Path) -> PathBuf {
    run_dir.join(SAVES_DIR)
}

/// 某个运行目录下的 `screenshots/`（纯路径拼接，不建、不检查）。
#[must_use]
pub fn screenshots_dir(run_dir: &Path) -> PathBuf {
    run_dir.join(SCREENSHOTS_DIR)
}

/// `saves/` 下的一个世界目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldEntry {
    /// 磁盘上的目录名（原样）。**这一版它就是展示名**：`LevelName` 在 `level.dat` 里，
    /// 解析 NBT 不在 U3 范围内（做缩略图墙时一起做）。
    pub dir: String,
    /// 有没有 `icon.png`（只判存在，不读内容）。
    pub icon: bool,
    /// 目录 mtime（毫秒 since UNIX_EPOCH；取不到为 `None`）。
    pub modified_ms: Option<u64>,
}

/// `screenshots/` 下的一张图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotEntry {
    /// 磁盘文件名（原样，含扩展名）。
    pub file: String,
    /// 字节数（UI 自己格式化，不在这里拼 `1.2 MiB`）。
    pub bytes: u64,
    /// 文件 mtime（毫秒 since UNIX_EPOCH；取不到为 `None`）。
    pub modified_ms: Option<u64>,
}

/// 被跳过的条目来自哪个目录。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    /// `<run_dir>/saves/`
    Saves,
    /// `<run_dir>/screenshots/`
    Screenshots,
}

/// 没收进列表的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipKind {
    /// 符号链接（不跟随：跟随会把实例外的东西算成存档）。
    SymbolicLink,
    /// `saves/` 下的非目录（散落文件），或 `screenshots/` 下的目录。
    NotADirectoryExpected,
    /// `screenshots/` 下的非普通文件（设备 / 管道 / 目录）。
    NotAFile,
    /// 名字过不了路径规则（含 `\`、`:`、`..` 之类；判据复用整合包侧那一份）。
    UnsafeName,
    /// 元数据读不出来。
    Unreadable,
    /// `screenshots/` 下的普通文件，但扩展名不是 [`IMAGE_EXTENSIONS`]。
    NotAnImage,
}

/// 一个没能进列表的条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavesSkip {
    /// 来自哪个目录。
    pub area: Area,
    /// 磁盘名字（原样；读不出来时给能拿到的那个串）。
    pub file: String,
    /// 为什么没收。
    pub reason: SkipKind,
}

/// 一次列举的结果（两个目录一次读回，UI 只收这一份快照）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavesScan {
    /// 被列的 `saves/` 目录。
    pub dir: PathBuf,
    /// `saves/` 存不存在（`false` = 这实例还没存过档，不是错误）。
    pub exists: bool,
    /// 世界列表（按名字排序，大小写不敏感）。
    pub worlds: Vec<WorldEntry>,
    /// 被列的 `screenshots/` 目录。
    pub screenshots_dir: PathBuf,
    /// `screenshots/` 存不存在。
    pub screenshots_exists: bool,
    /// 截图列表（**新的在前**：文件名就是时间戳，倒序即时间倒序）。
    pub screenshots: Vec<ScreenshotEntry>,
    /// 跳过的条目与原因（两个目录并在一份里，靠 [`SavesSkip::area`] 区分）。
    pub skipped: Vec<SavesSkip>,
}

impl SavesScan {
    /// `saves/` 与 `screenshots/` 一个都不存在（新实例的常态）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.worlds.is_empty() && self.screenshots.is_empty()
    }
}

/// 列一个运行目录下的 `saves/` 与 `screenshots/`。
///
/// 目录不存在 → `exists = false` 的空结果（新实例不是错误），**且不建那个目录**。
///
/// # Errors
///
/// 目录存在但不可用（符号链接 / 不是目录 / 读不了）时给 [`SavesError`]。
pub fn scan(run_dir: &Path) -> Result<SavesScan, SavesError> {
    let dir = saves_dir(run_dir);
    let (exists, children) = read_children(&dir)?;
    let mut worlds = Vec::new();
    let mut skipped = Vec::new();
    for child in children {
        let raw = child.file_name().to_string_lossy().to_string();
        let Ok(file_type) = child.file_type() else {
            skipped.push(skip(Area::Saves, raw, SkipKind::Unreadable));
            continue;
        };
        if file_type.is_symlink() {
            skipped.push(skip(Area::Saves, raw, SkipKind::SymbolicLink));
            continue;
        }
        if !file_type.is_dir() {
            skipped.push(skip(Area::Saves, raw, SkipKind::NotADirectoryExpected));
            continue;
        }
        if !plain_name(&raw) {
            skipped.push(skip(Area::Saves, raw, SkipKind::UnsafeName));
            continue;
        }
        let icon = is_regular_file(&dir.join(&raw).join(WORLD_ICON));
        let modified_ms = child.metadata().ok().and_then(|meta| modified_ms(&meta));
        worlds.push(WorldEntry {
            dir: raw,
            icon,
            modified_ms,
        });
    }
    // 名字排序（大小写不敏感；同名时按磁盘名字典序兜底，保证两帧之间不抖）。
    worlds.sort_by(|left, right| {
        left.dir
            .to_lowercase()
            .cmp(&right.dir.to_lowercase())
            .then_with(|| left.dir.cmp(&right.dir))
    });

    let shots_dir = screenshots_dir(run_dir);
    let (shots_exists, shots) = read_children(&shots_dir)?;
    let mut screenshots = Vec::new();
    for child in shots {
        let raw = child.file_name().to_string_lossy().to_string();
        let Ok(file_type) = child.file_type() else {
            skipped.push(skip(Area::Screenshots, raw, SkipKind::Unreadable));
            continue;
        };
        if file_type.is_symlink() {
            skipped.push(skip(Area::Screenshots, raw, SkipKind::SymbolicLink));
            continue;
        }
        if !file_type.is_file() {
            skipped.push(skip(Area::Screenshots, raw, SkipKind::NotAFile));
            continue;
        }
        if !plain_name(&raw) {
            skipped.push(skip(Area::Screenshots, raw, SkipKind::UnsafeName));
            continue;
        }
        if !is_image(&raw) {
            skipped.push(skip(Area::Screenshots, raw, SkipKind::NotAnImage));
            continue;
        }
        let meta = child.metadata().ok();
        screenshots.push(ScreenshotEntry {
            bytes: meta.as_ref().map_or(0, std::fs::Metadata::len),
            modified_ms: meta.as_ref().and_then(modified_ms),
            file: raw,
        });
    }
    // 新的在前：截图文件名就是 `2024-01-02_12.00.00.png`，字典序倒排就是时间倒排。
    screenshots.sort_by(|left, right| right.file.cmp(&left.file));
    skipped.sort_by(|left, right| {
        area_key(left.area)
            .cmp(&area_key(right.area))
            .then_with(|| left.file.cmp(&right.file))
    });

    Ok(SavesScan {
        dir,
        exists,
        worlds,
        screenshots_dir: shots_dir,
        screenshots_exists: shots_exists,
        screenshots,
        skipped,
    })
}

/// 列一个目录的子项（**不存在 ⇒ `(false, 空)`**；不可用 ⇒ 具名错误；符号链接不跟随）。
fn read_children(dir: &Path) -> Result<(bool, Vec<std::fs::DirEntry>), SavesError> {
    let meta = match std::fs::symlink_metadata(dir) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((false, Vec::new()));
        }
        Err(error) => return Err(SavesError::unusable(dir, error.to_string())),
    };
    if meta.file_type().is_symlink() {
        return Err(SavesError::unusable(
            dir,
            "是符号链接（不跟随：会把实例目录之外的内容算成存档）",
        ));
    }
    if !meta.is_dir() {
        return Err(SavesError::unusable(dir, "不是目录"));
    }
    let read =
        std::fs::read_dir(dir).map_err(|error| SavesError::unusable(dir, error.to_string()))?;
    let mut children: Vec<_> = read
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| SavesError::unusable(dir, error.to_string()))?;
    children.sort_by_key(std::fs::DirEntry::file_name);
    Ok((true, children))
}

fn skip(area: Area, file: String, reason: SkipKind) -> SavesSkip {
    SavesSkip { area, file, reason }
}

/// 名字是不是**裸名字**（判据复用整合包侧那一份，不在这里写第二份规则）。
///
/// `read_dir` 给的已经是单段名字，所以只需判它过不过得了路径规则：`\`、`:`、`..` 这类会被
/// 拒——它们既可能在展示里被误读成路径分隔，也可能在将来接"删除/重命名"时落到别处。
fn plain_name(raw: &str) -> bool {
    crate::addon::pack::path::normalize_file(raw).is_ok()
}

/// 扩展名是不是认得的图片（大小写不敏感）。
fn is_image(raw: &str) -> bool {
    Path::new(raw)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let ext = ext.to_ascii_lowercase();
            IMAGE_EXTENSIONS.iter().any(|known| *known == ext)
        })
        .unwrap_or(false)
}

/// 是不是**普通文件**（不跟随符号链接：`icon.png` 指到别处不算这个世界的图标）。
fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_file())
        .unwrap_or(false)
}

/// mtime → 毫秒（早于 1970 或取不到 ⇒ `None`，不报错）。
fn modified_ms(meta: &std::fs::Metadata) -> Option<u64> {
    let since = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(since.as_millis()).ok()
}

/// 跳过条目的排序键（先 `saves/` 再 `screenshots/`）。
fn area_key(area: Area) -> u8 {
    match area {
        Area::Saves => 0,
        Area::Screenshots => 1,
    }
}
