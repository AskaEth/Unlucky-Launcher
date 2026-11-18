//! tar 的读写细节：列候选、按名字取一个条目写成文件。
//!
//! 从 `unpack.rs` 拆出来的唯一理由：那边贴着 400 行硬线（AGENTS §3-1）。判据（选谁、多大算合规、
//! 失败怎么归因）全在父模块与 [`write_entry`]，本文件只做「照着头里的字段搬字节」。

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

use flate2::read::GzDecoder;

use super::{Candidate, Container, MAX_LAUNCHER_BYTES};
use crate::update::error::{UpdateError, UpdateErrorKind};

/// 归档里的候选条目（普通文件 + 可执行位 / `.exe` 名字，判据在父模块）。
pub(super) fn list(path: &Path, gzip: bool) -> Result<Vec<Candidate>, UpdateError> {
    let mut found = Vec::new();
    let mut archive = tar_archive(path, gzip)?;
    for entry in archive
        .entries()
        .map_err(|error| archive_error(path, &error))?
    {
        let entry = entry.map_err(|error| archive_error(path, &error))?;
        if !entry.header().entry_type().is_file() {
            continue; // 目录 / 符号链接 / 硬链接 / 设备：不是可执行字节
        }
        let name = entry
            .path()
            .map_err(|error| archive_error(path, &error))?
            .to_string_lossy()
            .into_owned();
        let mode = entry
            .header()
            .mode()
            .map_err(|error| archive_error(path, &error))?;
        if super::is_launcher_candidate(&name, mode) {
            found.push(Candidate { name });
        }
    }
    Ok(found)
}

pub(super) fn tar_archive(
    path: &Path,
    gzip: bool,
) -> Result<tar::Archive<Box<dyn Read>>, UpdateError> {
    let file = File::open(path).map_err(|error| UpdateError::io_at(path, &error))?;
    let reader: Box<dyn Read> = if gzip {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    Ok(tar::Archive::new(reader))
}

pub(super) fn archive_error(path: &Path, error: &dyn std::fmt::Display) -> UpdateError {
    UpdateError::new(
        UpdateErrorKind::InvalidManifest,
        format!("归档 {} 读不出来：{error}", path.display()),
    )
    .with_path(path)
}

/// 把候选条目写成 `out`，返回写出的字节数（失败时 `out` 已删掉）。
pub(super) fn extract(
    archive: &Path,
    format: Container,
    name: &str,
    out: &Path,
) -> Result<u64, UpdateError> {
    extract_tar(archive, format == Container::TarGz, name, out)
}

fn extract_tar(archive: &Path, gzip: bool, name: &str, out: &Path) -> Result<u64, UpdateError> {
    let mut tar = tar_archive(archive, gzip)?;
    for entry in tar
        .entries()
        .map_err(|error| archive_error(archive, &error))?
    {
        let mut entry = entry.map_err(|error| archive_error(archive, &error))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .map_err(|error| archive_error(archive, &error))?
            .to_string_lossy()
            .into_owned();
        if path == name {
            let declared = entry.size();
            return copy_entry(&mut entry, declared, out, archive);
        }
    }
    let _ = fs::remove_file(out);
    Err(missing_on_second_pass(archive, name))
}

/// 第二遍找不到同一个条目：归档在两遍之间变了（或者解压实现与列表口径不一致）。
fn missing_on_second_pass(archive: &Path, name: &str) -> UpdateError {
    UpdateError::new(
        UpdateErrorKind::LauncherMissing,
        format!(
            "归档 {} 第二次读时找不到本体条目 `{name}`（同一份文件两遍结果不同）",
            archive.display()
        ),
    )
    .with_path(archive)
}

/// 真正落盘：写 `out`，边写边数，写完对三件事——声明大小、0 字节、上限。
fn copy_entry(
    reader: &mut dyn Read,
    declared: u64,
    out: &Path,
    archive: &Path,
) -> Result<u64, UpdateError> {
    let verdict = write_entry(reader, declared, out, archive);
    if verdict.is_err() {
        let _ = fs::remove_file(out);
    }
    verdict
}

fn write_entry(
    reader: &mut dyn Read,
    declared: u64,
    out: &Path,
    archive: &Path,
) -> Result<u64, UpdateError> {
    if declared == 0 {
        return Err(size_error(
            archive,
            "本体是 0 字节（归档头声明的就是 0：发布侧打了一个空文件）",
        ));
    }
    if declared > MAX_LAUNCHER_BYTES {
        return Err(size_error(
            archive,
            &format!("本体声明 {declared} 字节，超过上限 {MAX_LAUNCHER_BYTES} 字节（解压炸弹？）"),
        ));
    }
    let mut file = File::create(out).map_err(|error| UpdateError::io_at(out, &error))?;
    // `take` 兜住「头里写的比真有的少」：读得动第 MAX+1 个字节说明实际更大，判失败。
    let mut limited = reader.take(MAX_LAUNCHER_BYTES + 1);
    let written =
        io::copy(&mut limited, &mut file).map_err(|error| UpdateError::io_at(out, &error))?;
    drop(file);
    if written > MAX_LAUNCHER_BYTES {
        return Err(size_error(
            archive,
            &format!("本体实际超过上限 {MAX_LAUNCHER_BYTES} 字节（归档头在撒谎）"),
        ));
    }
    if written != declared {
        return Err(size_error(
            archive,
            &format!("本体写出 {written} 字节，归档头声明 {declared} 字节"),
        ));
    }
    Ok(written)
}

fn size_error(archive: &Path, detail: &str) -> UpdateError {
    UpdateError::new(
        UpdateErrorKind::LauncherSizeMismatch,
        format!(
            "归档 {} 里的启动器本体大小不合规：{detail}",
            archive.display()
        ),
    )
    .with_path(archive)
}
