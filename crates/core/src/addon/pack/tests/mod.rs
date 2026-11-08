//! `addon::pack` 的单元测试：夹具读回、未知字段往返、探测优先级、扩展点、恶意样本。
//!
//! **全部离线**：没有网络调用，也没有真实整合包样本（见 `fixtures` 的来源声明）。
//!
//! # 临时目录必须**测试自己收尾**（M160 r2）
//!
//! 每个用例的临时根是 [`TempRoot`]：`Drop` 里 `remove_dir_all`，所以**正常结束与 panic 展开
//! 都会删**（Rust 展开会跑析构）。这条不是洁癖：`zip64` 那族用例要造 4 GiB 的**稀疏文件**验
//! 阈值，残留会把 `%TEMP%` 堆到撑爆整台机器（实测 1651 个目录 / 110 GB，把 CI 与别的 agent
//! 的构建一起咬死）。
//!
//! 写法的来源是仓库既有的那一份（`crate::update::tests::support::TempDir`）：同样的 `new` /
//! `join` / `Drop` 三件套，只把前缀换成 `ull-pack-`，不引 `tempfile`、不新增第四种范式。
//! 不信"系统会自动清 `%TEMP%`"——那要靠时间或外部工具，测试必须自证干净。

mod detect;
mod export;
mod export_harness;
mod export_pipeline;
mod export_roundtrip;
mod extend;
mod fixtures;
mod limits;
mod model;
mod roundtrip;
mod safety;
mod unknown_fields;
mod zip64;

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) use crate::addon::temp::TempRoot;

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

#[test]
fn the_temp_root_is_gone_once_dropped() {
    // 自证①：正常路径。用完（drop）之后连目录都不该存在。
    let path = {
        let root = TempRoot::new("self-proof");
        write_file(&root.join("overrides/mods/a.jar"), b"x");
        assert!(root.path().is_dir(), "活在的时候必须是真目录");
        root.path().to_path_buf()
    };
    assert!(
        !path.exists(),
        "drop 之后临时根必须被删掉：{}",
        path.display()
    );
}

#[test]
fn a_panicking_test_still_removes_its_temp_root() {
    // 自证②：panic 展开路径。断言"panic 也删"——这正是 4 GiB 稀疏文件残留的那条路：
    // 断言红了不代表可以留下 110 GB。
    let path = std::sync::Arc::new(std::sync::Mutex::new(PathBuf::new()));
    let slot = std::sync::Arc::clone(&path);
    let outcome = std::panic::catch_unwind(move || {
        let root = TempRoot::new("self-proof-panic");
        write_file(&root.join("overrides/mods/a.jar"), b"x");
        *slot.lock().expect("锁") = root.path().to_path_buf();
        panic!("故意失败：展开时守卫要照删不误");
    });
    assert!(outcome.is_err(), "这个用例必须真的走 panic 分支");
    let path = path.lock().expect("锁").clone();
    assert!(
        !path.exists(),
        "panic 之后临时根必须被删掉：{}",
        path.display()
    );
}

#[test]
fn a_sub_directory_root_takes_its_parent_with_it() {
    // 自证③：`under` 造的子树删的是**根**，不给 `%TEMP%` 留一堆空壳父目录。
    let (sub, parent) = {
        let root = TempRoot::under("self-proof-under", "instance");
        write_file(&root.join("mmc-pack.json"), b"{}");
        (
            root.path().to_path_buf(),
            root.path().parent().map(Path::to_path_buf),
        )
    };
    assert!(!sub.exists(), "子树要删");
    assert!(
        parent.is_some_and(|parent| !parent.exists()),
        "父目录也不许留空壳"
    );
}
