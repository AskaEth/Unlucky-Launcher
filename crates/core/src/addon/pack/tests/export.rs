//! 导出面的安全闸（r1 p2-1 / p3-1）：**跨格式导出必须被拒，失败不留半个包**。
//!
//! 这一组不测"映射是否做对"（那是 M112），只测"表达不了的时候会不会静默产出空包"。

use super::{fixtures, temp_root};
use crate::addon::pack::archive::PackArchive;
use crate::addon::pack::curseforge::CURSEFORGE;
use crate::addon::pack::error::PackErrorKind;
use crate::addon::pack::format::{PackFormat, PackPayload};
use crate::addon::pack::manifest::{OverrideFile, PackManifest, PackOverrides, PackSide};
use crate::addon::pack::modrinth::MODRINTH;
use crate::addon::pack::multimc::MULTIMC;

#[test]
fn a_multimc_instance_cannot_be_written_as_mrpack() {
    // r1 p2-1 实测过的那条：MultiMC 模型写 mrpack 会返回 Ok，但覆盖内容落在 `.minecraft/...`，
    // mrpack 导入器只解压 overrides / client-overrides / server-overrides ⇒ mods 数量为 0。
    // 现在必须在写第一个字节之前就拒。
    let root = fixtures::multimc("export-mmc-mr");
    let pack = PackArchive::open_default(&root).expect("打开实例目录");
    let mut manifest = MULTIMC.read(&pack).expect("读 MultiMC 清单");
    manifest.version = Some("1.0.0".to_string()); // mrpack 需要 versionId，别让别的错误顶包

    let target = root.with_file_name("cross.mrpack");
    let error = MODRINTH
        .write(&manifest, PackPayload::from_root(&root), &target)
        .expect_err("跨格式导出必须被拒，不能产出 mods 为 0 的包");
    assert_eq!(error.kind(), PackErrorKind::Unsupported, "{error}");
    assert!(
        error.message().contains(".minecraft"),
        "错误要说清是哪个目录表达不了：{}",
        error.message()
    );
    assert!(error.hint().is_some(), "要给下一步提示（目录映射属 M112）");
    assert!(!target.exists(), "被拒时不许留下目标文件");
}

#[test]
fn an_mrpack_with_split_sides_cannot_be_written_as_curseforge() {
    // CF 只有一个 `overrides` 目录字段，表达不了 client/server 分侧；现在是拒，不是猜。
    let fixture = fixtures::modrinth("export-mr-cf");
    let pack = PackArchive::open_default(&fixture.zip).expect("打开 mrpack 夹具");
    let manifest = MODRINTH.read(&pack).expect("读 mrpack 清单");

    let target = fixture.zip.with_file_name("cross.zip");
    let error = CURSEFORGE
        .write(&manifest, PackPayload::from_root(&fixture.dir), &target)
        .expect_err("CF 表达不了分侧覆盖目录");
    assert_eq!(error.kind(), PackErrorKind::Unsupported, "{error}");
    assert!(
        error.message().contains("client"),
        "错误要指出被拒的是哪一侧：{}",
        error.message()
    );
    assert!(!target.exists(), "被拒时不许留下目标文件");
}

#[test]
fn curseforge_refuses_more_than_one_both_side_group() {
    // CF 的 `overrides` 只有一个字段：两个 Both 组必然有一个不被清单声明（静默丢内容）。
    let mut manifest = PackManifest::new("两个 Both 组");
    manifest.minecraft = Some("1.20.1".to_string());
    manifest
        .overrides
        .push(PackOverrides::new("overrides", PackSide::Both));
    manifest
        .overrides
        .push(PackOverrides::new("extra-overrides", PackSide::Both));

    let target = temp_root("export-cf-two").join("out.zip");
    let error = CURSEFORGE
        .write(&manifest, PackPayload::metadata_only(), &target)
        .expect_err("多出来的 Both 组会被清单漏掉，必须拒");
    assert_eq!(error.kind(), PackErrorKind::Unsupported, "{error}");
    assert!(!target.exists(), "被拒时不许留下目标文件");
}

#[test]
fn a_failed_export_leaves_neither_a_half_pack_nor_a_temp_file() {
    // r1 p3-1：清单写完了、覆盖文件读不出来 → 目标路径与临时文件都不许留。
    let mut manifest = PackManifest::new("缺覆盖文件的模型");
    manifest.minecraft = Some("1.20.1".to_string());
    manifest.overrides.push(PackOverrides {
        dir: "overrides".to_string(),
        side: PackSide::Both,
        files: vec![OverrideFile {
            path: "mods/missing.jar".to_string(),
            sha1: "0".repeat(40),
            size: 1,
        }],
    });

    let root = temp_root("export-half");
    std::fs::create_dir_all(&root).expect("建目录");
    let target = root.join("out.zip");
    let error = CURSEFORGE
        .write(&manifest, PackPayload::from_root(&root), &target)
        .expect_err("覆盖文件不存在时必须失败");
    assert_eq!(error.kind(), PackErrorKind::Io, "{error}");
    assert!(!target.exists(), "失败后目标路径不许存在");

    let leftovers: Vec<String> = std::fs::read_dir(&root)
        .expect("列目录")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        leftovers.is_empty(),
        "临时文件没清干净（p3-1 回退）：{leftovers:?}"
    );
}

#[test]
fn same_format_export_is_still_allowed() {
    // 反向对照：这两条闸不能把同格式往返也挡掉（那是 r1 明确要保住的）。
    let fixture = fixtures::curseforge("export-same-cf");
    let pack = PackArchive::open_default(&fixture.zip).expect("打开 CF 夹具");
    let manifest = CURSEFORGE.read(&pack).expect("读 CF 清单");
    CURSEFORGE
        .check_overrides(&manifest)
        .expect("CF 自己读出来的模型必须能被 CF 表达");

    let fixture = fixtures::modrinth("export-same-mr");
    let pack = PackArchive::open_default(&fixture.zip).expect("打开 mrpack 夹具");
    let manifest = MODRINTH.read(&pack).expect("读 mrpack 清单");
    MODRINTH
        .check_overrides(&manifest)
        .expect("mrpack 自己读出来的模型必须能被 mrpack 表达");
}

#[test]
fn mrpack_refuses_two_override_groups_for_the_same_side() {
    // r2 p3-a：mrpack 的「每侧只能有一个覆盖目录」分支此前零测试（删掉它 59 条全绿）。
    // 目录名检查管不到这里：两个组的 `dir` 都合法，区别只在于同一侧出现了两次。
    let mut manifest = PackManifest::new("同侧两个覆盖目录");
    manifest.version = Some("1.0.0".to_string());
    manifest.minecraft = Some("1.20.1".to_string());
    manifest
        .overrides
        .push(PackOverrides::new("overrides", PackSide::Both));
    manifest
        .overrides
        .push(PackOverrides::new("overrides", PackSide::Both));

    let target = temp_root("export-mr-same-side").join("out.mrpack");
    let error = MODRINTH
        .write(&manifest, PackPayload::metadata_only(), &target)
        .expect_err("同一侧两个覆盖目录必须拒");
    assert_eq!(error.kind(), PackErrorKind::Unsupported, "{error}");
    assert!(
        error.message().contains("每个侧只能有一个覆盖目录"),
        "错误要指出是「同侧重复」而不是「目录名不对」：{}",
        error.message()
    );
    assert!(!target.exists(), "被拒时不许留下目标文件");
}
