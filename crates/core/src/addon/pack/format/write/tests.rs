//! `format/write.rs` 的单测：**从 `write.rs` 拆出来守 400 行硬线**。
//!
//! 拆分口径与仓库其它拆分一致：**纯搬移**，断言与注释一行不改（只在外面加一层 `mod tests;`）。
//! 这些用例钉的是「闸在碰内容之前」「边界在 `>=` 上」「写循环不许重复累计」三件事。

use super::*;
use crate::addon::pack::manifest::{OverrideFile, PackOverrides, PackSide};
use crate::addon::temp::TempRoot;

#[test]
fn the_threshold_is_reached_not_exceeded() {
    // `>=` 而不是 `>`：恰好等于阈值就已经需要 ZIP64 了。
    assert!(guard_with(99, 100).is_ok());
    let error = guard_with(100, 100).expect_err("等于阈值必须拒");
    assert_eq!(
        error.kind(),
        super::super::super::error::PackErrorKind::Limits
    );
    assert!(error.message().contains("100"), "{error}");
    assert!(guard_with(101, 100).is_err());
}

#[test]
fn the_second_gate_counts_from_the_manifest_length_not_the_preflight_total() {
    // P2-1 的窗口：真实总量在 [上限一半, 上限) 之间的包**不许**被拒。这一条不真写
    // 几百 MiB（那要几十秒 deflate 与一大片内存/页面文件），而是直接把窗口里的字节数
    // 喂给第二道闸的计数器——它的起点是清单长度，所以只有"把预检的全量 total 当基数"
    // 那种写法才会在这里红（红法是 `spend` 返回 Limits）。
    let limit = WriteLimits::production().total;
    let inside = limit / 2 + limit / 4;
    let mut written = Written::new(28, limit);
    written.spend(inside).expect("没到统一上限，第二道闸不许拒");
    assert!(inside > limit / 2, "样本要落在过拒窗口里");

    // 对照：撞上上限必须拒，且正好在上限处（不是超一点点才拒）。
    let mut written = Written::new(28, limit);
    let error = written.spend(limit).expect_err("撞上限必须拒");
    assert_eq!(
        error.kind(),
        super::super::super::error::PackErrorKind::Limits
    );
}

#[test]
fn the_container_does_not_double_count_the_preflight_total() {
    // 容器层的窗口回归钉（P2-1），**用几十字节的 limit 把窗口缩小到可测尺寸**：
    // `json` 30 字节 + 覆盖文件 40 字节 = 声明总量 70。
    //  - 正确实现：第二道闸从 30 起算，写到 70 也不到 100 ⇒ 导出成功；
    //  - P2-1 的写法（把预检返回的全量 70 当基数）：写循环再累加 40 ⇒ 110 ≥ 100 ⇒ 误拒。
    // 真造 2.5 GB 才能暴露同一个 bug，本机资源纪律下不跑：GiB 级的**数值判定**由
    // `the_gib_scale_boundaries_decide_without_any_fixture` 用纯算术钉住。测试全部用
    // `TempRoot`（drop 即删、panic 也删）：旧版自己拼 `%TEMP%` 路径、只在**正常收尾**时删，
    // 断言一红就留下 `ull-double-count-*`（M160 r2 的泄漏口径）。
    let root = TempRoot::new("double-count");
    std::fs::create_dir_all(root.join("overrides/mods")).expect("建目录");
    std::fs::write(root.join("overrides/mods/a.bin"), [0_u8; 40]).expect("写覆盖文件");
    // 清单很短，limit 取"声明总量 + 20 字节"的余量：正确实现只写到 70，重复累计会写到 110。
    let json = "{\"name\":\"0123456789012345\"}";
    let declared = json.len() as u64 + 40;
    let limit = declared + 20;
    let mut manifest = PackManifest::new("double count");
    manifest.overrides.push(PackOverrides {
        dir: "overrides".to_string(),
        side: PackSide::Both,
        files: vec![OverrideFile {
            path: "mods/a.bin".to_string(),
            sha1: String::new(),
            size: 40,
        }],
    });
    let target = root.join("out.zip");

    container(
        "curseforge",
        "manifest.json",
        json,
        &manifest,
        PackPayload::from_root(&root),
        &target,
        WriteLimits::new(limit, 1024, 16),
    )
    .expect("声明总量没到 limit：写循环不许把预检的 total 重复累计");

    // 对照：limit 降到声明总量本身 ⇒ 预检就该拒，且临时文件都没建。
    let refused = root.join("refused.zip");
    let error = container(
        "curseforge",
        "manifest.json",
        json,
        &manifest,
        PackPayload::from_root(&root),
        &refused,
        WriteLimits::new(declared, 1024, 16),
    )
    .expect_err("声明总量达到 limit 必须拒");
    assert_eq!(
        error.kind(),
        super::super::super::error::PackErrorKind::Limits
    );
    assert!(!refused.exists(), "预检失败时目标路径不许出现");
}

#[test]
fn the_preflight_measures_declared_sizes_not_content() {
    // 预检必须只量 `metadata().len()`。夹具刻意**只有几 KiB**（M166）：把"没读内容"钉住的是
    // **结构**而不是字节数——`container` 的顺序是 预检 → `File::create(temp)` → 打开源文件读，
    // 所以"临时文件根本没被创建"等价于"一个字节的内容都没被打开过"。
    // 旧版这里造一个 3 GiB 的"稀疏"文件，实测在 NTFS 上真占 3 GiB（见 `zip64.rs` 模块文档）。
    const LIMIT: u64 = 8 * 1024;
    const HUGE: u64 = 6 * 1024;
    let root = TempRoot::new("preflight");
    std::fs::create_dir_all(root.join("overrides/mods")).expect("建目录");
    std::fs::write(root.join("overrides/mods/small.jar"), b"0123456789").expect("写小文件");
    std::fs::write(
        root.join("overrides/mods/huge.jar"),
        vec![0_u8; HUGE as usize],
    )
    .expect("写大一点的覆盖文件");

    let mut manifest = PackManifest::new("preflight");
    manifest.overrides.push(PackOverrides {
        dir: "overrides".to_string(),
        side: PackSide::Both,
        files: vec![
            OverrideFile {
                path: "mods/small.jar".to_string(),
                sha1: String::new(),
                size: 10,
            },
            OverrideFile {
                path: "mods/huge.jar".to_string(),
                sha1: String::new(),
                size: 0,
            },
        ],
    });

    let total = preflight(
        7,
        &manifest,
        PackPayload::from_root(&root),
        // 总量闸放到 8 KiB：声明总量 7 + 10 + 6144 仍在它之下，预检不许误伤。
        WriteLimits::new(LIMIT, LIMIT, 16),
    )
    .expect("声明总量仍在闸下，预检不许误伤");
    assert_eq!(total, 7 + 10 + HUGE, "清单 + 每个覆盖文件的声明尺寸");
}

#[test]
fn the_production_limits_are_the_shared_constants() {
    // 写侧口径的**接线**证明：生产路径拿到的就是读侧那三个常量，不是另抄一份。
    // 这条是纯算术。旧版是靠"造 64 MiB 文件、看报错文案里出现的数字"来证的——
    // 那既要真占 64 MiB，也只覆盖到文案碰巧提到的字段。
    let production = WriteLimits::production();
    assert_eq!(
        production.total,
        crate::addon::pack::archive::limits::PACK_MAX_UNCOMPRESSED_BYTES
    );
    assert_eq!(
        production.entry,
        crate::addon::pack::archive::limits::PACK_MAX_ENTRY_BYTES
    );
    assert_eq!(
        production.entries,
        crate::addon::pack::archive::limits::PACK_MAX_ENTRIES
    );
}

#[test]
fn the_gib_scale_boundaries_decide_without_any_fixture() {
    // M166：**>2 GiB 与 >4 GiB 两条边界**在这里钉，不需要落盘——两道闸吃的都是 `u64`。
    // 以前它们只有"造 4 GiB / 2.5 GB 夹具"这一条证法，而那在本机 NTFS 上是真占 4 GiB。
    use crate::addon::pack::archive::limits::{PACK_MAX_UNCOMPRESSED_BYTES, PACK_ZIP64_BYTES_THR};
    const GIB: u64 = 1024 * 1024 * 1024;

    // ① >4 GiB 那条：撞上 ZIP64 阈值就拒，差一字节就放行（`>=` 不是 `>`）。
    let zip64 = PACK_ZIP64_BYTES_THR;
    assert_eq!(zip64, u32::MAX as u64, "ZIP64 阈值就是 u32::MAX ≈ 4 GiB");
    assert!(guard_with(zip64 - 1, zip64).is_ok(), "差一字节必须放行");
    let error = guard_with(zip64, zip64).expect_err("撞上 ZIP64 阈值必须拒");
    assert!(error.message().contains("ZIP64"), "{error}");

    // ② >2 GiB 那条：M152 时代那个 2.5 GB 的样本，现在按**统一上限**（512 MiB）拒。
    let old_sample: u64 = 2_500_000_000;
    assert!(old_sample > 2 * GIB, "样本必须真的在 2 GiB 之上");
    assert!(
        old_sample < zip64,
        "样本又必须在 ZIP64 阈值之下：拒绝理由是统一上限"
    );
    let error =
        guard_with(old_sample, PACK_MAX_UNCOMPRESSED_BYTES).expect_err("统一上限必须拒 2.5 GB");
    assert!(
        error
            .message()
            .contains(&PACK_MAX_UNCOMPRESSED_BYTES.to_string()),
        "文案要给出统一上限那个数：{error}"
    );

    // ③ 第二道闸（`Written`）在同一批数值上也不许误拒，越界才拒。
    let mut written = Written::new(0, zip64);
    written
        .spend(old_sample)
        .expect("2.5 GB < ZIP64 阈值，第二道闸不许拒");
    written.spend(GIB).expect("3.5 GB 仍在上限之下");
    assert!(written.spend(GIB).is_err(), "越过 4 GiB 必须拒");
}
