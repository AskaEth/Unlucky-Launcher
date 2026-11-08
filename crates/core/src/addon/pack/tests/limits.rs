//! 限额口径的统一（M160）：**导入读取与导出写入只此一份常数**，且不等式关系钉死在这里。
//!
//! # 为什么需要这个文件
//!
//! M152 之前两侧各写各的：读侧 `PackLimits` 是 20 000 条 / 512 MiB / 64 MiB，写侧直接拿
//! `zip::ZIP64_BYTES_THR`（≈4 GiB）与 u32。结果是一个**能导出、导不回来**的包是合法的。
//! 现在写侧改读同一份常量（`WriteLimits::production`），本文件就是那条口径的证据：
//!
//! 1. 三个常量之间的关系（`entry ≤ total < ZIP64 ≤ u32`）；
//! 2. 读侧 `PackLimits::default()` 就是这三个数；
//! 3. 写侧在**单条边界**上拒（`>` 而不是 `≥`：恰好 64 MiB 合法）；
//! 4. 写侧在**总量**上拒，且拒绝发生在读内容之前（临时目录里什么都不留）。

use super::{write_file, TempRoot};
use crate::addon::pack::archive::limits::{
    PACK_MAX_ENTRIES, PACK_MAX_ENTRY_BYTES, PACK_MAX_UNCOMPRESSED_BYTES, PACK_ZIP64_BYTES_THR,
};
use crate::addon::pack::curseforge::CURSEFORGE;
use crate::addon::pack::error::PackErrorKind;
use crate::addon::pack::format::{PackFormat, PackPayload};
use crate::addon::pack::manifest::{OverrideFile, PackManifest, PackOverrides, PackSide};
use crate::addon::pack::PackLimits;

/// 一个覆盖文件的声明（内容由调用方提供）。
fn override_file(path: &str) -> OverrideFile {
    OverrideFile {
        path: path.to_string(),
        sha1: "0".repeat(40),
        size: 0,
    }
}

fn manifest_of(files: Vec<OverrideFile>) -> PackManifest {
    let mut manifest = PackManifest::new("限额口径");
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
fn sparse_file(path: &std::path::Path, length: u64) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建目录");
    }
    let file = std::fs::File::create(path).expect("建稀疏文件");
    file.set_len(length).expect("拉长文件（稀疏）");
}

#[test]
fn the_write_side_refuses_a_pack_over_the_unified_entry_count() {
    // 写侧以前只有总量与单条两道闸：内容目录超过 2 万条的实例**导得出来、自己拒读**
    // （读侧 `archive::open` 的「条目数超过上限」）。这条钉住第三道闸，以及它挡在**读盘之前**。
    let root = TempRoot::new("limits-entries");
    write_file(&root.join("overrides/mods/a.jar"), b"x");
    let target = root.join("out.zip");

    // ① 对照：条目数远低于上限时不许误伤。
    CURSEFORGE
        .write(
            &manifest_of(vec![override_file("mods/a.jar")]),
            PackPayload::from_root(&root),
            &target,
        )
        .expect("单个覆盖条目必须写得出来");
    let _ = std::fs::remove_file(&target);

    // ② 声明 20001 条（上限 20000 + 1）：只有**第一条**的文件真的存在，其余 2 万条谁都不在盘上。
    //    报出来的必须是 `pack_limits` 而不是 `pack_io` —— 那就是"条目数闸在读第一个文件之前"的证据
    //    （真去 `metadata()` 的话，第二个文件就已经报 `pack_io` 了）。
    let mut files = vec![override_file("mods/a.jar")];
    for index in 1..=PACK_MAX_ENTRIES {
        files.push(override_file(&format!("mods/{index:05}.jar")));
    }
    assert_eq!(files.len(), PACK_MAX_ENTRIES + 1, "上限 + 1 条");
    let error = CURSEFORGE
        .write(&manifest_of(files), PackPayload::from_root(&root), &target)
        .expect_err("超过条目数上限必须拒");
    assert_eq!(error.kind(), PackErrorKind::Limits, "{error}");
    assert!(
        error.message().contains(&PACK_MAX_ENTRIES.to_string()),
        "文案要给出统一上限那个数（读侧用的是同一个）：{error}"
    );
    assert!(
        error.hint().is_some_and(|hint| hint.contains("移出实例")),
        "要给可执行的下一步：{error:?}"
    );
    assert!(!target.exists(), "预检失败时目标路径不许出现");
}

#[test]
fn the_entry_count_gate_is_the_same_number_as_the_read_side() {
    // 「三个常量同源」的写侧那一半：生产口径的条目数上限必须**就是**读侧那个常量。
    // 改坏它会先在 `pick` 这层红，而不是等某个用户导出 2 万条内容的实例才发现。
    let production = crate::addon::pack::archive::limits::PACK_MAX_ENTRIES;
    let read_side = PackLimits::default().max_entries;
    assert_eq!(production, read_side, "写侧与读侧必须是同一个数");
    assert_eq!(read_side, 20_000, "上限本身也钉住（改数值要显式改这里）");
}

#[test]
fn the_three_limits_form_one_chain_and_the_read_side_uses_them() {
    // ① 关系：单条 ≤ 总量 < ZIP64 ≤ u32。前一半保证"能写出来的包读得回来"，
    //    后一半保证我们永远不需要写 ZIP64，也永远撞不到 mrpack 的 fileSize(u32) 上限。
    //    这三条是**编译期**断言（`const` 块）：改坏常量会在 `cargo check` 就红，不用等跑测试。
    const _: () = assert!(
        PACK_MAX_ENTRY_BYTES <= PACK_MAX_UNCOMPRESSED_BYTES,
        "单条上限必须 ≤ 总量上限（否则单条规则永远够不着）"
    );
    const _: () = assert!(
        PACK_MAX_UNCOMPRESSED_BYTES < PACK_ZIP64_BYTES_THR,
        "总量上限必须严格低于 ZIP64 阈值"
    );
    const _: () = assert!(
        PACK_ZIP64_BYTES_THR <= u32::MAX as u64,
        "ZIP64 阈值也不该越过 u32（mrpack fileSize 的口径）"
    );

    // ② 读侧默认限额就是这三个数（不是"看起来一样"的另一份拷贝）。
    let limits = PackLimits::default();
    assert_eq!(limits.max_entries, PACK_MAX_ENTRIES);
    assert_eq!(limits.max_uncompressed_bytes, PACK_MAX_UNCOMPRESSED_BYTES);
    assert_eq!(limits.max_entry_bytes, PACK_MAX_ENTRY_BYTES);
}

#[test]
fn the_write_side_refuses_a_single_entry_over_the_unified_entry_limit() {
    let root = TempRoot::new("limits-entry");
    // 恰好 +1 字节超限：拒绝理由是**单条**上限（总量 64 MiB + 1 仍在 512 MiB 之下）。
    sparse_file(
        &root.join("overrides/mods/too-big.jar"),
        PACK_MAX_ENTRY_BYTES + 1,
    );
    let target = root.join("out.zip");

    let error = CURSEFORGE
        .write(
            &manifest_of(vec![override_file("mods/too-big.jar")]),
            PackPayload::from_root(&root),
            &target,
        )
        .expect_err("超过单条统一上限必须拒");

    assert_eq!(error.kind(), PackErrorKind::Limits, "{error}");
    assert!(
        error.message().contains(&PACK_MAX_ENTRY_BYTES.to_string()),
        "文案要给出那个数（读侧用的是同一个）：{error}"
    );
    assert!(
        error.hint().is_some_and(|hint| hint.contains("移出实例")),
        "要给可执行的下一步：{error:?}"
    );
    assert!(!target.exists(), "预检失败时目标路径不许出现");
    // 预检在建临时文件之前就失败 ⇒ 目录里只有源目录，没有 `out.zip.*.part`。
    let leftovers: Vec<String> = std::fs::read_dir(&root)
        .expect("列目录")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name != "overrides")
        .collect();
    assert!(leftovers.is_empty(), "不许留临时文件：{leftovers:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_entry_boundary_is_exactly_the_limit() {
    // 边界口径是 `>`：恰好 64 MiB 合法（与读侧的 `size > max_entry_bytes` 同一侧）。
    let root = TempRoot::new("limits-boundary");
    sparse_file(&root.join("overrides/mods/exact.jar"), PACK_MAX_ENTRY_BYTES);
    let target = root.join("out.zip");

    CURSEFORGE
        .write(
            &manifest_of(vec![override_file("mods/exact.jar")]),
            PackPayload::from_root(&root),
            &target,
        )
        .expect("恰好等于上限必须写得出来（否则两侧的边界就错开了）");
    let written = crate::addon::pack::PackArchive::open(
        &target,
        // 读回来时单条上限就是"恰好等于它"，证明写出来的东西读侧认。
        PackLimits {
            max_entry_bytes: PACK_MAX_ENTRY_BYTES,
            ..PackLimits::default()
        },
    )
    .expect("刚写出来的包必须能被自己的读取层打开");
    assert!(written.has_entry("overrides/mods/exact.jar"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_write_side_refuses_a_total_over_the_unified_total_limit() {
    // 每条都在单条上限之内（64 MiB），但只要 9 条就超总量（576 MiB > 512 MiB）——
    // 这条与"单条规则"是两件事：判据是**累计**。
    let root = TempRoot::new("limits-total");
    let mut files = Vec::new();
    for index in 0..9 {
        let relative = format!("mods/part-{index}.jar");
        sparse_file(
            &root.join("overrides").join(&relative),
            PACK_MAX_ENTRY_BYTES,
        );
        files.push(override_file(&relative));
    }
    let total = PACK_MAX_ENTRY_BYTES * 9;
    assert!(total > PACK_MAX_UNCOMPRESSED_BYTES, "样本必须真的超总量");
    let target = root.join("out.zip");

    let error = CURSEFORGE
        .write(&manifest_of(files), PackPayload::from_root(&root), &target)
        .expect_err("累计超总量必须拒");

    assert_eq!(error.kind(), PackErrorKind::Limits, "{error}");
    assert!(
        error
            .message()
            .contains(&PACK_MAX_UNCOMPRESSED_BYTES.to_string()),
        "文案要给出统一总量上限：{error}"
    );
    assert!(!target.exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_real_pack_written_under_the_limit_reopens_under_the_read_limits() {
    // 端到端的那句话："我们能写出的包一定能被自己读回来"。用一份正常尺寸的覆盖内容跑一遍。
    let root = TempRoot::new("limits-parity");
    write_file(&root.join("overrides/mods/jei.jar"), b"PK\x03\x04 jei");
    write_file(
        &root.join("overrides/config/known.toml"),
        b"shared = true\n",
    );
    let target = root.join("out.mrpack");

    crate::addon::pack::modrinth::MODRINTH
        .write(
            &manifest_of(vec![
                override_file("mods/jei.jar"),
                override_file("config/known.toml"),
            ]),
            PackPayload::from_root(&root),
            &target,
        )
        .expect("正常尺寸必须写得出来");

    let pack = crate::addon::pack::PackArchive::open_default(&target)
        .expect("默认限额（= 统一口径）必须读得回来");
    assert!(pack.has_entry("overrides/mods/jei.jar"));
    assert!(pack.has_entry("overrides/config/known.toml"));
    let _ = std::fs::remove_dir_all(&root);
}
