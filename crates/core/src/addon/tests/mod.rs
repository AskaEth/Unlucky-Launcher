//! `addon` 的**导入管线**测试（M111）：三格式端到端 + 三方合并 + 必须先失败的恶意样本。
//!
//! # 夹具来源
//!
//! [`fixtures`] 不是本目录的文件，而是用 `#[path]` 直接吃 M110 的
//! `addon/pack/tests/fixtures.rs`——**同一份手造包只维护一处**。它的来源声明也在那边：
//! 三份 CF / mrpack / MultiMC 包全部手造（字段取自 analyse/05 的 HMCL 解析器形状），
//! 没有一个来自真实导出；`mods/half.jar` / `config/broken.toml` / `mods/broken.jar`
//! 是刻意造的坏文件（导入器不许崩，也不许悄悄丢）。
//!
//! 为了让 `mod` 路径上的 `use super::{temp_root, write_file, zip_dir}` 解析得到，本文件把
//! 这三个辅助函数放在与 `addon/pack/tests/mod.rs` 同名同形的位置。
//!
//! # 全部离线
//!
//! 网络一律走 [`support::StubFetcher`]（内存里的假文件），没有一个用例发请求。

// `fixtures.rs` 被 `addon::pack::tests` 与本目录**同时**编译（同一份手造包只维护一处），
// 所以 clippy 的 `duplicate_mod` 在这里是刻意为之：改成两份副本才是真的错。
#[allow(clippy::duplicate_mod)]
#[path = "../pack/tests/fixtures.rs"]
mod fixtures;
mod import_curseforge;
mod import_failures;
mod import_merge;
mod import_modrinth;
mod import_multimc;
mod import_report;
mod support;

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 本次进程内唯一的临时根目录（沿用仓库既有约定：不引 `tempfile`）。
pub(crate) fn temp_root(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("ull-import-{}-{tag}-{unique}", std::process::id()))
}

/// 写一个文件（自动建父目录）。
pub(crate) fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建目录");
    }
    std::fs::write(path, bytes).expect("写文件");
}

/// 把一个目录打成 zip（`Stored`，方便断言字节）。
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
            .expect("读目录")
            .collect::<Result<Vec<_>, _>>()
            .expect("目录项");
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let relative = relative.join(child.file_name());
            if child.file_type().expect("类型").is_dir() {
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

/// 打一个带**符号链接条目**的 zip（`S_IFLNK` + 目标当内容）。
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

/// 递归列出目录下的文件（`/` 分隔的相对路径，排序）。
///
/// "失败后目标目录零残留"就是靠它断言的：`Failed` 之后这里必须返回空表。
pub(crate) fn list_tree(root: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                out.push(
                    path.strip_prefix(base)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// 基线相对实例根的路径（`/` 分隔，给 `list_tree` 断言用）。
///
/// 从 `PackBaseline::relative_path()` 派生，**不在测试里手抄**目录名——手抄的那一份正是
/// 复审 r5 P1-1 的成因（读写两条路径各写各的）。
pub(crate) fn baseline_rel() -> String {
    crate::addon::import::PackBaseline::relative_path()
        .to_string_lossy()
        .replace('\\', "/")
}

/// 读一个文件成字符串（断言覆盖内容用）。
pub(crate) fn read_text(path: &Path) -> String {
    let bytes =
        std::fs::read(path).unwrap_or_else(|error| panic!("读 {} 失败：{error}", path.display()));
    String::from_utf8_lossy(&bytes).to_string()
}

/// 断言目录不存在（= 零残留）。
pub(crate) fn assert_absent(path: &Path) {
    assert!(
        !path.exists(),
        "目标目录应当不存在（零残留），实际：{:?}",
        list_tree(path)
    );
}
