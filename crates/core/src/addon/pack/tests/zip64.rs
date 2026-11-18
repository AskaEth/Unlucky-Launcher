//! 导出侧的体积闸：**拒写**过大包（research/18-p2 §8.6 / 表 B #17 的导出侧那条）。
//!
//! # 现在的判据是「与导入读取同一份常量」（M160）
//!
//! 以前判据是 `zip::ZIP64_BYTES_THR`（≈4 GiB），而导入读取只允许 512 MiB —— 于是存在
//! 「导得出来、导不回去」的包。M160 把写侧上限换成读侧那份常量（`pack::archive::limits`），
//! 本文件仍钉**同一件事**：闸门在碰内容之前就把包拒了。只是拒绝的理由从"要写 ZIP64"
//! 变成"超过统一上限，所以永远不会写出 ZIP64"。
//!
//! # 夹具只有几 KiB（M166）
//!
//! 判据是**声明尺寸**（`metadata().len()`），所以判据本身跟夹具真实大小无关。旧版本靠
//! `File::set_len` 造 4 GiB「稀疏文件」，以为"只改 EOF、不占磁盘"——**实测是错的**：
//!
//! ```text
//! PS> $fs=[IO.File]::Create("$env:TEMP\probe.bin"); $fs.SetLength(1GB); $fs.Close()
//! logical=1024.0MiB  freeDelta=1024.0MiB      # 可用空间真的掉了 1 GiB
//! ```
//!
//! 本机是 NTFS（Windows 11）。ext4 / APFS 上 `truncate`（= `set_len`）通常确实不分配块，
//! **但那是平台差异，不是可以依赖的性质**；而在 NTFS 上这两条用例的声明尺寸合计
//! `u32::MAX + 1`（≈4 GiB）+ `3 × 1.5 GiB`（4.5 GiB）= **8.5 GiB 真占用**。
//!
//! 现在改走 [`format::write_with_limits`]：limits 缩到 KiB 级，几百字节的**真**文件就触发
//! 同一条预检分支（`write::preflight` → `guard_with` / `guard_entry`），一条用例都不 `#[ignore]`、
//! 一条断言都没删。**GiB 级的数值判定**（>2 GiB 与 >4 GiB）搬到了 `format/write/tests.rs`：
//! 那两道闸吃的是 `u64`，本来就不需要落盘。
//!
//! # 「声明长度」必须自己造出来（M170）
//!
//! M166 的 `the_gate_runs_before_any_content_is_opened` 把源做成目录、量 `metadata().len()`，
//! 量到 0 就 `return`。而**本机 NTFS 上空目录的长度就是 0**（实测：空目录 0；条目塞进去之后
//! 才变成 4–16 KiB 量级，见 [`DIR_LENGTH_FILLERS`] 的注释；ext4 空目录直接报 4096）——所以
//! 那条"行为证明"在 Windows 上一条断言都没执行，只有 CI 的 ubuntu 那侧真跑。现在改成：先把
//! 目录填到长度**大于清单 JSON 自身**，拿**这个长度减一字节**当总量上限，并且前提不满足时
//! `assert!` 而不是 `return`——要么真判过东西，要么在这台机器上红。

use std::io::Read;
use std::path::Path;

use super::write_file;
use super::TempRoot;
use crate::addon::pack::curseforge::CURSEFORGE;
use crate::addon::pack::error::PackErrorKind;
use crate::addon::pack::format::{write_with_limits, PackFormat, PackPayload, WriteLimits};
use crate::addon::pack::manifest::{OverrideFile, PackManifest, PackOverrides, PackSide};

/// 小 limits：生产口径是 512 MiB 总量 / 64 MiB 单条 / 2 万条，这里缩到 KiB 级。
///
/// 判据只吃"声明尺寸 ≥ 上限"这个**关系**，所以缩小的只是夹具，不是分支。
const SMALL_TOTAL: u64 = 8 * 1024;
const SMALL_ENTRY: u64 = 2 * 1024;
const SMALL_ENTRIES: usize = 16;

/// 把目录填充到「`metadata().len()` 大到能当判据」用几个条目。
///
/// **这个数字不是阈值，是区间里挑的一个稳的点**：NTFS 的目录长度是索引分配的字节数，按 4 KiB
/// 一档往上加，**跟条目名的长度和条目数都有关**——本机实测（`std::fs::metadata`，同一台机器、
/// 同一个 `%TEMP%`）：
///
/// | 条目名 | 空 | 3 条 | 7 条 | 20 条 | 32 条 | 39 条 | 40 条 |
/// | --- | --- | --- | --- | --- | --- | --- | --- |
/// | `f0`…`f39`（短名） | 0 | 0 | 4096 | 4096 | 4096 | 4096 | 4096 |
/// | `filler-NN`（本用例的形状） | 0 | 4096 | 4096 | 8192 | **8192** | 12288 | 16384 |
///
/// 所以"第几个条目起非零"没有平台无关的答案，别在注释里钉死一个漂亮的数字：这里只钉
/// **32 个 `filler-NN` 条目**这个形状（本机 8192），其余文件系统（ext4 空目录 4096、tmpfs
/// 680 量级）由下面那条 `limit > 清单 JSON 长度` 的 `assert!` 现场把关——不满足就用例红。
const DIR_LENGTH_FILLERS: usize = 32;

/// 上面那三个数组成的限额（`WriteLimits::new` 是 `#[cfg(test)]` 的测试入口）。
fn small_limits() -> WriteLimits {
    WriteLimits::new(SMALL_TOTAL, SMALL_ENTRY, SMALL_ENTRIES)
}

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

/// 造一个真有 `length` 字节内容的覆盖文件（**不稀疏**，见模块文档）。
fn content_file(path: &Path, length: u64) {
    write_file(
        path,
        &vec![0_u8; usize::try_from(length).expect("夹具尺寸")],
    );
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
    // 单条就等于总量上限：预检在 `guard_with` 上拒，**不许读内容**。
    // （旧版这里是 `ZIP64_BYTES_THR + 1` 的 4 GiB 夹具，见模块文档。）
    content_file(&root.join("overrides/mods/huge.jar"), SMALL_TOTAL);
    let manifest = manifest_of(vec![override_file("mods/huge.jar")]);
    let target = root.join("out.zip");

    let error = write_with_limits(
        &CURSEFORGE,
        &manifest,
        PackPayload::from_root(&root),
        &target,
        small_limits(),
    )
    .expect_err("超统一上限必须拒写");

    assert_eq!(error.kind(), PackErrorKind::Limits, "{error}");
    // 文案要点名「我们永远不会写 ZIP64」这件事，而不是拿 ZIP64 阈值当判据。
    assert!(error.message().contains("ZIP64"), "{error}");
    // 报出来的必须是**在用的**那个上限（生产路径上它就是 PACK_MAX_UNCOMPRESSED_BYTES，
    // 接线由 `format/write/tests.rs` 的 `the_production_limits_are_the_shared_constants` 钉）。
    assert!(
        error.message().contains(&SMALL_TOTAL.to_string()),
        "要报出在用的总量上限：{error}"
    );
    assert!(
        error.hint().is_some_and(|hint| hint.contains("移出实例")),
        "要给「把大文件移出实例」这种可执行的提示：{error:?}"
    );
    assert!(!target.exists(), "失败后目标路径不许存在");
    // 临时文件也不许留。**但这条不是"一个字节都没读"的证明**（M170 更正）：失败分支自己会
    // `remove_file(&temp)`，所以它在任何失败路径上都成立，等于"确实返回了 Err"的同义反复。
    // 上面的 `kind()` 也**不区分闸的位置**：这条用例的源是**可读的** 8 KiB 真文件，闸被挪到
    // `File::open` 之后时，写循环的第二道闸 `Written::spend`（`write.rs` 每块之后
    // `guard_with`）照样返回 `Limits`——所以本用例在"闸挪位置"的变异下仍然绿（复审 r1 实测，
    // 我自己的变异 A 表里"其余 4 条仍绿"就是同一件事）。**顺序只由
    // `the_gate_runs_before_any_content_is_opened` 用"读不出来的源"证明**（含常驻反证）。
    assert_eq!(leftovers(&root, &["overrides"]), Vec::<String>::new());
}

#[test]
fn the_threshold_is_cumulative_not_per_file() {
    // 四条各自都**没有**越过单条目上限，加起来越过了总量上限：判据是**整包未压缩总量**。
    let root = TempRoot::new("zip64-cumulative");
    let each = SMALL_ENTRY;
    assert!(each * 4 >= SMALL_TOTAL, "四条加起来必须真的越过总量上限");
    let mut files = Vec::new();
    for index in 0..4 {
        let relative = format!("mods/part-{index}.jar");
        content_file(&root.join("overrides").join(&relative), each);
        files.push(override_file(&relative));
    }

    let target = root.join("out.zip");
    let error = write_with_limits(
        &CURSEFORGE,
        &manifest_of(files),
        PackPayload::from_root(&root),
        &target,
        small_limits(),
    )
    .expect_err("累计超上限同样必须拒写");

    assert_eq!(error.kind(), PackErrorKind::Limits, "{error}");
    assert!(error.message().contains("ZIP64"), "{error}");
    assert!(!target.exists());
    assert_eq!(leftovers(&root, &["overrides"]), Vec::<String>::new());
}

#[test]
fn a_single_entry_over_the_entry_limit_is_refused_before_a_single_byte_is_read() {
    // **单条目**那道闸（`guard_entry`）：总量还远在上限之下，只有一条越界。
    // 这条顶替了旧版 `#[ignore]` 的「2.5 GB 单文件落在 ZIP64 过拒窗口」端到端用例——
    // M160 之后那个窗口按设计不再存在，留下的是"单条越界就拒、且拒在读内容之前"。
    // 2.5 GB 那个**数值**的判定搬到 `format/write/tests.rs`（那里不需要落盘）。
    let root = TempRoot::new("zip64-entry");
    content_file(&root.join("overrides/mods/big.jar"), SMALL_ENTRY + 1);
    let manifest = manifest_of(vec![override_file("mods/big.jar")]);
    let target = root.join("out.zip");

    let error = write_with_limits(
        &CURSEFORGE,
        &manifest,
        PackPayload::from_root(&root),
        &target,
        small_limits(),
    )
    .expect_err("超过单条目上限必须拒");

    assert_eq!(error.kind(), PackErrorKind::Limits, "{error}");
    assert!(
        error.message().contains(&SMALL_ENTRY.to_string()),
        "要报出在用的单条目上限：{error}"
    );
    assert!(!target.exists(), "预检失败时目标路径不许出现");
    assert_eq!(leftovers(&root, &["overrides"]), Vec::<String>::new());
}

#[test]
fn the_gate_runs_before_any_content_is_opened() {
    // **行为**证明（复审 r1 的观察 2）：源做成**目录**——`metadata()` 拿得到长度，把它当内容
    // 读却读不出来（Windows NTFS：`File::open` 直接"拒绝访问"；Linux：`open` 成功但 `read`
    // 报 EISDIR）。于是闸若被挪到"读内容"那一段，这里拿到的会是 `pack_io` 而不是
    // `pack_limits`——比只看"临时文件没被创建"硬，因为后者在任何失败路径上都成立。
    //
    // 但"硬"的前提是**真有声明长度可判**：见下面的两块 `assert!`（M170）。
    let root = TempRoot::new("zip64-before-read");
    let directory = root.join("overrides/mods/not-a-file");
    std::fs::create_dir_all(&directory).expect("建目录");
    // 长度得**自己造**：NTFS 上空目录报 0（本机实测：空 0；塞条目之后按 4 KiB 一档
    // 往上跳，`filler-NN` 形状 3 条 → 4096、32 条 → 8192，详见 `DIR_LENGTH_FILLERS`），
    // 直接量空目录会得到 0 —— r2 版本就是在这里 `return` 的，于是 Windows 上一条断言都没跑。
    for index in 0..DIR_LENGTH_FILLERS {
        write_file(&directory.join(format!("filler-{index:02}")), b"x");
    }
    let declared = std::fs::metadata(&directory)
        .expect("stat 目录（这一步必须成功，否则区分不了两道闸）")
        .len();
    let manifest = manifest_of(vec![override_file("mods/not-a-file")]);
    let json_len = CURSEFORGE
        .render(&manifest)
        .expect("渲染清单（只是为了量它多长）")
        .len() as u64;
    // 上限 = 声明长度减一字节（`guard_with` 是 `>=`，声明尺寸自己就越线）。用
    // `saturating_sub` 而不是 `-`：目录长度报 0 时要走到下一条 `assert!` 的清楚信息，
    // 而不是一个整数下溢的 panic。
    let limit = declared.saturating_sub(1);
    // 前提①（r1 复审建议的收紧）：上限必须**大于清单 JSON 自身长度**。`total` 是
    // `清单 + 声明尺寸`，若上限比清单还小，清单自己就能把 `total` 顶过线——用例照样绿，
    // 却悄悄失去对「总量闸漏算了覆盖条目的声明尺寸」那类洞的判别力（复审的变异 E）。
    // 本用例这份 CurseForge 清单渲染出来实测 **220 字节**（复审用另一份清单量到约 165，差别
    // 来自清单字段）；ext4（4095）、tmpfs（679）、本机 NTFS（8191）都远在它之上。对 32 个
    // 条目报不到 220 字节的文件系统会让这条用例红——那是对的，它在那里证明不了声明尺寸承重
    // （把 `DIR_LENGTH_FILLERS` 临时改成 0 就能看到这句 assert 的话：`声明长度 0 − 1 = 0`）。
    assert!(
        limit > json_len,
        "前提：总量上限（目录声明长度 {declared} − 1 = {limit}）必须大于清单 JSON 自身长度\
         （{json_len}），否则拒写可能只是清单越线，声明尺寸没被证明承重（**不许静默 return**）"
    );

    // 前提②：这条源**确实**读不出来，否则 `pack_limits` / `pack_io` 区分不了闸的位置。
    // 走的就是写循环那一对调用（`File::open` → `read`），不满足照样 `assert!` 而不是跳过。
    let consumed = std::fs::File::open(&directory).and_then(|mut file| {
        let mut byte = [0_u8; 1];
        file.read(&mut byte).map(|_| ())
    });
    assert!(
        consumed.is_err(),
        "前提：目录源必须读不出来，否则拿到 pack_limits 也说明不了闸在内容之前：{consumed:?}"
    );

    // 单条目上限给 `u64::MAX`：不让 `guard_entry` 当替身——如果总量闸漏算了覆盖条目的声明
    // 尺寸，这里就不会拿到 `pack_limits`，而是一路走到读内容上拿到 `pack_io`（用例变红）。
    let target = root.join("out.zip");

    let error = write_with_limits(
        &CURSEFORGE,
        &manifest,
        PackPayload::from_root(&root),
        &target,
        WriteLimits::new(limit, u64::MAX, SMALL_ENTRIES),
    )
    .expect_err("声明尺寸超限必须拒");

    assert_eq!(
        error.kind(),
        PackErrorKind::Limits,
        "闸必须在打开内容**之前**：拿到 pack_io 说明它被挪到读之后了：{error}"
    );
    assert!(
        error.message().contains("总量") && error.message().contains(&limit.to_string()),
        "拒写理由必须是**总量闸**、且报出在用的上限（{limit}）：{error}"
    );
    assert!(
        !error.message().contains("读覆盖文件"),
        "文案是「读覆盖文件失败」的口径 ⇒ 闸已经在碰内容了：{error}"
    );

    // # 反证：同一个夹具，闸一放行就必定死在"读内容"上
    //
    // 这一步是"上面那个 `kind` 断言不是同义反复"的证据：把限额放到远大于声明尺寸，同一个
    // 目录源会在 `File::open` / `read` 上炸成 `pack_io`。于是上面拿到 `pack_limits` 只有
    // 一种解释——闸在碰内容**之前**就拒了。（而 `!target.exists()` 与"目录里没 `.part`"
    // 这两条：失败分支会 `remove_file(temp)`、只有成功分支才 `rename`，所以它们在任何失败
    // 路径上都成立，属于**结构侧检**、不是判据。r2 曾把后者当成"一个字节都没读"的证明，
    // 那才是同义反复。）
    let error = write_with_limits(
        &CURSEFORGE,
        &manifest_of(vec![override_file("mods/not-a-file")]),
        PackPayload::from_root(&root),
        &root.join("out-loose.zip"),
        WriteLimits::new(u64::MAX, u64::MAX, SMALL_ENTRIES),
    )
    .expect_err("目录源读不出来：闸放行就只能死在读内容上");
    assert_eq!(
        error.kind(),
        PackErrorKind::Io,
        "反证不成立：闸放行后没有死在读内容上，那上面那个 kind 断言就区分不了闸的位置：{error}"
    );
    assert!(error.message().contains("读覆盖文件"), "{error}");

    // 结构侧检（**不是判据**）：两次调用到这里都没留下 `out.zip` / `out-loose.zip` / `.part`。
    // 这两条在任何失败路径上都成立（失败分支自己 `remove_file(temp)`，只有成功分支才
    // `rename`），所以它们只值"确实返回了 Err"的同义反复——上面那条反证把真正有牙的判据
    // （`kind()` + 文案）与它们区分开。
    assert!(!target.exists(), "预检失败时目标路径不许出现");
    assert_eq!(leftovers(&root, &["overrides"]), Vec::<String>::new());
}

#[test]
fn an_export_under_the_threshold_still_writes_a_normal_pack() {
    // 对照：不超阈值时预检不许误伤（否则这条护栏就变成"导出全挂"）。
    // 走**生产口径**（`CURSEFORGE.write` → `WriteLimits::production()`）：小 limits 那几条
    // 证明的是分支，这条证明的是生产接线不会把正常包拒掉。
    let root = TempRoot::new("zip64-under");
    let content = b"a normal sized override file";
    write_file(&root.join("overrides/mods/jei.jar"), content);
    let manifest = manifest_of(vec![override_file("mods/jei.jar")]);
    let target = root.join("out.zip");

    CURSEFORGE
        .write(&manifest, PackPayload::from_root(&root), &target)
        .expect("正常尺寸必须写得出来");
    assert!(target.exists());
    let written = crate::addon::pack::PackArchive::open_default(&target).expect("打开导出的包");
    assert!(written.has_entry("overrides/mods/jei.jar"));
}
