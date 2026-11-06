//! `addon::pack` 的单元测试：夹具读回、未知字段往返、探测优先级、扩展点、恶意样本。
//!
//! **全部离线**：没有网络调用，也没有真实整合包样本（见 `fixtures` 的来源声明）。

mod detect;
mod export;
mod export_harness;
mod export_pipeline;
mod export_roundtrip;
mod extend;
mod fixtures;
mod model;
mod roundtrip;
mod safety;
mod unknown_fields;

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 本次进程内唯一的临时根目录（沿用仓库既有约定：不引 `tempfile`）。
pub(crate) fn temp_root(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("ull-pack-{}-{tag}-{unique}", std::process::id()))
}

/// 写一个夹具文件（自动建父目录）。
pub(crate) fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建夹具目录");
    }
    std::fs::write(path, bytes).expect("写夹具文件");
}

/// 把一个目录打成 zip（`Stored` 压缩，方便测试直接改字节）。
pub(crate) fn zip_dir(source: &Path, target: &Path) {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("建 zip 目录");
    }
    let file = File::create(target).expect("建 zip");
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let dir = source.join(&relative);
        let mut children: Vec<_> = std::fs::read_dir(&dir)
            .expect("读夹具目录")
            .collect::<Result<Vec<_>, _>>()
            .expect("目录项");
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let relative = relative.join(child.file_name());
            if child.file_type().expect("夹具类型").is_dir() {
                pending.push(relative);
                continue;
            }
            let bytes = std::fs::read(child.path()).expect("读夹具文件");
            let entry = relative.to_string_lossy().replace('\\', "/");
            writer.start_file(entry, options).expect("写 zip 条目");
            writer.write_all(&bytes).expect("写 zip 内容");
        }
    }
    writer.finish().expect("收尾 zip");
}

/// 用一张显式条目表打 zip（恶意样本用）。
pub(crate) fn write_zip(target: &Path, entries: &[(&str, &[u8])]) {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("建 zip 目录");
    }
    let file = File::create(target).expect("建 zip");
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in entries {
        writer.start_file(*name, options).expect("写 zip 条目");
        writer.write_all(bytes).expect("写 zip 内容");
    }
    writer.finish().expect("收尾 zip");
}

/// 打一个带**符号链接条目**的 zip（`S_IFLNK` + 目标当内容，与真实恶意包同构）。
pub(crate) fn write_zip_with_symlink(
    target: &Path,
    entries: &[(&str, &[u8])],
    link_name: &str,
    link_target: &str,
) {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("建 zip 目录");
    }
    let file = File::create(target).expect("建 zip");
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in entries {
        writer.start_file(*name, options).expect("写 zip 条目");
        writer.write_all(bytes).expect("写 zip 内容");
    }
    writer
        .add_symlink(link_name, link_target, options)
        .expect("写符号链接条目");
    writer.finish().expect("收尾 zip");
}
