//! ZIP64 拒写（research/18-p2 §8.6 / 表 B #17 的导出侧那条）。
//!
//! # 为什么这个文件以前是空的
//!
//! M112 的完成报告写明：「ZIP64 阈值那条分支只有代码没有端到端测试（要造 4 GiB 数据）」。
//! 而更要紧的是——**那条分支当时根本够不着**：实现先 `std::fs::read` 整个覆盖文件进内存，
//! 再累加比较，所以真要导出一个 4 GiB 文件的实例，内存先炸，用户永远看不到那句
//! `pack_limits`。现在写侧改成「先按 `metadata().len()` 预检」，两个问题一起解决：
//!
//! - 边界是真的会在读第一个字节之前拦下来的（本文件的用例就断言这一点：失败后临时目录里
//!   没有 `.part`、目标路径不存在）；
//! - 测试可以用**稀疏文件**造出 4 GiB 的"声明尺寸"而几乎不占磁盘（`set_len` 只改 EOF，
//!   NTFS / ext4 都不会真写零）。这也是唯一能在不塞满硬盘的前提下打通这条分支的办法。

use std::path::Path;

use super::temp_root;
use crate::addon::pack::curseforge::CURSEFORGE;
use crate::addon::pack::error::PackErrorKind;
use crate::addon::pack::format::{PackFormat, PackPayload};
use crate::addon::pack::manifest::{OverrideFile, PackManifest, PackOverrides, PackSide};

/// 一个覆盖文件的声明（内容由调用方提供）。
fn override_file(path: &str) -> OverrideFile {
    OverrideFile {
        path: path.to_string(),
        sha1: "0".repeat(40),
        size: 0,
    }
}

fn manifest_of(files: Vec<OverrideFile>) -> PackManifest {
    let mut manifest = PackManifest::new("超大包");
    manifest.version = Some("1.0.0".to_string());
    manifest.minecraft = Some("1.20.1".to_string());
    manifest.overrides.push(PackOverrides {
        dir: "overrides".to_string(),
        side: PackSide::Both,
        files,
    });
    manifest
}

/// 造一个只占元数据的 `length` 字节文件（`set_len` 不写零）。
fn sparse_file(path: &Path, length: u64) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建目录");
    }
    let file = std::fs::File::create(path).expect("建稀疏文件");
    file.set_len(length).expect("拉长文件（稀疏）");
}

/// 临时目录里除 `keep` 之外的文件名（断言"没留下 .part"用）。
fn leftovers(root: &Path, keep: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .expect("列目录")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| !keep.contains(&name.as_str()))
        .collect();
    names.sort();
    names
}

#[test]
fn one_file_over_the_threshold_is_refused_before_a_single_byte_is_read() {
    let root = temp_root("zip64-single");
    // 恰好一个字节超过阈值：预检必须在这里就报，不许读内容（真读会先要 4 GiB 内存）。
    sparse_file(
        &root.join("overrides/mods/huge.jar"),
        zip::ZIP64_BYTES_THR + 1,
    );
    let manifest = manifest_of(vec![override_file("mods/huge.jar")]);
    let target = root.join("out.zip");

    let error = CURSEFORGE
        .write(&manifest, PackPayload::from_root(&root), &target)
        .expect_err("超 ZIP64 阈值必须拒写");

    assert_eq!(error.kind(), PackErrorKind::Limits, "{error}");
    assert!(error.message().contains("ZIP64"), "{error}");
    assert!(
        error.hint().is_some_and(|hint| hint.contains("移出实例")),
        "要给「把大文件移出实例」这种可执行的提示：{error:?}"
    );
    assert!(!target.exists(), "失败后目标路径不许存在");
    // 临时文件也不许留（预检在建临时文件之前就失败 ⇒ 目录里只有源文件）。
    assert_eq!(leftovers(&root, &["overrides"]), Vec::<String>::new());
}

#[test]
fn the_threshold_is_cumulative_not_per_file() {
    // 三个文件各自都远小于阈值，加起来超了：判据是**整包未压缩总量**，不是单文件。
    let root = temp_root("zip64-cumulative");
    let each = 1_610_612_736_u64; // 1.5 GiB
    assert!(each < zip::ZIP64_BYTES_THR, "单条必须都在阈值以下");
    assert!(each * 3 > zip::ZIP64_BYTES_THR, "三条加起来必须超标");
    let mut files = Vec::new();
    for index in 0..3 {
        let relative = format!("mods/part-{index}.jar");
        sparse_file(&root.join("overrides").join(&relative), each);
        files.push(override_file(&relative));
    }

    let target = root.join("out.zip");
    let error = CURSEFORGE
        .write(&manifest_of(files), PackPayload::from_root(&root), &target)
        .expect_err("累计超阈值同样必须拒写");

    assert_eq!(error.kind(), PackErrorKind::Limits, "{error}");
    assert!(error.message().contains("ZIP64"), "{error}");
    assert!(!target.exists());
}

#[test]
fn an_export_under_the_threshold_still_writes_a_normal_pack() {
    // 对照：不超阈值时预检不许误伤（否则这条护栏就变成"导出全挂"）。
    let root = temp_root("zip64-under");
    let content = b"a normal sized override file";
    std::fs::create_dir_all(root.join("overrides/mods")).expect("建目录");
    std::fs::write(root.join("overrides/mods/jei.jar"), content).expect("写覆盖文件");
    let manifest = manifest_of(vec![override_file("mods/jei.jar")]);
    let target = root.join("out.zip");

    CURSEFORGE
        .write(&manifest, PackPayload::from_root(&root), &target)
        .expect("正常尺寸必须写得出来");
    assert!(target.exists());
    let written = crate::addon::pack::PackArchive::open_default(&target).expect("打开导出的包");
    assert!(written.has_entry("overrides/mods/jei.jar"));
}

/// 端到端版的窗口回归钉（**不进默认套件**：真写 2.5 GB 要 ~70 秒 deflate，且会把机器内存 /
/// 页面文件顶上去；塔台 2026-11-07 的资源配置令要求避免）。
///
/// 默认套件里由 `format::write` 的单测 `the_second_gate_counts_from_the_manifest_length_not_
/// the_preflight_total` 便宜地钉住同一个窗口（把窗口里的字节数直接喂给第二道闸的计数器）。
/// 本条留给"真跑一遍确认容器层接线没错"的场合：
///
/// ```text
/// cargo test -p unlucky-core --lib -- zip64 --ignored --nocapture
/// ```
///
/// 交付时的实测（M152 r2）：单文件 2 500 000 000 字节导出**成功**，包内条目声明尺寸原样为
/// 2.5 GB，耗时 ~70 秒、源文件稀疏（占盘 0），产物是压缩后的零（几 MB）。
#[test]
#[ignore = "真写 2.5 GB（~70 秒 deflate + 一片内存）：默认套件用 Written 的便宜单测钉同一窗口"]
fn a_single_file_inside_the_double_count_window_still_exports() {
    // P2-1 的回归钉：预检返回的是**全量**声明尺寸，写循环曾经直接拿它当累加基数，于是每个
    // 文件的字节被算两遍 —— 真实总量只要过半阈值（≈2.15 GiB）就在内容读完**之后**被误拒。
    // 2.5 GB 正落在这个窗口里：它必须能导出，且中途不许把整个人内容摊进内存。
    let root = temp_root("zip64-window");
    let length = 2_500_000_000_u64;
    assert!(length < zip::ZIP64_BYTES_THR, "样本必须在阈值之下");
    assert!(
        length > zip::ZIP64_BYTES_THR / 2,
        "样本必须落在过拒窗口里（≥ 阈值一半）"
    );
    sparse_file(&root.join("overrides/mods/big.jar"), length);
    let manifest = manifest_of(vec![override_file("mods/big.jar")]);
    let target = root.join("out.zip");

    let result = CURSEFORGE.write(&manifest, PackPayload::from_root(&root), &target);
    assert!(
        result.is_ok(),
        "2.5 GB 的覆盖文件没超 ZIP64 阈值，不许误拒：{:?}",
        result.err()
    );

    // 包内条目的**声明未压缩尺寸**就是 2.5 GB（真写出来的是压缩后的零，只有几 MB）。
    // 这里直接用 `zip` 的读者，而不是 `PackArchive::open_default`：后者对**不可信输入**有
    // 单条目 64 MiB 的上限（`PackLimits`），拿它读我们自己刚写的包会撞限额，与本次判据无关。
    let file = std::fs::File::open(&target).expect("打开导出包");
    let mut archive = zip::ZipArchive::new(file).expect("读 zip");
    let entry = archive
        .by_name("overrides/mods/big.jar")
        .expect("覆盖条目要在包里");
    assert_eq!(entry.size(), length, "声明尺寸要原样带过去");

    // 稀疏源文件只占元数据，但产物与目录都清掉，别让 temp 越积越多。
    let _ = std::fs::remove_dir_all(&root);
}
