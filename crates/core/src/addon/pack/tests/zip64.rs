//! 导出侧的体积闸：**拒写**过大包（research/18-p2 §8.6 / 表 B #17 的导出侧那条）。
//!
//! # 现在的判据是「与导入读取同一份常量」（M160）
//!
//! 以前判据是 `zip::ZIP64_BYTES_THR`（≈4 GiB），而导入读取只允许 512 MiB —— 于是存在
//! 「导得出来、导不回去」的包。M160 把写侧上限换成读侧那份常量（`pack::archive::limits`），
//! 本文件的两条用例（单文件超大 / 累计超大）仍然钉住**同一件事**：闸门在碰内容之前就把包拒了。
//! 只是拒绝的理由从"要写 ZIP64"变成"超过统一上限，所以永远不会写出 ZIP64"。
//!
//! # 为什么用稀疏文件
//!
//! 判据是**声明尺寸**（`metadata().len()`），所以测试可以用稀疏文件造出 GB 级的"声明尺寸"而
//! 几乎不占磁盘（`set_len` 只改 EOF，NTFS / ext4 都不会真写零）。这也是唯一能在不塞满硬盘的
//! 前提下打通这条分支的办法。

use std::path::Path;

use super::TempRoot;
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
    let root = TempRoot::new("zip64-single");
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
    let root = TempRoot::new("zip64-cumulative");
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
    let root = TempRoot::new("zip64-under");
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

/// 端到端版的旧窗口回归钉 → **现在钉的是口径统一**（`#[ignore]`：要造稀疏大文件 + 一遍 deflate）。
///
/// M160 把写侧的上限换成读侧那份常量（512 MiB 总量 / 64 MiB 单条），所以 M152 时代
/// 「2.5 GB 单文件落在 ZIP64 过拒窗口里、必须能导出」这件事**按设计不再成立**：那个包现在导得
/// 出来、导不回去。本条留下的是新口径的端到端证据——2.5 GB 的覆盖文件在**碰内容之前**就被
/// 统一上限拒掉，且临时文件与目标路径都不留。
///
/// ```text
/// cargo test -p unlucky-core --lib -- zip64 --ignored --nocapture
/// ```
#[test]
#[ignore = "造 2.5 GB 稀疏文件再跑一遍预检：默认套件用 `Written` 的便宜单测与 tests/limits.rs 钉同一口径"]
fn a_single_file_over_the_old_zip64_window_is_refused_by_the_unified_limit() {
    let root = TempRoot::new("zip64-window");
    let length = 2_500_000_000_u64;
    assert!(
        length < crate::addon::pack::archive::limits::PACK_ZIP64_BYTES_THR,
        "样本仍在 ZIP64 阈值之下：拒绝的理由是**统一上限**，不是 ZIP64"
    );
    sparse_file(&root.join("overrides/mods/big.jar"), length);
    let manifest = manifest_of(vec![override_file("mods/big.jar")]);
    let target = root.join("out.zip");

    let error = CURSEFORGE
        .write(&manifest, PackPayload::from_root(&root), &target)
        .expect_err("超过统一上限（512 MiB）必须拒：否则导出的包自己导不回来");
    assert_eq!(error.kind(), PackErrorKind::Limits, "{error}");
    assert!(
        error.message().contains(
            &crate::addon::pack::archive::limits::PACK_MAX_UNCOMPRESSED_BYTES.to_string()
        ),
        "文案要给出统一上限那个数：{error}"
    );
    assert!(!target.exists(), "预检失败时目标路径不许出现");

    // 稀疏源文件只占元数据，但目录与临时文件都清掉，别让 temp 越积越多。
    let _ = std::fs::remove_dir_all(&root);
}
