//! 归档解压：zip（Windows）/ tar.gz（Linux、macOS 的 Temurin 只发这种）+ 幂等戳记。
//!
//! # 为什么不用「解压到最终目录」了事
//!
//! Temurin 的归档里所有条目都在一个顶层目录下（`jdk-21.0.13+9/bin/java`），
//! 而托管布局要的是 `java-managed/jdk-21-temurin/bin/java`。裸解压会多出一层，
//! 于是这里先在**暂存目录**里解完，再把那个唯一的顶层目录（或暂存目录本身）
//! 提升成最终目录——这样终态恒定，`bin/java` 的位置不会因为厂商换打包方式而漂移。
//!
//! # 幂等
//!
//! 戳记 `.ull-jdk.json` 记下「这份目录是用哪个 sha256 的归档解出来的 + 文件数」。
//! 重复 `ull java install 21` 时 sha256 相同就**一个字节都不动**（返回 `skipped`）；
//! 官方发了新版（sha256 变）才重解。戳记缺失/损坏一律当「没装过」。
//!
//! # 安全
//!
//! zip 走 `enclosed_name()`（拒绝 `..` 与绝对路径），tar 走 `unpack_in()`
//! （同样拒绝越界），两者都不会把文件写到目标目录之外（zip-slip）。
//! tar 里的符号链接条目跳过（Windows 建不了；Temurin 的 JDK 归档不依赖它们）。

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use super::error::{io_error, JavaError, JavaErrorKind};

/// 戳记文件名（放在托管 JDK 根目录里）。
pub const STAMP_FILE: &str = ".ull-jdk.json";

/// 戳记 schema 版本（改字段语义才升）。
const STAMP_VERSION: u8 = 1;

/// 暂存目录前缀（解压在它里面进行，成功后提升成最终目录）。
const STAGING_PREFIX: &str = ".staging-";

/// 托管 JDK 的戳记。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JdkStamp {
    /// schema 版本。
    pub v: u8,
    /// 归档文件名。
    pub archive: String,
    /// 归档的 SHA-256（幂等判据）。
    pub sha256: String,
    /// 大版本号。
    pub major: u32,
    /// 解出来的文件数。
    pub files: usize,
}

/// 一次解压的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractReport {
    /// 本次落盘的文件数（`skipped` 时为 0）。
    pub files: usize,
    /// 是否因为戳记命中而整体跳过。
    pub skipped: bool,
}

impl ExtractReport {
    /// 是否真的写盘了。
    #[must_use]
    pub const fn wrote(self) -> bool {
        !self.skipped
    }
}

/// 托管 JDK 的 `java` 可执行文件路径。
#[must_use]
pub fn java_binary(dir: &Path) -> PathBuf {
    dir.join("bin").join(java_binary_name())
}

/// 平台上的 java 可执行文件名。
#[must_use]
pub const fn java_binary_name() -> &'static str {
    if cfg!(windows) {
        "java.exe"
    } else {
        "java"
    }
}

/// 读戳记（不存在 / 损坏 / 版本不符都当作「没有」）。
#[must_use]
pub fn read_stamp(dir: &Path) -> Option<JdkStamp> {
    let text = fs::read_to_string(dir.join(STAMP_FILE)).ok()?;
    let stamp: JdkStamp = serde_json::from_str(&text).ok()?;
    (stamp.v == STAMP_VERSION).then_some(stamp)
}

/// 这个目录里是不是已经躺着一份用**同一个归档**解出来的 JDK。
#[must_use]
pub fn is_ready(dir: &Path, sha256: &str) -> bool {
    let Some(stamp) = read_stamp(dir) else {
        return false;
    };
    stamp.sha256 == sha256.to_ascii_lowercase() && java_binary(dir).is_file()
}

/// 解压一个 JDK 归档到 `dest`（幂等）。
///
/// # Errors
///
/// 归档打不开/格式不认识、解压写盘失败、`dest` 已存在却不由我们管理
/// （没有戳记）时返回错误。
pub fn extract_archive(
    archive: &Path,
    sha256: &str,
    major: u32,
    dest: &Path,
) -> Result<ExtractReport, JavaError> {
    if is_ready(dest, sha256) {
        return Ok(ExtractReport {
            files: 0,
            skipped: true,
        });
    }
    let parent = dest.parent().ok_or_else(|| {
        JavaError::new(
            JavaErrorKind::Io,
            format!("托管目录没有父目录：{}", dest.display()),
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| io_error("创建托管目录", parent.display(), &error))?;

    let folder = dest
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "jdk".to_string());
    let staging = parent.join(format!("{STAGING_PREFIX}{folder}"));
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|error| io_error("清理暂存目录", staging.display(), &error))?;
    }
    fs::create_dir_all(&staging)
        .map_err(|error| io_error("创建暂存目录", staging.display(), &error))?;

    let extracted = if is_zip(archive) {
        extract_zip(archive, &staging)
    } else {
        extract_tar_gz(archive, &staging)
    };
    let files = match extracted {
        Ok(files) => files,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };

    let root = single_top_level_dir(&staging).unwrap_or_else(|| staging.clone());
    if dest.exists() {
        // 只回收「我们自己装的」目录：有戳记，或空目录。其余一律不碰。
        let ours = read_stamp(dest).is_some() || is_empty_dir(dest);
        if !ours {
            let _ = fs::remove_dir_all(&staging);
            return Err(JavaError::new(
                JavaErrorKind::Io,
                format!("目标目录已存在且不是托管 JDK：{}", dest.display()),
            )
            .with_hint("换个目录（`--data-dir`），或先手动清掉它"));
        }
        fs::remove_dir_all(dest)
            .map_err(|error| io_error("清空旧托管目录", dest.display(), &error))?;
    }
    fs::rename(&root, dest).map_err(|error| {
        io_error(
            "提升暂存目录",
            format!("{} → {}", root.display(), dest.display()),
            &error,
        )
    })?;
    if root != staging {
        let _ = fs::remove_dir_all(&staging);
    }
    if !java_binary(dest).is_file() {
        return Err(JavaError::new(
            JavaErrorKind::Probe,
            format!("解压完成但找不到 bin/{}", java_binary_name()),
        )
        .with_hint("归档形态与平台不匹配（例如把 Linux 包当成 Windows 包装）"));
    }

    let stamp = JdkStamp {
        v: STAMP_VERSION,
        archive: archive
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        sha256: sha256.to_ascii_lowercase(),
        major,
        files,
    };
    let text = serde_json::to_string_pretty(&stamp).map_err(|error| {
        JavaError::new(
            JavaErrorKind::Metadata,
            format!("序列化 JDK 戳记失败：{error}"),
        )
    })?;
    let stamp_path = dest.join(STAMP_FILE);
    fs::write(&stamp_path, text)
        .map_err(|error| io_error("写 JDK 戳记", stamp_path.display(), &error))?;
    Ok(ExtractReport {
        files,
        skipped: false,
    })
}

/// 归档是不是 zip（魔数优先，读不到才退回扩展名）。
///
/// 不按扩展名一刀切：镜像把 `.zip` 存成 `.zip.tmp` 之类时魔数仍然可靠，
/// 反过来把 tar.gz 命名成 `.zip` 也不会被错解。
#[must_use]
pub fn is_zip(archive: &Path) -> bool {
    let mut head = [0u8; 4];
    let magic = File::open(archive)
        .and_then(|mut file| io::Read::read_exact(&mut file, &mut head).map(|()| head));
    if let Ok(head) = magic {
        // `PK\x03\x04` 普通条目 / `PK\x05\x06` 空归档。
        return head == *b"PK\x03\x04" || head == *b"PK\x05\x06";
    }
    archive
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|name| name.ends_with(".zip"))
}

/// zip：逐条目落盘，`enclosed_name` 挡路径穿越。
fn extract_zip(archive: &Path, dir: &Path) -> Result<usize, JavaError> {
    let file = File::open(archive)
        .map_err(|error| io_error("打开 JDK 归档", archive.display(), &error))?;
    let mut zip = ZipArchive::new(file).map_err(|error| {
        JavaError::new(
            JavaErrorKind::Io,
            format!("JDK 归档不是合法 zip（{}）：{error}", archive.display()),
        )
    })?;
    let mut files = 0;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| {
            JavaError::new(
                JavaErrorKind::Io,
                format!("读 zip 条目失败（{}）：{error}", archive.display()),
            )
        })?;
        if entry.is_dir() {
            continue;
        }
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let dest = dir.join(&relative);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error("创建解压子目录", parent.display(), &error))?;
        }
        let mut out = File::create(&dest)
            .map_err(|error| io_error("创建解压文件", dest.display(), &error))?;
        io::copy(&mut entry, &mut out)
            .map_err(|error| io_error("写解压文件", dest.display(), &error))?;
        files += 1;
    }
    Ok(files)
}

/// tar.gz：`unpack_in` 挡路径穿越，符号链接条目跳过。
fn extract_tar_gz(archive: &Path, dir: &Path) -> Result<usize, JavaError> {
    let file = File::open(archive)
        .map_err(|error| io_error("打开 JDK 归档", archive.display(), &error))?;
    let mut tar = tar::Archive::new(GzDecoder::new(file));
    let entries = tar.entries().map_err(|error| {
        JavaError::new(
            JavaErrorKind::Io,
            format!("JDK 归档不是合法 tar.gz（{}）：{error}", archive.display()),
        )
    })?;
    let mut files = 0;
    for entry in entries {
        let mut entry = entry.map_err(|error| {
            JavaError::new(
                JavaErrorKind::Io,
                format!("读 tar 条目失败（{}）：{error}", archive.display()),
            )
        })?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let unpacked = entry
            .unpack_in(dir)
            .map_err(|error| io_error("解压 tar 条目", dir.display(), &error))?;
        if unpacked {
            files += 1;
        }
    }
    Ok(files)
}

/// 暂存目录里是不是只有一个顶层目录（Temurin 的 `jdk-21.0.13+9/`）。
fn single_top_level_dir(dir: &Path) -> Option<PathBuf> {
    let mut entries = fs::read_dir(dir).ok()?;
    let first = entries.next()?.ok()?;
    if entries.next().is_some() {
        return None;
    }
    let path = first.path();
    path.is_dir().then_some(path)
}

fn is_empty_dir(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_none())
}
