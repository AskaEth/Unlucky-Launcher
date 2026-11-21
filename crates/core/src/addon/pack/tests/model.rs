//! 模型级辅助与读取预算：r1 p2-2（跨侧重名）与 p3-4（批量读取的累计限额）。

use super::fixtures;
use crate::addon::pack::archive::{PackArchive, ReadBudget};
use crate::addon::pack::error::PackErrorKind;
use crate::addon::pack::manifest::{OverrideFile, PackManifest, PackOverrides, PackSide};

fn file(path: &str, sha1: &str, size: u64) -> OverrideFile {
    OverrideFile {
        path: path.to_string(),
        sha1: sha1.to_string(),
        size,
    }
}

#[test]
fn duplicate_override_paths_counts_across_sides_not_per_side() {
    // r1 p2-2：mrpack 的真实冲突是 `overrides`(Both) 与 `client-overrides`(Client) 撞同一路径，
    // 按 `(side, path)` 计数时两侧键天然不同 ⇒ 永远返回空表。
    let mut both = PackOverrides::new("overrides", PackSide::Both);
    both.files.push(file("config/x.toml", "a", 1));
    both.files.push(file("config/only-both.toml", "b", 1));
    let mut client = PackOverrides::new("client-overrides", PackSide::Client);
    client.files.push(file("config/x.toml", "c", 2));
    let mut server = PackOverrides::new("server-overrides", PackSide::Server);
    server.files.push(file("config/only-server.toml", "d", 3));
    let manifest = PackManifest {
        overrides: vec![both, client, server],
        ..PackManifest::default()
    };

    assert_eq!(
        manifest.duplicate_override_paths(),
        vec![(
            "config/x.toml".to_string(),
            vec![PackSide::Both, PackSide::Client],
            2
        )],
        "只报真正撞车的路径，并给出涉及的侧与条目份数"
    );
}

#[test]
fn duplicate_override_paths_still_reports_two_groups_on_the_same_side() {
    // r2 p2-2：上一版按侧去重，把这半能力换掉了 —— 两个 Both 组撞同一路径实测返回 `[]`。
    // 现在判据是「侧数 > 1 || 条份数 > 侧数」，两种冲突同时报得出来。
    let mut first = PackOverrides::new("overrides", PackSide::Both);
    first.files.push(file("config/x.toml", "a", 1));
    let mut second = PackOverrides::new("extra-overrides", PackSide::Both);
    second.files.push(file("config/x.toml", "b", 2));
    let manifest = PackManifest {
        overrides: vec![first, second],
        ..PackManifest::default()
    };
    assert_eq!(
        manifest.duplicate_override_paths(),
        vec![("config/x.toml".to_string(), vec![PackSide::Both], 2)],
        "同侧两个组撞同一路径必须报（侧集合只有一个，份数是 2）"
    );
}

#[test]
fn duplicate_override_paths_reports_the_same_path_twice_inside_one_group() {
    // 同侧撞车的另一种形态：一个组里同一路径列了两次（模型被合并出来时会出现）。
    let mut group = PackOverrides::new("overrides", PackSide::Both);
    group.files.push(file("config/x.toml", "a", 1));
    group.files.push(file("config/x.toml", "b", 2));
    let manifest = PackManifest {
        overrides: vec![group],
        ..PackManifest::default()
    };
    assert_eq!(
        manifest.duplicate_override_paths(),
        vec![("config/x.toml".to_string(), vec![PackSide::Both], 2)]
    );
}

#[test]
fn duplicate_override_paths_is_empty_when_nothing_collides() {
    let mut both = PackOverrides::new("overrides", PackSide::Both);
    both.files.push(file("config/a.toml", "a", 1));
    let mut client = PackOverrides::new("client-overrides", PackSide::Client);
    client.files.push(file("config/b.toml", "b", 1));
    let manifest = PackManifest {
        overrides: vec![both, client],
        ..PackManifest::default()
    };
    assert!(manifest.duplicate_override_paths().is_empty());
}

#[test]
fn a_batch_scan_spends_one_cumulative_budget() {
    // r1 p3-4：打开阶段只能看声明大小；批量读取（override_files）必须按**实际字节**累计扣减，
    // 否则"很多条各自合规"累起来就能绕过 `max_uncompressed_bytes`（CPU/时间 DoS）。
    let fixture = fixtures::modrinth("budget");
    let pack = PackArchive::open_default(&fixture.zip).expect("打开 mrpack 夹具");

    let generous = ReadBudget::new(1024 * 1024);
    let files = pack
        .override_files_with("overrides", &generous)
        .expect("预算够时必须成功");
    assert_eq!(files.len(), 2);

    // 预算只有 4 字节：第一个文件（config/known.toml，15 字节）就该把预算吃穿。
    let tight = ReadBudget::new(4);
    let error = pack
        .override_files_with("overrides", &tight)
        .expect_err("累计预算必须生效");
    assert_eq!(error.kind(), PackErrorKind::Limits, "{error}");
    assert_eq!(tight.remaining(), 0, "失败后预算不该被退回");
}
