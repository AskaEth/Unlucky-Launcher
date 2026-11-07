//! 原子写：临时文件 → fsync → rename。
//!
//! 出处：`DevDocs/analyse/07-settings-packaging-update.md` §1.6（HMCL `FileUtils.saveSafely`：
//! 写 tmp + `Files.move(REPLACE_EXISTING)`）。配置与下载落盘都该走这里，禁止直接 `fs::write`。
//!
//! `std::fs::rename` 在 Unix 与 Windows 上都是「替换式」语义
//! （Windows 侧 std 用 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`），因此不需要先删目标文件。

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::error::{io_at, PlatformError, PlatformResult};

/// 同进程内的 tmp 名去重计数器。
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 原子地把 `bytes` 写到 `path`（父目录不存在会创建）。
///
/// # Errors
/// 建目录、写临时文件、fsync、rename 任一步失败都返回 [`PlatformError::Io`]，
/// 且失败时会尽力清理临时文件，不留垃圾。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> PlatformResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent).map_err(io_at(parent))?;
    }

    let tmp = tmp_path(path);
    let write_result = (|| -> io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(source) = write_result {
        std::fs::remove_file(&tmp).ok();
        return Err(PlatformError::Io { path: tmp, source });
    }

    if let Err(source) = std::fs::rename(&tmp, path) {
        std::fs::remove_file(&tmp).ok();
        return Err(PlatformError::Io {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

/// 读文件；不存在返回 `None`，其它错误照原样上报（用于区分「没配过」和「读不了」）。
///
/// # Errors
/// 除 `NotFound` 以外的读失败返回 [`PlatformError::Io`]。
pub fn read_optional(path: &Path) -> PlatformResult<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(PlatformError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// 同目录下的临时文件名（`launcher.json` → `launcher.json.tmp<pid>-<n>`）。
#[must_use]
pub fn tmp_path(path: &Path) -> PathBuf {
    let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().map_or_else(
        || String::from("ull"),
        |name| name.to_string_lossy().into_owned(),
    );
    let tmp_name = format!(".{name}.tmp{}-{counter}", std::process::id());
    match path.parent() {
        Some(parent) => parent.join(tmp_name),
        None => PathBuf::from(tmp_name),
    }
}
