//! 备份（zip 导出）那几条判据——从 `write/tests.rs` 拆出：那边加完 11 条用例之后越过 400 行硬线
//! （拆法与仓库其它 `-p1/-p2` / `tail*` 一致：**纯搬移，判据一条不改**）。
//!
//! | 用例 | 抓得住 |
//! |---|---|
//! | [`backup_writes_a_timestamped_zip`] | 时戳命名、条目与内容能读回来、空目录条目也在 |
//! | [`backup_never_overwrites_an_earlier_export`] | 同一时刻两份备份 ⇒ `-2` 后缀，前一份不变 |
//! | [`backup_limits_are_checked_before_any_byte_is_written`] | 超限 ⇒ `TooLarge` 且落点目录一个文件都没有 |
//! | [`backup_refuses_a_relative_or_empty_dest`] | 空 / 相对的落点 ⇒ `Boundary`（相对路径会落到进程当前目录） |

use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use super::super::backup::{backup_world_at, stamp, Limits};
use super::super::*;
use super::{fingerprint, run_dir, world};

/// 2024-05-01 10:00:00 UTC（写死的时刻：时戳命名要能逐字断言）。
fn fixed_at() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_millis(1_714_557_600_000)
}

/// 时戳命名、条目与内容能读回来、目录条目也在。
#[test]
fn backup_writes_a_timestamped_zip() {
    let root = run_dir("m247-backup");
    let dir = world(&root, "Alpha");
    std::fs::create_dir_all(dir.join("empty-dir")).expect("建空目录");
    let exports = root.join("exports");

    let backup =
        backup_world_at(&root, "Alpha", &exports, fixed_at(), Limits::production()).expect("备份");
    assert_eq!(
        backup.path.file_name().unwrap().to_string_lossy(),
        "Alpha-20240501-100000.zip",
        "落点名要带 UTC 时戳"
    );
    assert_eq!(backup.skipped, 0);
    assert!(backup.bytes > 0);
    assert_eq!(backup.sourced, 8, "level.dat + region 共 8 字节");
    assert_eq!(
        backup.entries, 4,
        "region/ 与 empty-dir/ 两个目录条目 + 两个文件条目"
    );

    let file = std::fs::File::open(&backup.path).expect("开备份");
    let mut archive = zip::ZipArchive::new(file).expect("读 zip");
    let names: Vec<String> = (0..archive.len())
        .map(|index| {
            archive
                .by_index(index)
                .expect("读条目")
                .name()
                .expect("条目名")
                .to_string()
        })
        .collect();
    assert!(names.contains(&"level.dat".to_string()), "条目：{names:?}");
    assert!(
        names.contains(&"region/r.0.0.mca".to_string()),
        "条目：{names:?}"
    );
    assert!(
        names.contains(&"empty-dir/".to_string()),
        "空目录丢了：{names:?}"
    );

    let mut body = Vec::new();
    archive
        .by_name("level.dat")
        .expect("取 level.dat")
        .read_to_end(&mut body)
        .expect("读 level.dat");
    assert_eq!(body, b"nbt", "包里的内容与盘上不一致");

    // 时戳换算本身（同一张固定时刻表）。
    assert_eq!(stamp(fixed_at()), "20240501-100000");
}

/// 同一时刻两份备份 ⇒ `-2` 后缀，前一份内容不变。
#[test]
fn backup_never_overwrites_an_earlier_export() {
    let root = run_dir("m247-backup-twice");
    world(&root, "Alpha");
    let exports = root.join("exports");
    let first = backup_world_at(&root, "Alpha", &exports, fixed_at(), Limits::production())
        .expect("第一次");
    let body = std::fs::read(&first.path).expect("读第一份");
    let second = backup_world_at(&root, "Alpha", &exports, fixed_at(), Limits::production())
        .expect("第二次");

    assert_ne!(first.path, second.path, "第二份覆盖了第一份");
    assert_eq!(
        second.path.file_name().unwrap().to_string_lossy(),
        "Alpha-20240501-100000-2.zip"
    );
    assert_eq!(
        std::fs::read(&first.path).expect("读第一份"),
        body,
        "第一份被改了"
    );
}

/// 超限 ⇒ `TooLarge` 且落点目录**一个文件都没有**（闸在第一个字节之前）。
#[test]
fn backup_limits_are_checked_before_any_byte_is_written() {
    let root = run_dir("m247-backup-limit");
    world(&root, "Alpha");
    let exports = root.join("exports");

    // 总量闸：世界有 8 字节，上限给 4。
    let error = backup_world_at(&root, "Alpha", &exports, fixed_at(), Limits::new(4, 100))
        .expect_err("超限居然备份成功了");
    assert_eq!(error.kind, SavesWriteKind::TooLarge, "{}", error.detail);
    assert!(!exports.exists(), "超限时连落点目录都不该建");

    // 条目闸：4 个条目 + 上限给 2。
    let error = backup_world_at(
        &root,
        "Alpha",
        &exports,
        fixed_at(),
        Limits::new(1 << 30, 2),
    )
    .expect_err("条目超限居然备份成功了");
    assert_eq!(error.kind, SavesWriteKind::TooLarge);
    assert!(
        !exports.exists()
            || std::fs::read_dir(&exports)
                .expect("读落点")
                .next()
                .is_none(),
        "超限时写出了文件"
    );
}

/// 落点目录必须是绝对路径：相对路径会落到进程当前目录（一次实测里 `dest = ""` 把 zip
/// 写进了 crate 根），所以「空 / 相对」直接拒。
#[test]
fn backup_refuses_a_relative_or_empty_dest() {
    let root = run_dir("m247-backup-dest");
    world(&root, "Alpha");
    let before = fingerprint(&root);
    for dest in [
        PathBuf::new(),
        PathBuf::from("exports"),
        PathBuf::from("./exports"),
    ] {
        let error = backup_world(&root, "Alpha", &dest).expect_err("相对落点居然备份成功了");
        assert_eq!(error.kind, SavesWriteKind::Boundary, "{}", error.detail);
    }
    assert_eq!(fingerprint(&root), before, "被拒的备份动了磁盘");
}
