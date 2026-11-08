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
    // 真造 2.5 GB 才能暴露同一个 bug，本机资源纪律下不跑（那条留成 `zip64.rs` 里的
    // `#[ignore]` 用例）。
    // 临时根用 `TempRoot`（drop 即删、panic 也删）：这两个用例以前自己拼 `%TEMP%` 路径、
    // 只在**正常收尾**时删，断言一红就留下 `ull-double-count-*`（M160 r2 的泄漏口径）。
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
    // 预检必须只量 `metadata().len()`：这一条用"小文件 + 3 GiB 稀疏文件"钉住，
    // 真读内容的话要么慢要么直接吃 3 GiB 内存。
    let root = TempRoot::new("preflight");
    std::fs::create_dir_all(root.join("overrides/mods")).expect("建目录");
    std::fs::write(root.join("overrides/mods/small.jar"), b"0123456789").expect("写小文件");
    let huge = root.join("overrides/mods/huge.jar");
    let file = std::fs::File::create(&huge).expect("建稀疏文件");
    file.set_len(3 * 1024 * 1024 * 1024).expect("拉长（稀疏）");
    drop(file);

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
        // 本条只关心"预检不读内容"，所以三道闸都放到最宽（生产口径见 `tests/limits.rs`）。
        WriteLimits::new(
            crate::addon::pack::archive::limits::PACK_ZIP64_BYTES_THR,
            crate::addon::pack::archive::limits::PACK_ZIP64_BYTES_THR,
            crate::addon::pack::archive::limits::PACK_MAX_ENTRIES,
        ),
    )
    .expect("3 GiB 总量仍在最宽闸之下，预检不许误伤");
    assert_eq!(
        total,
        7 + 10 + 3 * 1024 * 1024 * 1024,
        "清单 + 每个覆盖文件的声明尺寸"
    );
}
