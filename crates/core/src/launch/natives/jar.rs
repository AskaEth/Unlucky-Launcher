//! natives jar 的读取细节：zip 遍历、条目筛选、目录清理、戳记读写。
//!
//! 与 [`super`] 的分工：`super` 管「计划与报告」，这里管「怎么把一个 jar 里的原生库落到盘上」。

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use super::{NativeJar, NativesError, NativesReport, STAMP_FILE};

/// 戳记 schema 版本（改字段语义才升）。
const STAMP_VERSION: u8 = 1;

/// 内置排除前缀：Mojang 清单惯例，manifest 没写 `extract` 规则时也这么办。
const DEFAULT_EXCLUDES: &[&str] = &["META-INF/"];

/// 原生库扩展名白名单（Windows / Linux / macOS；`jnilib` 是 JDK6 时代的老 mac 后缀）。
const NATIVE_EXTENSIONS: &[&str] = &["dll", "so", "dylib", "jnilib"];

/// 签名 / 校验附属文件扩展名：即使在 natives jar 根目录也绝不落地。
const SIGNATURE_EXTENSIONS: &[&str] = &["sf", "rsa", "dsa", "ec", "asc", "sha1", "sha256", "git"];

/// 戳记的一条：源 jar 的「路径 + 字节数 + mtime」指纹。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct JarStamp {
    /// 源 jar 绝对路径（同一目录换了别的版本时必须能区分开）。
    pub path: String,
    /// 字节数。
    pub len: u64,
    /// 修改时间（Unix 毫秒；读不到时 0，等价于「每次都当它变了」）。
    pub mtime_ms: u64,
}

/// 目录戳记文件内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Stamp {
    v: u8,
    files: usize,
    jars: Vec<JarStamp>,
}

impl Stamp {
    /// 戳记是否仍然对应当前的 jar 集（版本 + 指纹逐一相等）。
    pub(crate) fn matches(&self, stamps: &[JarStamp]) -> bool {
        self.v == STAMP_VERSION && self.jars == stamps
    }

    /// 上次解压的文件数。
    pub(crate) fn files(&self) -> usize {
        self.files
    }
}

/// 构造 `NativesError::Io`。
pub(crate) fn io_error(
    op: &'static str,
    path: impl Into<PathBuf>,
    source: io::Error,
) -> NativesError {
    NativesError::Io {
        op,
        path: path.into(),
        source,
    }
}

/// 打开一个 natives jar（打不开 / 不是合法 zip 都以 [`NativesError`] 带路径报出）。
fn open_archive(native: &NativeJar) -> Result<ZipArchive<File>, NativesError> {
    let file = File::open(&native.path)
        .map_err(|source| io_error("open_native_jar", &native.path, source))?;
    ZipArchive::new(file).map_err(|err| NativesError::Zip {
        path: native.path.clone(),
        reason: err.to_string(),
    })
}

/// 该落盘的条目（zip 索引 + 相对路径）：排除前缀 → 原生库白名单 → zip-slip 防护。
///
/// 只读遍历，不解压：`extract_jar` 拿它算落点，`count_jar` 拿它清点数量，
/// 两边共用同一套筛选口径（预览与真解压不会给出不同的数字）。
fn extractable_entries(
    archive: &mut ZipArchive<File>,
    native: &NativeJar,
    report: &mut NativesReport,
) -> Result<Vec<(usize, PathBuf)>, NativesError> {
    let excludes: Vec<&str> = DEFAULT_EXCLUDES
        .iter()
        .copied()
        .chain(native.exclude.iter().map(String::as_str))
        .collect();
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|err| NativesError::Zip {
            path: native.path.clone(),
            reason: err.to_string(),
        })?;
        if entry.is_dir() {
            continue;
        }
        // `enclosed_name` 拒绝 `..` / 绝对路径条目（zip-slip）。
        let Some(relative) = entry.enclosed_name() else {
            report.ignored += 1;
            continue;
        };
        let name = relative.to_string_lossy().replace('\\', "/");
        if !should_extract(&name, &excludes) {
            report.ignored += 1;
            continue;
        }
        entries.push((index, relative));
    }
    Ok(entries)
}

/// 解压单个 jar：`extractable_entries` 选条目，再逐个落到 `dir` 下。
pub(crate) fn extract_jar(
    native: &NativeJar,
    dir: &Path,
    report: &mut NativesReport,
) -> Result<(), NativesError> {
    let mut archive = open_archive(native)?;
    for (index, relative) in extractable_entries(&mut archive, native, report)? {
        let mut entry = archive.by_index(index).map_err(|err| NativesError::Zip {
            path: native.path.clone(),
            reason: err.to_string(),
        })?;
        let dest = dir.join(&relative);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| io_error("create_natives_subdir", parent, source))?;
        }
        let mut out =
            File::create(&dest).map_err(|source| io_error("create_native_file", &dest, source))?;
        io::copy(&mut entry, &mut out)
            .map_err(|source| io_error("write_native_file", &dest, source))?;
        report.files += 1;
    }
    Ok(())
}

/// 只读清点单个 jar：只算「真解压会写下几个原生库」，一个字节都不落盘。
///
/// 与 [`extract_jar`] 的差别只有「写」这一步；jar 打不开照样报错，所以预览不会把
/// 真启动会遇到的坏包藏起来。
pub(crate) fn count_jar(
    native: &NativeJar,
    report: &mut NativesReport,
) -> Result<(), NativesError> {
    let mut archive = open_archive(native)?;
    let entries = extractable_entries(&mut archive, native, report)?;
    report.files += entries.len();
    Ok(())
}

/// 该条目要不要解压：排除前缀命中、签名文件、非原生库扩展名 → 跳过。
pub(crate) fn should_extract(name: &str, excludes: &[&str]) -> bool {
    if excludes.iter().any(|prefix| name.starts_with(prefix)) {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    let extension = lower.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    if SIGNATURE_EXTENSIONS.contains(&extension) {
        return false;
    }
    NATIVE_EXTENSIONS.contains(&extension)
}

/// 清掉目录里已有的原生库与戳记（尽力而为：删不掉的留着，后面覆盖写会报错）。
pub(crate) fn clear_natives(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if name == STAMP_FILE {
            let _ = fs::remove_file(&path);
            continue;
        }
        let is_native = name
            .rsplit_once('.')
            .map(|(_, ext)| NATIVE_EXTENSIONS.contains(&ext))
            .unwrap_or(false);
        if is_native {
            let _ = fs::remove_file(&path);
        }
    }
}

/// 源 jar 指纹（路径 + 字节数 + mtime）。
pub(crate) fn jar_stamps(jars: &[NativeJar]) -> Vec<JarStamp> {
    jars.iter()
        .map(|native| {
            let metadata = fs::metadata(&native.path);
            let len = metadata.as_ref().map(|meta| meta.len()).unwrap_or(0);
            let mtime_ms = metadata
                .ok()
                .and_then(|meta| meta.modified().ok())
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_millis() as u64)
                .unwrap_or(0);
            JarStamp {
                path: native.path.display().to_string(),
                len,
                mtime_ms,
            }
        })
        .collect()
}

/// 读戳记（不存在 / 损坏 / 版本不符都当作「没有戳记」）。
pub(crate) fn read_stamp(dir: &Path) -> Option<Stamp> {
    let text = fs::read_to_string(dir.join(STAMP_FILE)).ok()?;
    let stamp: Stamp = serde_json::from_str(&text).ok()?;
    if stamp.v == STAMP_VERSION {
        Some(stamp)
    } else {
        None
    }
}

/// 写戳记（直接覆盖写：半截文件下次解析失败即视为「没有戳记」，重解一遍就行）。
pub(crate) fn write_stamp(dir: &Path, files: usize, jars: &[JarStamp]) -> Result<(), NativesError> {
    let path = dir.join(STAMP_FILE);
    let stamp = Stamp {
        v: STAMP_VERSION,
        files,
        jars: jars.to_vec(),
    };
    let text = serde_json::to_string_pretty(&stamp).map_err(|err| NativesError::Zip {
        path: path.clone(),
        reason: err.to_string(),
    })?;
    fs::write(&path, text).map_err(|source| io_error("write_natives_stamp", &path, source))
}

#[cfg(test)]
mod tests {
    use super::{should_extract, DEFAULT_EXCLUDES};

    #[test]
    fn native_library_extensions_pass() {
        for name in [
            "lwjgl.dll",
            "OpenAL.dll",
            "liblwjgl.so",
            "liblwjgl.dylib",
            "liblwjgl.jnilib",
        ] {
            assert!(should_extract(name, DEFAULT_EXCLUDES), "{name} 应解压");
        }
    }

    #[test]
    fn metadata_and_signature_entries_are_skipped() {
        for name in [
            "META-INF/MANIFEST.MF",
            "META-INF/FOO.SF",
            "META-INF/FOO.RSA",
            "lwjgl.dll.sha1",
            "lwjgl.dll.sha256",
            "README.txt",
            "LICENSE",
            // 无扩展名的文件一律不进 java.library.path。
            "lwjgl",
        ] {
            assert!(!should_extract(name, DEFAULT_EXCLUDES), "{name} 不该解压");
        }
    }

    #[test]
    fn manifest_exclude_prefixes_apply_on_top_of_builtin() {
        assert!(!should_extract("extra/x.dll", &["extra/"]));
        assert!(should_extract("x.dll", &["extra/"]));
        // 大小写不敏感只对扩展名生效（排除前缀按清单原样比较，与 HMCL 一致）。
        assert!(should_extract("X.DLL", DEFAULT_EXCLUDES));
    }
}
