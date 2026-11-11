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
//!
//! # 夹具是 KiB 级，判据仍是生产口径（M166）
//!
//! 第 3、4 条以前靠"造 64 MiB 的**稀疏**文件"验，以为稀疏不占磁盘。**实测不成立**：本机
//! NTFS 上 `set_len` 真占（`SetLength(1 GiB)` ⇒ C 盘可用空间掉 1024 MiB，见
//! `tests/zip64.rs` 的模块文档），那三条用例合计真占 ~700 MiB。
//!
//! 现在判据的**数值**用纯算术钉（`format/write/tests.rs` 的
//! `the_production_limits_are_the_shared_constants` 证明生产路径拿的就是这三个常量），
//! **分支**用 `format::write_with_limits` 把 limits 缩到 KiB 级来验——两条合起来等价于
//! 原来的"造 64 MiB 看文案"，但一条用例都 `#[ignore]`、一条断言都没删。

use super::{write_file, TempRoot};
use crate::addon::pack::archive::limits::{
    PACK_MAX_ENTRIES, PACK_MAX_ENTRY_BYTES, PACK_MAX_UNCOMPRESSED_BYTES, PACK_ZIP64_BYTES_THR,
};
use crate::addon::pack::curseforge::CURSEFORGE;
use crate::addon::pack::error::PackErrorKind;
use crate::addon::pack::format::{write_with_limits, PackFormat, PackPayload, WriteLimits};
use crate::addon::pack::manifest::{OverrideFile, PackManifest, PackOverrides, PackSide};
use crate::addon::pack::PackLimits;

/// 小 limits 用的单条目上限（生产口径是 64 MiB，这里缩到 2 KiB）。
const ENTRY: u64 = 2 * 1024;

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

/// 造一个真有 `length` 字节内容的覆盖文件（**不稀疏**，见模块文档）。
fn content_file(path: &std::path::Path, length: u64) {
    write_file(
        path,
        &vec![0_u8; usize::try_from(length).expect("夹具尺寸")],
    );
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
    //    这条**不需要**小 limits：清单条目本身不占磁盘，2 万条只是一个 JSON 数组。
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
    //    这三条是**编译期**断言（`const` 块，而且写在 `#[test] fn` 体内 ⇒ **只在测试构建里存在**）：
    //    改坏常量会在 `cargo test` / `cargo clippy --all-targets` 的编译期就红（E0080），
    //    不用等跑到测试函数；但**裸 `cargo check -p unlucky-core`** 是 lib 非测试构建，
    //    看不见这些断言（复审 r1 实测：E0080 只在 `--no-run`/`--all-targets` 下出现）。
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
    // 单条目那道闸。生产口径下"恰好 +1 字节超 64 MiB"要真占 64 MiB，所以这里把 limits
    // 缩到 KiB 级、触发**同一条** `guard_entry` 分支；生产数值由
    // `format/write/tests.rs::the_production_limits_are_the_shared_constants` 钉住。
    let root = TempRoot::new("limits-entry");
    let limits = WriteLimits::new(ENTRY * 4, ENTRY, 16);
    // 恰好 +1 字节超**单条**上限；声明总量（清单 + 2049）仍在总量上限 ENTRY*4 之下，
    // 所以只有 `guard_entry` 能拒——两档判据没串味。
    content_file(&root.join("overrides/mods/too-big.jar"), ENTRY + 1);
    assert!(
        ENTRY + 1 < limits.total,
        "样本的声明总量必须仍在总量上限之下"
    );
    let target = root.join("out.zip");

    let error = write_with_limits(
        &CURSEFORGE,
        &manifest_of(vec![override_file("mods/too-big.jar")]),
        PackPayload::from_root(&root),
        &target,
        limits,
    )
    .expect_err("超过单条统一上限必须拒");

    assert_eq!(error.kind(), PackErrorKind::Limits, "{error}");
    assert!(
        error.message().contains(&ENTRY.to_string()),
        "文案要给出在用的那个单条上限：{error}"
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
}

#[test]
fn the_entry_boundary_is_exactly_the_limit() {
    // 边界口径是 `>`：恰好等于上限合法（与读侧的 `size > max_entry_bytes` 同一侧）。
    let root = TempRoot::new("limits-boundary");
    let limits = WriteLimits::new(ENTRY * 4, ENTRY, 16);
    content_file(&root.join("overrides/mods/exact.jar"), ENTRY);
    let target = root.join("out.zip");

    write_with_limits(
        &CURSEFORGE,
        &manifest_of(vec![override_file("mods/exact.jar")]),
        PackPayload::from_root(&root),
        &target,
        limits,
    )
    .expect("恰好等于上限必须写得出来（否则两侧的边界就错开了）");
    let written = crate::addon::pack::PackArchive::open(
        &target,
        // 读回来时单条上限就是"恰好等于它"，证明写出来的东西读侧认。
        PackLimits {
            max_entry_bytes: ENTRY,
            ..PackLimits::default()
        },
    )
    .expect("刚写出来的包必须能被自己的读取层打开");
    assert!(written.has_entry("overrides/mods/exact.jar"));
}

#[test]
fn the_write_side_refuses_a_total_over_the_unified_total_limit() {
    // 每条都在单条上限之内（2 KiB），但只要 3 条就超总量（6 KiB + 清单 ≥ 6 KiB）——
    // 这条与"单条规则"是两件事：判据是**累计**。以前是 9 × 64 MiB = 576 MiB 真占用。
    let root = TempRoot::new("limits-total");
    let limits = WriteLimits::new(ENTRY * 3, ENTRY, 16);
    let mut files = Vec::new();
    for index in 0..3 {
        let relative = format!("mods/part-{index}.jar");
        content_file(&root.join("overrides").join(&relative), ENTRY);
        files.push(override_file(&relative));
    }
    let total = ENTRY * 3;
    assert!(
        ENTRY <= limits.entry,
        "每条都必须在单条上限之内（判据是累计，不是单条）"
    );
    assert!(total >= limits.total, "三条加起来必须真的超总量");
    let target = root.join("out.zip");

    let error = write_with_limits(
        &CURSEFORGE,
        &manifest_of(files),
        PackPayload::from_root(&root),
        &target,
        limits,
    )
    .expect_err("累计超总量必须拒");

    assert_eq!(error.kind(), PackErrorKind::Limits, "{error}");
    assert!(
        error.message().contains(&total.to_string()),
        "文案要给出在用的总量上限：{error}"
    );
    assert!(!target.exists());
}

#[test]
fn the_production_write_path_hands_the_shared_constants_to_the_preflight() {
    // p2-1（复审 r1）：夹具缩到 KiB 之后，"生产路径 + 真超限内容"那种顺带守住生产限额的
    // 证法没了，于是「`PackFormat::write` 到底把哪份限额交给 `write_with_limits`」
    // 没有任何测试能观测——把它换成 `WriteLimits::new(u64::MAX, u64::MAX, …)` 会全绿。
    // `the_production_limits_are_the_shared_constants` 只钉了 `production()` 的**返回值**，
    // 这条钉的是**调用点**（走生产的 `CURSEFORGE.write`，靠 `format::probe` 记录）。
    let root = TempRoot::new("limits-binding");
    write_file(&root.join("overrides/mods/jei.jar"), b"PK\x03\x04 jei");
    let target = root.join("out.zip");

    CURSEFORGE
        .write(
            &manifest_of(vec![override_file("mods/jei.jar")]),
            PackPayload::from_root(&root),
            &target,
        )
        .expect("正常尺寸必须写得出来");

    let recorded = crate::addon::pack::format::probe::last_limits()
        .expect("生产 write() 必须经过 write_with_limits");
    let production = WriteLimits::production();
    assert_eq!(recorded.total, production.total, "总量限额：生产路径传错了");
    assert_eq!(
        recorded.entry, production.entry,
        "单条目限额：生产路径传错了"
    );
    assert_eq!(
        recorded.entries, production.entries,
        "条目数限额：生产路径传错了"
    );
    // 再把生产值本身钉在统一常量上：两条合起来才等于"生产路径用的是读侧那三个数"。
    assert_eq!(recorded.total, PACK_MAX_UNCOMPRESSED_BYTES);
    assert_eq!(recorded.entry, PACK_MAX_ENTRY_BYTES);
    assert_eq!(recorded.entries, PACK_MAX_ENTRIES);
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
}
