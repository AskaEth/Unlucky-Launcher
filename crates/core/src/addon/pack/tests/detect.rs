//! 探测优先级：顺序写死在注册表里，用例把"谁赢"和"为什么赢"都钉住。
//!
//! 判据分三层，缺一层都可能被"看起来对"糊过去：
//! 1. 内置顺序本身（[`PackRegistry::ids`]）；
//! 2. 同一份包命中多格式时选谁（含 `also_matched` 记录的其它命中）；
//! 3. **换一下注册顺序，赢家跟着换**——这才证明优先级来自注册表，而不是代码里散落的 if。

use super::{fixtures, temp_root, write_zip};
use crate::addon::pack::archive::PackArchive;
use crate::addon::pack::curseforge::CURSEFORGE;
use crate::addon::pack::error::PackErrorKind;
use crate::addon::pack::format::{PackFormat, PackRegistry};
use crate::addon::pack::modrinth::MODRINTH;
use crate::addon::pack::multimc::MULTIMC;

const CF_MANIFEST: &[u8] = br#"{"manifestType":"minecraftModpack","manifestVersion":1,
  "name":"hybrid pack","version":"1.0","minecraft":{"version":"1.20.1"},"files":[]}"#;

const MR_INDEX: &[u8] = br#"{"game":"minecraft","formatVersion":1,"versionId":"1.0",
  "name":"hybrid pack","files":[],"dependencies":{"minecraft":"1.20.1"}}"#;

const MMC_PACK: &[u8] =
    br#"{"formatVersion":1,"components":[{"uid":"net.minecraft","version":"1.20.1"}]}"#;

/// 同时带三种身份标记的"不标准包"。
fn hybrid_pack(tag: &str) -> std::path::PathBuf {
    let target = temp_root(tag).join("hybrid.zip");
    write_zip(
        &target,
        &[
            ("manifest.json", CF_MANIFEST),
            ("modrinth.index.json", MR_INDEX),
            ("mmc-pack.json", MMC_PACK),
            ("overrides/config/x.toml", &b"x = 1\n"[..]),
        ],
    );
    target
}

#[test]
fn builtin_priority_is_pinned() {
    assert_eq!(
        PackRegistry::builtin().ids(),
        vec!["curseforge", "modrinth", "multimc"],
        "探测顺序是决策，不是实现细节：改了要连着报告与测试一起改"
    );
}

#[test]
fn curseforge_wins_when_a_pack_carries_every_marker() {
    let pack = PackArchive::open_default(hybrid_pack("detect-hybrid")).expect("打开混合包");
    let matched = PackRegistry::builtin().detect(&pack).expect("要能选出格式");
    assert_eq!(matched.format().id(), "curseforge");
    assert_eq!(
        matched.also_matched().to_vec(),
        vec!["modrinth", "multimc"],
        "其余命中必须可见（调用方据此提示「这包不标准」）"
    );
    assert!(matched.is_ambiguous());
}

#[test]
fn a_plain_manifest_json_does_not_claim_curseforge() {
    // `manifest.json` 这名字太泛：形状不符就不能认领，否则 mrpack 会被抢走。
    let target = temp_root("detect-plain").join("plain.zip");
    write_zip(
        &target,
        &[
            ("manifest.json", &br#"{"hello":"world"}"#[..]),
            ("modrinth.index.json", MR_INDEX),
        ],
    );
    let pack = PackArchive::open_default(&target).expect("打开包");
    let matched = PackRegistry::builtin().detect(&pack).expect("要能选出格式");
    assert_eq!(matched.format().id(), "modrinth");
    assert!(matched.also_matched().is_empty(), "CF 不该被「泛名」骗到");
}

#[test]
fn registration_order_decides_the_winner() {
    // 同一份包同时有 CF 与 mrpack 标记：只换注册顺序，赢家就该换。
    let target = temp_root("detect-order").join("both.zip");
    write_zip(
        &target,
        &[
            ("manifest.json", CF_MANIFEST),
            ("modrinth.index.json", MR_INDEX),
        ],
    );
    let pack = PackArchive::open_default(&target).expect("打开双标记包");

    let builtin = PackRegistry::builtin()
        .detect(&pack)
        .expect("默认顺序要能选");
    assert_eq!(builtin.format().id(), "curseforge");
    assert_eq!(builtin.also_matched().to_vec(), vec!["modrinth"]);

    let reversed = PackRegistry::empty().with(&MODRINTH).with(&CURSEFORGE);
    let matched = reversed.detect(&pack).expect("反序注册也要能选");
    assert_eq!(
        matched.format().id(),
        "modrinth",
        "反序注册后 Modrinth 排第一，就该它赢"
    );
    assert_eq!(matched.also_matched().to_vec(), vec!["curseforge"]);
}

#[test]
fn unknown_package_is_rejected_with_the_list_of_tried_formats() {
    let target = temp_root("detect-unknown").join("unknown.zip");
    write_zip(&target, &[("readme.txt", &b"nothing to see"[..])]);
    let pack = PackArchive::open_default(&target).expect("打开包");
    assert!(PackRegistry::builtin().detect_all(&pack).is_empty());
    let error = PackRegistry::builtin()
        .detect(&pack)
        .expect_err("没有格式认领就必须报错");
    assert_eq!(error.kind(), PackErrorKind::Unsupported);
    for id in ["curseforge", "modrinth", "multimc"] {
        assert!(
            error.message().contains(id),
            "错误里要列出试过哪些格式，实际：{}",
            error.message()
        );
    }
}

#[test]
fn multimc_needs_a_non_empty_component_graph() {
    let target = temp_root("detect-mmc-empty").join("empty-mmc.zip");
    write_zip(
        &target,
        &[(
            "mmc-pack.json",
            &br#"{"formatVersion":1,"components":[]}"#[..],
        )],
    );
    let pack = PackArchive::open_default(&target).expect("打开包");
    assert_eq!(
        MULTIMC.detect(&pack).expect("mmc-pack.json 可读"),
        None,
        "空 components 是空壳，不能当 MultiMC 实例"
    );
    assert_eq!(
        PackRegistry::builtin()
            .detect(&pack)
            .expect_err("不该有格式认领")
            .kind(),
        PackErrorKind::Unsupported
    );
}

#[test]
fn multimc_instance_directory_is_detected() {
    let root = fixtures::multimc("detect-mmc-dir");
    let pack = PackArchive::open_default(&root).expect("打开实例目录");
    let matched = PackRegistry::builtin()
        .detect(&pack)
        .expect("目录形式也要能探测");
    assert_eq!(matched.format().id(), "multimc");
    assert!(!matched.is_ambiguous());
}

#[test]
fn a_corrupt_manifest_is_a_metadata_error_not_unsupported() {
    // r1 p3-2：清单在、但读不出来 = 包坏了（`pack_metadata`）。之前三个 detect 都用
    // `.ok().flatten()` 吞掉解析错误，结果被报成「没有格式认领」——M111 的失败分级会据此
    // 把"可以重下文件"的包判成"不支持"。
    let target = temp_root("corrupt-manifest").join("bad.zip");
    write_zip(&target, &[("manifest.json", &br#"{"name": "#[..])]);
    let pack = PackArchive::open_default(&target).expect("zip 本身可读");

    let error = PackRegistry::builtin()
        .detect(&pack)
        .expect_err("坏清单必须报错");
    assert_eq!(error.kind(), PackErrorKind::Metadata, "{error}");
    assert!(
        !error.message().contains("没有格式认领"),
        "不许把「包坏了」说成「不认识」：{}",
        error.message()
    );
}

#[test]
fn a_corrupt_foreign_manifest_does_not_hide_a_valid_mrpack() {
    // 命中优先于失败：包里躺着一份别人的坏 manifest.json，不该让真 mrpack 也变得不可用。
    let target = temp_root("corrupt-foreign").join("mixed.zip");
    write_zip(
        &target,
        &[
            ("manifest.json", &br#"{"name": "#[..]),
            ("modrinth.index.json", MR_INDEX),
        ],
    );
    let pack = PackArchive::open_default(&target).expect("zip 本身可读");
    let matched = PackRegistry::builtin()
        .detect(&pack)
        .expect("mrpack 命中应当优先于其它格式的坏文件");
    assert_eq!(matched.format().id(), "modrinth");
    assert!(matched.also_matched().is_empty(), "CF 没命中（它坏了）");
}

#[test]
fn a_missing_manifest_is_not_an_error() {
    // 反向对照：**没有** `manifest.json` 是"不是 CF"，不是"包坏了"。
    let target = temp_root("no-manifest").join("mmc.zip");
    write_zip(
        &target,
        &[(
            "mmc-pack.json",
            &br#"{"formatVersion":1,"components":[{"uid":"net.minecraft","version":"1.20.1"}]}"#[..],
        )],
    );
    let pack = PackArchive::open_default(&target).expect("zip 本身可读");
    assert_eq!(CURSEFORGE.detect(&pack).expect("缺清单不是错误"), None);
    assert_eq!(
        PackRegistry::builtin()
            .detect(&pack)
            .expect("MultiMC 应当命中")
            .format()
            .id(),
        "multimc"
    );
}
