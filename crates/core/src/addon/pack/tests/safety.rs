//! 恶意样本：每条安全判据都配一条"会失败的包"，而不是靠注释保证。
//!
//! 覆盖：路径穿越四类形状（`..` / 绝对路径 / 反斜杠 / 盘符）、符号链接条目、条目数上限、
//! 解压总体积上限、单条目上限、规整后重名、篡改载荷（CRC 不符）、以及**清单字段里的**穿越
//! （mrpack `files[].path`、CF `overrides`）。

use std::path::{Path, PathBuf};

use super::{temp_root, write_zip, write_zip_with_symlink};
use crate::addon::pack::archive::{PackArchive, PackLimits};
use crate::addon::pack::curseforge::CURSEFORGE;
use crate::addon::pack::error::PackErrorKind;
use crate::addon::pack::format::PackFormat;
use crate::addon::pack::manifest::{PackLoader, PackManifest, PackOverrides, PackSide};
use crate::addon::pack::modrinth::MODRINTH;

/// 打一个只有一条恶意条目的 zip。
fn single_entry(tag: &str, name: &str) -> PathBuf {
    let target = temp_root(tag).join("evil.zip");
    write_zip(&target, &[(name, &b"payload"[..])]);
    target
}

fn open_error(target: &Path) -> crate::addon::pack::error::PackError {
    PackArchive::open_default(target).expect_err("恶意包必须被拒")
}

#[test]
fn rejects_parent_directory_entries() {
    let target = single_entry("trav-parent", "../evil.txt");
    let error = open_error(&target);
    assert_eq!(
        error.kind(),
        PackErrorKind::Traversal,
        "{}",
        error.message()
    );
}

#[test]
fn rejects_absolute_entries() {
    let target = single_entry("trav-absolute", "/etc/passwd");
    let error = open_error(&target);
    assert_eq!(
        error.kind(),
        PackErrorKind::Traversal,
        "{}",
        error.message()
    );
}

#[test]
fn rejects_backslash_separators() {
    // Win32 把 `\` 当分隔符：按 `/` 切是"一个普通名字"，落盘却跳三层。
    let target = single_entry("trav-backslash", "mods\\..\\..\\evil.jar");
    let error = open_error(&target);
    assert_eq!(
        error.kind(),
        PackErrorKind::Traversal,
        "{}",
        error.message()
    );
}

#[test]
fn rejects_drive_letter_and_ads_entries() {
    for (tag, name) in [
        ("trav-drive", "C:/Windows/evil.dll"),
        ("trav-ads", "mods/jei.jar:Zone.Identifier"),
    ] {
        let target = single_entry(tag, name);
        let error = open_error(&target);
        assert_eq!(
            error.kind(),
            PackErrorKind::Traversal,
            "{name}: {}",
            error.message()
        );
    }
}

#[test]
fn rejects_symlink_entries() {
    let target = temp_root("symlink").join("link.zip");
    write_zip_with_symlink(
        &target,
        &[("readme.txt", &b"x"[..])],
        "mods/evil.jar",
        "../../../etc/passwd",
    );
    let error = open_error(&target);
    assert_eq!(error.kind(), PackErrorKind::Symlink, "{}", error.message());
}

#[test]
fn rejects_too_many_entries() {
    let target = temp_root("limit-count").join("many.zip");
    write_zip(
        &target,
        &[
            ("a.txt", &b"1"[..]),
            ("b.txt", &b"2"[..]),
            ("c.txt", &b"3"[..]),
            ("d.txt", &b"4"[..]),
        ],
    );
    let error = PackArchive::open(&target, PackLimits::new(3, 1024 * 1024, 1024 * 1024))
        .expect_err("条目数超限必须拒");
    assert_eq!(error.kind(), PackErrorKind::Limits, "{}", error.message());
}

#[test]
fn rejects_uncompressed_volume_over_limit() {
    let target = temp_root("limit-volume").join("big.zip");
    let payload = vec![b'a'; 4096];
    write_zip(
        &target,
        &[("a.bin", payload.as_slice()), ("b.bin", payload.as_slice())],
    );
    let error = PackArchive::open(&target, PackLimits::new(16, 4096, 4096))
        .expect_err("解压总体积超限必须拒");
    assert_eq!(error.kind(), PackErrorKind::Limits, "{}", error.message());
}

#[test]
fn rejects_a_single_oversized_entry() {
    let target = temp_root("limit-entry").join("big.zip");
    let payload = vec![b'a'; 2048];
    write_zip(&target, &[("a.bin", payload.as_slice())]);
    let error = PackArchive::open(&target, PackLimits::new(16, 1024 * 1024, 512))
        .expect_err("单条目超限必须拒");
    assert_eq!(error.kind(), PackErrorKind::Limits, "{}", error.message());
}

#[test]
fn rejects_names_that_collide_after_normalization() {
    // `./a.txt` 与 `a.txt` 规整后同名：留着就是"读到的和落盘的"不是同一个文件的经典混淆。
    let target = temp_root("dup-name").join("dup.zip");
    write_zip(&target, &[("./a.txt", &b"1"[..]), ("a.txt", &b"2"[..])]);
    let error = open_error(&target);
    assert_eq!(error.kind(), PackErrorKind::Metadata, "{}", error.message());
}

#[test]
fn a_tampered_payload_is_not_silently_accepted() {
    // 中央目录没问题、数据被改过（CRC 不符）：打开能过，读内容必须报错。
    const MARKER: &[u8] = b"UNIQUE-PAYLOAD-MARKER";
    let target = temp_root("tampered").join("tampered.zip");
    write_zip(
        &target,
        &[
            (
                "manifest.json",
                br#"{"manifestType":"minecraftModpack","manifestVersion":1,
                     "name":"tampered pack","minecraft":{"version":"1.20.1"},"files":[],
                     "overrides":"overrides"}"#,
            ),
            ("overrides/config/bad.toml", MARKER),
        ],
    );
    let mut bytes = std::fs::read(&target).expect("读 zip");
    let at = bytes
        .windows(MARKER.len())
        .position(|window| window == MARKER)
        .expect("找得到载荷");
    bytes[at] = b'X';
    std::fs::write(&target, bytes).expect("写回 zip");

    let pack = PackArchive::open_default(&target).expect("中央目录完好，打开该成功");
    let error = pack
        .read("overrides/config/bad.toml")
        .expect_err("载荷被改过，读内容必须失败");
    assert!(
        matches!(error.kind(), PackErrorKind::Io | PackErrorKind::Metadata),
        "实际大类：{:?}（{}）",
        error.kind(),
        error.message()
    );
    // 整包读取会算覆盖文件的 SHA-1，因此也必须失败，而不是"算出一个错的哈希"。
    assert!(CURSEFORGE.read(&pack).is_err());
}

#[test]
fn mrpack_file_path_traversal_is_rejected() {
    let target = temp_root("mr-trav").join("evil.mrpack");
    let index = r#"{"game":"minecraft","formatVersion":1,"versionId":"1.0","name":"evil pack",
      "files":[{"path":"../evil.jar",
                "hashes":{"sha512":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"},
                "downloads":["https://example.invalid/evil.jar"]}],
      "dependencies":{"minecraft":"1.20.1"}}"#;
    write_zip(&target, &[("modrinth.index.json", index.as_bytes())]);
    let pack = PackArchive::open_default(&target).expect("zip 本身可读");
    let error = MODRINTH.read(&pack).expect_err("清单里的穿越路径必须拒");
    assert_eq!(
        error.kind(),
        PackErrorKind::Traversal,
        "{}",
        error.message()
    );
}

#[test]
fn curseforge_overrides_dir_traversal_is_rejected() {
    let target = temp_root("cf-trav").join("evil.zip");
    write_zip(
        &target,
        &[(
            "manifest.json",
            br#"{"manifestType":"minecraftModpack","manifestVersion":1,"name":"evil pack",
                 "minecraft":{"version":"1.20.1"},"files":[],"overrides":"../../etc"}"#,
        )],
    );
    let pack = PackArchive::open_default(&target).expect("zip 本身可读");
    let error = CURSEFORGE
        .read(&pack)
        .expect_err("overrides 目录指向包外必须拒");
    assert_eq!(
        error.kind(),
        PackErrorKind::Traversal,
        "{}",
        error.message()
    );
}

#[test]
fn write_side_path_check_catches_hand_built_models() {
    // 模型可能不是我们读出来的（UI/AI 手搓）：导出前必须再查一遍。
    let mut manifest = PackManifest::new("手搓模型");
    manifest.overrides.push(PackOverrides {
        dir: "overrides".to_string(),
        side: PackSide::Both,
        files: Vec::new(),
    });
    manifest
        .loaders
        .push(PackLoader::parse_curse("forge-47.2.0", true));
    assert!(manifest.check_paths().is_ok());

    manifest.overrides[0].dir = "../escape".to_string();
    assert_eq!(
        manifest
            .check_paths()
            .expect_err("越界目录必须被写侧检查拦下")
            .kind(),
        PackErrorKind::Traversal
    );
}

#[test]
fn missing_package_is_an_io_error() {
    let target = temp_root("missing").join("does-not-exist.zip");
    let error = open_error(&target);
    assert_eq!(error.kind(), PackErrorKind::Io, "{}", error.message());
}

#[test]
fn directory_source_rejects_symlinks_when_the_platform_can_make_them() {
    // Windows 建符号链接需要开发者模式/管理员：建不出来就跳过，但绝不静默当通过。
    #[cfg(unix)]
    {
        let root = temp_root("dir-symlink").join("instance");
        std::fs::create_dir_all(&root).expect("建目录");
        std::fs::write(root.join("mmc-pack.json"), b"{}").expect("写文件");
        std::os::unix::fs::symlink("/etc/passwd", root.join("escape")).expect("建符号链接");
        let error = open_error(&root);
        assert_eq!(error.kind(), PackErrorKind::Symlink, "{}", error.message());
    }
    #[cfg(not(unix))]
    {
        eprintln!(
            "跳过：当前平台建符号链接需要额外权限（目录侧的检查逻辑与 zip 侧共用同一条判据）"
        );
    }
}

#[test]
#[cfg(windows)]
fn the_second_check_catches_windows_device_names_and_dot_suffixes() {
    // r1 p2-3：第一检（平台无关的七条）对这些名字是**放行**的，拦下它们的是第二检
    // `enclosed_name()` 的 Win32 命名空间规则（DOS 设备名、结尾点/空格、`< > " | ? *`）。
    // 删掉第二检，这条用例必须红——这是「双检各有一条恶意样本」的第二条。
    for (tag, name) in [
        ("dev-con", "CON"),
        ("dev-nul-nested", "mods/NUL"),
        ("dev-com1", "COM1.txt"),
        ("dev-trailing-dot", "a."),
        ("dev-trailing-space", "mods/b "),
        ("dev-wildcard", "mods/*.jar"),
    ] {
        let target = single_entry(tag, name);
        let error = open_error(&target);
        assert_eq!(
            error.kind(),
            PackErrorKind::Traversal,
            "{name}: {}",
            error.message()
        );
        assert!(
            error.message().contains("enclosed_name"),
            "{name} 必须是被第二检拦下的（不然这条用例钉错了地方）：{}",
            error.message()
        );
    }

    // 对照：第一检对这些名字没有任何意见（所以它们只能靠第二检）。
    for name in ["CON", "mods/NUL", "a.", "mods/b ", "mods/*.jar"] {
        assert!(
            crate::addon::pack::path::normalize_file(name).is_ok(),
            "第一检不该拦 {name}（拦了这条用例就失去意义）"
        );
    }
}
