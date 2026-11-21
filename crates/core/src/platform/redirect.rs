//! `redirect.json` 指针文件——迁移后留在旧目录，防止便携盘被搬走后旧目录重新长出一个新实例。
//!
//! 出处：`DevDocs/PlanDoc/04-data-and-config.md` §1 细节（「迁移 = 移动 + 原地留 redirect.json 指针」）。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::atomic;
use super::data_dir::DataMode;
use super::error::{io_at, PlatformError, PlatformResult};
use super::layout::REDIRECT_FILE;

/// 本实现能读懂的指针文件版本；更高版本视为不认识（避免瞎读新格式）。
pub const REDIRECT_SCHEMA_VERSION: u32 = 1;

/// 指针内容：旧目录 → 新目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedirectPointer {
    /// 格式版本。
    pub schema_version: u32,
    /// 迁移时采用的模式。
    pub mode: DataMode,
    /// 新的启动器数据目录。
    pub launcher_home: PathBuf,
    /// 新的游戏目录。
    pub game_home: PathBuf,
    /// 迁移时间（毫秒 since UNIX_EPOCH）。
    pub migrated_at_ms: u64,
}

/// 跟随指针的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Followed {
    /// 最终命中的指针。
    pub pointer: RedirectPointer,
    /// 实际跟随跳数（1 = 只跟了一跳）。
    pub hops: usize,
}

/// `<dir>/redirect.json`。
#[must_use]
pub fn redirect_path(dir: &Path) -> PathBuf {
    dir.join(REDIRECT_FILE)
}

/// 读指针；没有指针文件返回 `None`。
///
/// # Errors
/// 读失败或 JSON 损坏、版本不认识时返回 [`PlatformError`]。
pub fn read(dir: &Path) -> PlatformResult<Option<RedirectPointer>> {
    let path = redirect_path(dir);
    let Some(bytes) = atomic::read_optional(&path)? else {
        return Ok(None);
    };
    let pointer: RedirectPointer =
        serde_json::from_slice(&bytes).map_err(|err| PlatformError::RedirectCorrupt {
            path: path.clone(),
            reason: err.to_string(),
        })?;
    if pointer.schema_version > REDIRECT_SCHEMA_VERSION {
        return Err(PlatformError::RedirectCorrupt {
            path,
            reason: format!(
                "指针版本 {} 高于本实现支持的 {REDIRECT_SCHEMA_VERSION}",
                pointer.schema_version
            ),
        });
    }
    Ok(Some(pointer))
}

/// 原子写指针（父目录自动创建）。
///
/// # Errors
/// 序列化或写盘失败时返回 [`PlatformError`]。
pub fn write(dir: &Path, pointer: &RedirectPointer) -> PlatformResult<PathBuf> {
    let path = redirect_path(dir);
    let bytes =
        serde_json::to_vec_pretty(pointer).map_err(|err| PlatformError::RedirectCorrupt {
            path: path.clone(),
            reason: format!("无法序列化指针: {err}"),
        })?;
    std::fs::create_dir_all(dir).map_err(io_at(dir))?;
    atomic::write_atomic(&path, &bytes)?;
    Ok(path)
}

/// 删除指针（`None` = 本来就没有）。
///
/// # Errors
/// 删除失败时返回 [`PlatformError::Io`]。
pub fn remove(dir: &Path) -> PlatformResult<bool> {
    let path = redirect_path(dir);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(PlatformError::Io { path, source: err }),
    }
}

/// 跟随指针链（最多 `max_hops` 跳，成环或超限报错）。
///
/// # Errors
/// 指针损坏或跳转成环/超限时返回 [`PlatformError`]。
pub fn follow(dir: &Path, max_hops: usize) -> PlatformResult<Option<Followed>> {
    let mut current = dir.to_path_buf();
    let mut visited: BTreeSet<PathBuf> = BTreeSet::new();
    visited.insert(current.clone());
    let mut hops = 0usize;

    loop {
        let Some(pointer) = read(&current)? else {
            return Ok(None);
        };
        hops += 1;
        if hops > max_hops {
            return Err(PlatformError::RedirectCycle {
                dir: dir.to_path_buf(),
                hops,
            });
        }
        let next = pointer.launcher_home.clone();
        if !visited.insert(next.clone()) {
            return Err(PlatformError::RedirectCycle {
                dir: dir.to_path_buf(),
                hops,
            });
        }
        // 还有下一跳就继续跟，否则这就是终点。
        if !redirect_path(&next).exists() {
            return Ok(Some(Followed { pointer, hops }));
        }
        current = next;
    }
}
