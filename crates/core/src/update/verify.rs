//! 下载产物的校验：长度（截断）+ 摘要（内容），不符即拒绝安装。
//!
//! 承 `download` 的规矩「先校验后落盘」：包体先落在暂存目录的中间文件里，长度与摘要都对上
//! 才算 [`StagedPackage`]；对不上就**立刻删掉**，绝不让它有机会顶到目标路径上。
//!
//! 两道判据的顺序是有意的：先比长度（能抓住截断，且不需要读全文件），再比摘要（能抓住
//! 「长度对但内容错」）。顺序反过来会让「被截断的包」白白多读一遍盘。
//!
//! 长度判据用「**恰好等于**」而不是「不小于」：清单里的 `size` 是我们自己发的字节数，
//! 多了同样是错的（比如拼接了两个包），多一个字节也不许装。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::download::verify::{digest_file, matches};
use crate::download::Integrity;

use super::error::{UpdateError, UpdateErrorKind};
use super::manifest::Artifact;
use super::source::UpdateSource;

/// 暂存文件的进程内序号（同一进程多次更新不撞名）。
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 校验通过的暂存包。
///
/// `Drop` 时删掉暂存文件（装好之后 `rename` 已经把它搬走，删除是无操作）；
/// 因此「校验失败要清理」这件事不需要调用方记得做。
#[derive(Debug)]
pub struct StagedPackage {
    path: PathBuf,
    size: u64,
    integrity: Integrity,
}

impl StagedPackage {
    /// 暂存文件路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 实际字节数（已与清单核对过，等于清单声明的 `size`）。
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// 校验用的摘要。
    #[must_use]
    pub fn integrity(&self) -> &Integrity {
        &self.integrity
    }
}

impl Drop for StagedPackage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// 从源取包体 → 落暂存 → 长度与摘要双检。
///
/// # Errors
///
/// 取包失败透传 [`UpdateError`]；长度不符是 [`UpdateErrorKind::SizeMismatch`]，
/// 摘要不符是 [`UpdateErrorKind::DigestMismatch`]。失败时暂存文件已被删掉。
pub fn stage(
    source: &dyn UpdateSource,
    artifact: &Artifact,
    staging_dir: &Path,
) -> Result<StagedPackage, UpdateError> {
    fs::create_dir_all(staging_dir).map_err(|error| UpdateError::io_at(staging_dir, &error))?;
    let path = staging_dir.join(format!(
        "{}-{}.ullupd",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let outcome = source
        .fetch_artifact(artifact, &path)
        .and_then(|_| verify_file(&path, &artifact.integrity, artifact.size));
    match outcome {
        Ok(size) => Ok(StagedPackage {
            path,
            size,
            integrity: artifact.integrity.clone(),
        }),
        Err(error) => {
            let _ = fs::remove_file(&path);
            Err(error)
        }
    }
}

/// 校验一个已经落盘的文件：长度必须**恰好**等于 `expected_size`，摘要必须相等。
///
/// # Errors
///
/// 读不到文件/读失败是 [`UpdateErrorKind::Io`]；长度不符是 `SizeMismatch`；
/// 摘要不符是 `DigestMismatch`。
pub fn verify_file(
    path: &Path,
    integrity: &Integrity,
    expected_size: u64,
) -> Result<u64, UpdateError> {
    let metadata = fs::metadata(path).map_err(|error| UpdateError::io_at(path, &error))?;
    let actual = metadata.len();
    if actual != expected_size {
        return Err(UpdateError::new(
            UpdateErrorKind::SizeMismatch,
            format!(
                "包体 {actual} 字节，清单声明 {expected_size} 字节（差 {} 字节；短了通常是下载被截断）",
                expected_size.abs_diff(actual)
            ),
        )
        .with_path(path));
    }
    let digest =
        digest_file(path, integrity.algo).map_err(|error| UpdateError::io_at(path, &error))?;
    if !matches(&integrity.hex, &digest) {
        return Err(UpdateError::new(
            UpdateErrorKind::DigestMismatch,
            format!(
                "{} 摘要不符：清单 {} / 实际 {}（拒绝安装）",
                integrity.algo.as_str(),
                integrity.short_prefix(),
                &digest[..digest.len().min(8)]
            ),
        )
        .with_path(path));
    }
    Ok(actual)
}
