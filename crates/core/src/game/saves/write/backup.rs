//! 世界目录 → zip 备份（流式、限额预检、原子落点）。
//!
//! # 落点与命名
//!
//! `<dest_dir>/<世界名>-YYYYMMDD-HHMMSS.zip`（UTC 时间戳）。落点名撞上已有文件就顺延
//! `-2` … `-99`，撞满报 [`SavesWriteKind::Conflict`]——**绝不覆盖**上一次备份。
//!
//! `dest_dir` 由调用方给（presenter host 的默认值是启动器自己的 `exports/saves/`，见
//! `core::launch::crash_bundle::default_target_dir` 的同一先例）。本模块不猜「用户想要哪个目录」，
//! 也不在 `saves/` 里偷偷建备份目录：`saves/` 下多一个 `backups/` 会在列表里长成一个世界。
//!
//! # 为什么先预检再写第一个字节
//!
//! 一条写死的边界（与 `addon::pack::format::write` 的 `preflight` 同一条纪律）：**超限要在
//! 内容进内存之前就拒**。这里的预检只走 `metadata().len()`：顶层递归一遍算条目数与未压缩总量，
//! 超限时连临时文件都不建。之后写循环里还有第二道（**已写出**的字节数），因为预检与写入之间
//! 文件可能变大（实例目录不是快照）。
//!
//! 上限本身是 [`MAX_BACKUP_TOTAL_BYTES`]（3 GiB）与 [`MAX_BACKUP_ENTRIES`]（20 万条）：
//! 前者压在 zip 的 32 位尺寸阈值（4 GiB）之下，所以**永远不会写 ZIP64**——一个能被自己与
//! 常见第三方工具读回来的包，比一个「理论上更大」的包有用。
//!
//! # 为什么写临时文件再改名
//!
//! 半截的备份文件比没有备份更坏：用户会以为它是一份完整的备份。所以写到 `<名字>.part`，
//! 收尾成功才 `rename` 到最终名字；任何一步失败都把临时文件删掉（同 `addon::pack` 的
//! `temporary_path` 口径）。
//!
//! # 符号链接
//!
//! **不跟随**：世界内部遇到链接只**跳过并计数**（[`Backup::skipped`]），不打包链接指向的内容。
//! 跳过不是静默的——`skipped` 会进 UI 的那句话（「跳过了 2 个符号链接」），与 M235「不静默丢」同一条纪律。

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::boundary::guard;
use super::{SavesWriteError, SavesWriteKind};

/// 备份文件的扩展名。
pub const BACKUP_EXT: &str = "zip";

/// 未压缩总量上限（3 GiB：压在 zip 的 32 位尺寸阈值之下，见模块文档）。
pub const MAX_BACKUP_TOTAL_BYTES: u64 = 3 * 1024 * 1024 * 1024;

/// 条目数上限（20 万条；一个巨型世界的 `region/` 通常是几千条）。
pub const MAX_BACKUP_ENTRIES: usize = 200_000;

/// 搬内容时的缓冲（流式，不把文件整份读进内存）。
const COPY_CHUNK: usize = 1024 * 1024;

/// 撞名时最多顺延多少个后缀（`-2` … `-99`）。
const MAX_SUFFIX: u32 = 99;

/// 一次成功的备份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    /// 落点（最终名字，不是临时文件）。
    pub path: PathBuf,
    /// 写出来的 zip 字节数。
    pub bytes: u64,
    /// 源目录的**未压缩**字节数（预检算出来的）。
    pub sourced: u64,
    /// 进了包的条目数（含目录条目）。
    pub entries: usize,
    /// 被跳过的符号链接数（不跟随；这个数要能报到界面上）。
    pub skipped: usize,
}

/// 写操作的限额（生产口径见 [`Limits::production`]；测试可以用极小的值钉住边界）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Limits {
    /// 未压缩总量上限。
    pub(super) total: u64,
    /// 条目数上限。
    pub(super) entries: usize,
}

impl Limits {
    pub(super) const fn production() -> Self {
        Self {
            total: MAX_BACKUP_TOTAL_BYTES,
            entries: MAX_BACKUP_ENTRIES,
        }
    }

    #[cfg(test)]
    pub(super) const fn new(total: u64, entries: usize) -> Self {
        Self { total, entries }
    }
}

/// 备份一个世界目录到 `dest_dir`，返回落点与统计。
///
/// 第一句永远是 [`guard`]：源必须是 `<run_dir>/saves` 下的真目录。
///
/// # Errors
///
/// | 情况 | 类别 |
/// |---|---|
/// | 源越界 / 是链接 / 不在 / 不是目录 | `Boundary` / `SymbolicLink` / `NotFound` / `NotADirectory` |
/// | 超过 [`MAX_BACKUP_TOTAL_BYTES`] 或 [`MAX_BACKUP_ENTRIES`] | [`SavesWriteKind::TooLarge`] |
/// | 撞名顺延到 `-99` 还被占 | [`SavesWriteKind::Conflict`] |
/// | 建目录 / 读文件 / 写 zip / 改名失败 | [`SavesWriteKind::Io`]（`path` 指出卡在哪一条） |
pub fn backup_world(
    run_dir: &Path,
    name: &str,
    dest_dir: &Path,
) -> Result<Backup, SavesWriteError> {
    backup_world_at(
        run_dir,
        name,
        dest_dir,
        SystemTime::now(),
        Limits::production(),
    )
}

/// 带显式时钟与限额的那一份（测试用它钉「时戳命名」与「限额拦在内容进内存之前」）。
pub(super) fn backup_world_at(
    run_dir: &Path,
    name: &str,
    dest_dir: &Path,
    at: SystemTime,
    limits: Limits,
) -> Result<Backup, SavesWriteError> {
    let target = guard(run_dir, name)?;
    // 落点目录必须是**绝对且非空**的：相对路径会落到进程当前目录（测试里真的发生过——
    // 一次 `dest = ""` 把 zip 写进了 crate 根），这属于「写到用户没想到的地方」，直接拒。
    if dest_dir.as_os_str().is_empty() || !dest_dir.is_absolute() {
        return Err(SavesWriteError::new(
            dest_dir,
            SavesWriteKind::Boundary,
            format!(
                "落点目录需要绝对路径，收到 `{}`（相对路径会落到进程当前目录）",
                dest_dir.display()
            ),
        ));
    }
    // 预检：只走 metadata，不读内容；超限时下面的临时文件连建都不建。
    let plan = plan(&target.world, limits)?;
    std::fs::create_dir_all(dest_dir).map_err(|error| SavesWriteError::io(dest_dir, &error))?;
    let path = unique_target(dest_dir, name, at)?;
    let temp = temporary_path(&path);
    // 上一轮被打断留下的临时文件：先清掉（同名字段是 `.part`，扫不到就继续）。
    let _ = std::fs::remove_file(&temp);

    match write_zip(&plan, &temp, limits) {
        Ok(()) => {}
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            return Err(error);
        }
    }
    if let Err(error) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(SavesWriteError::io(&path, &error));
    }
    let bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    Ok(Backup {
        path,
        bytes,
        sourced: plan.total,
        entries: plan.dirs.len() + plan.files.len(),
        skipped: plan.skipped,
    })
}

/// 预检结果：要打进包的那一份清单（`saves/` 之外一个字节都没碰）。
struct Plan {
    /// 目录条目（zip 内名字，带结尾 `/`）。
    dirs: Vec<String>,
    /// 文件条目（zip 内名字 + 磁盘路径 + 字节数）。
    files: Vec<(String, PathBuf, u64)>,
    /// 未压缩总量。
    total: u64,
    /// 跳过的符号链接数。
    skipped: usize,
}

/// 走一遍世界目录，算条目数与总量（**只读**，不读内容）。
fn plan(world: &Path, limits: Limits) -> Result<Plan, SavesWriteError> {
    let mut out = Plan {
        dirs: Vec::new(),
        files: Vec::new(),
        total: 0,
        skipped: 0,
    };
    // (磁盘目录, zip 内前缀)：显式栈，避免深目录上的递归栈。
    let mut stack: Vec<(PathBuf, String)> = vec![(world.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let read = std::fs::read_dir(&dir).map_err(|error| SavesWriteError::io(&dir, &error))?;
        // 同一个目录里的顺序不影响包内容，但排序能让同名世界两次备份的条目序一致（可比对）。
        let mut children: Vec<_> = read
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| SavesWriteError::io(&dir, &error))?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let raw = child.file_name().to_string_lossy().to_string();
            let path = child.path();
            let name = if prefix.is_empty() {
                raw.clone()
            } else {
                format!("{prefix}/{raw}")
            };
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                // 预检与写入之间被删掉：当成"不在这一包里"，不让整份备份失败。
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(SavesWriteError::io(&path, &error)),
            };
            if meta.file_type().is_symlink() {
                out.skipped += 1;
                continue;
            }
            if meta.is_dir() {
                out.dirs.push(format!("{name}/"));
                guard_entries(&path, out.dirs.len() + out.files.len(), limits)?;
                stack.push((path, name));
                continue;
            }
            let length = meta.len();
            out.total = out.total.saturating_add(length);
            out.files.push((name, path, length));
            guard_total(world, out.total, limits)?;
            guard_entries(world, out.dirs.len() + out.files.len(), limits)?;
        }
    }
    Ok(out)
}

fn guard_total(path: &Path, total: u64, limits: Limits) -> Result<(), SavesWriteError> {
    if total >= limits.total {
        return Err(SavesWriteError::new(
            path,
            SavesWriteKind::TooLarge,
            format!(
                "世界未压缩总量超过上限 {} 字节（写出的包不写 ZIP64，所以上限压在 4 GiB 之下）",
                limits.total
            ),
        ));
    }
    Ok(())
}

fn guard_entries(path: &Path, count: usize, limits: Limits) -> Result<(), SavesWriteError> {
    if count > limits.entries {
        return Err(SavesWriteError::new(
            path,
            SavesWriteKind::TooLarge,
            format!("条目数 {count} 超过上限 {}", limits.entries),
        ));
    }
    Ok(())
}

/// 真正的 zip 写入（`temp` 由调用方负责在失败时删掉）。
fn write_zip(plan: &Plan, temp: &Path, limits: Limits) -> Result<(), SavesWriteError> {
    let file = File::create(temp).map_err(|error| SavesWriteError::io(temp, &error))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for dir in &plan.dirs {
        writer
            .add_directory(dir.as_str(), options)
            .map_err(|error| SavesWriteError::new(temp, SavesWriteKind::Io, error.to_string()))?;
    }
    // 第二道闸：**已写出**的未压缩字节数从 0 起算，与预检结果无关（不重复累计）。
    let mut written = Written::default();
    let mut buffer = vec![0_u8; COPY_CHUNK];
    for (name, source, _) in &plan.files {
        let mut handle = File::open(source).map_err(|error| SavesWriteError::io(source, &error))?;
        writer
            .start_file(name.as_str(), options)
            .map_err(|error| SavesWriteError::new(source, SavesWriteKind::Io, error.to_string()))?;
        loop {
            let read = handle
                .read(&mut buffer)
                .map_err(|error| SavesWriteError::io(source, &error))?;
            if read == 0 {
                break;
            }
            writer.write_all(&buffer[..read]).map_err(|error| {
                SavesWriteError::new(temp, SavesWriteKind::Io, error.to_string())
            })?;
            written.spend(temp, read as u64, limits)?;
        }
    }
    writer
        .finish()
        .map_err(|error| SavesWriteError::new(temp, SavesWriteKind::Io, error.to_string()))?;
    Ok(())
}

/// 写循环的第二道闸（**已写出**的未压缩字节数）。
///
/// 起点是 0，**与预检的结果无关**：拿预检的返回值当基数会把全量声明尺寸再累加一遍
/// （`addon::pack::format::write::Written` 上记着同一条 P2-1 教训）。
#[derive(Default)]
struct Written {
    total: u64,
}

impl Written {
    /// 记一笔实际写出的字节，每一块之后过闸（大文件能在中途被拦下）。
    fn spend(&mut self, path: &Path, bytes: u64, limits: Limits) -> Result<(), SavesWriteError> {
        self.total = self.total.saturating_add(bytes);
        guard_total(path, self.total, limits)
    }
}

/// 落点名：`<世界名>-<时戳>.zip`，撞名顺延（见模块文档）。
fn unique_target(dest_dir: &Path, world: &str, at: SystemTime) -> Result<PathBuf, SavesWriteError> {
    let stem = format!("{world}-{}", stamp(at));
    for suffix in 1..=MAX_SUFFIX {
        let name = if suffix == 1 {
            format!("{stem}.{BACKUP_EXT}")
        } else {
            format!("{stem}-{suffix}.{BACKUP_EXT}")
        };
        let candidate = dest_dir.join(name);
        match std::fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidate),
            Ok(_) => {}
            Err(error) => return Err(SavesWriteError::io(&candidate, &error)),
        }
    }
    Err(SavesWriteError::new(
        dest_dir,
        SavesWriteKind::Conflict,
        format!("`{stem}` 的同名备份已经有 {MAX_SUFFIX} 份，换个落点目录再试"),
    ))
}

/// 与目标同目录的临时文件名（同卷 `rename` 才能原子替换）。
fn temporary_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().map_or_else(
        || std::ffi::OsString::from("backup"),
        std::ffi::OsStr::to_os_string,
    );
    name.push(".part");
    target.with_file_name(name)
}

/// `SystemTime` → `YYYYMMDD-HHMMSS`（**UTC**，与 `core::launch::time` 同一个时间口径）。
///
/// 从 RFC3339 的固定位置切片，不解析、不重排（与页面 `stamp_utc` 同一条做法）。
#[must_use]
pub fn stamp(at: SystemTime) -> String {
    let text = crate::launch::time::format_rfc3339_millis(at);
    let part = |range: std::ops::Range<usize>| text.get(range).unwrap_or_default();
    format!(
        "{}{}{}-{}{}{}",
        part(0..4),
        part(5..7),
        part(8..10),
        part(11..13),
        part(14..16),
        part(17..19)
    )
}
