//! MultiMC / Prism 实例目录导入：`.minecraft/` 直拷、component 逐项交代、`instance.cfg` 取内存。

use serde_json::Value;

use crate::addon::import::{import_pack, keys, ImportRequest, ImportStatus};
use crate::addon::loader::LoaderKind;
use crate::download::blocking::block_on;

use super::{fixtures, list_tree, read_text, temp_root, write_file};

#[test]
fn imports_multimc_instance_end_to_end() {
    let source = fixtures::multimc("mmc-e2e");
    let target = temp_root("mmc-e2e-out").join("instance");
    let request = ImportRequest::new(&source, &target);
    let report = block_on(import_pack(&request));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    assert_eq!(report.format.as_deref(), Some("multimc"));
    assert_eq!(report.pack_name.as_deref(), Some(fixtures::MMC_NAME));
    assert_eq!(report.game_version.as_deref(), Some("1.20.1"));
    let loader = report.loader.as_ref().expect("fabric-loader 组件");
    assert_eq!(loader.kind, LoaderKind::Fabric);
    assert_eq!(loader.loader_version, "0.15.11");
    assert_eq!(loader.version_id, "1.20.1-fabric-0.15.11");
    // MultiMC 没有远端条目：内容就是目录。
    assert!(report.missing.is_empty());
    assert!(report
        .files
        .iter()
        .all(|file| file.source == crate::addon::import::FileSource::Overrides));

    // `.minecraft/` 直拷到实例根（不多一层 `.minecraft/`），零字节 jar 也照拷。
    assert_eq!(
        std::fs::read(target.join("mods/example.jar")).expect("example.jar"),
        b"PK\x03\x04 hand-made jar stub"
    );
    assert_eq!(
        std::fs::metadata(target.join("mods/broken.jar"))
            .expect("broken.jar")
            .len(),
        0,
        "零字节 jar 是夹具刻意造的：导入器不许悄悄丢掉它"
    );
    assert!(target.join("config/known.toml").is_file());
    assert_eq!(
        list_tree(&target),
        vec![
            super::baseline_rel(),
            "config/known.toml".to_string(),
            "mods/broken.jar".to_string(),
            "mods/example.jar".to_string(),
        ]
    );
}

#[test]
fn unsupported_components_are_listed_one_by_one_not_dropped() {
    let source = fixtures::multimc("mmc-components");
    let target = temp_root("mmc-components-out").join("instance");
    let request = ImportRequest::new(&source, &target);
    let report = block_on(import_pack(&request));

    // 夹具里的 `org.lwjgl3` 不是加载器 ⇒ 必须逐项进"未支持清单"。
    assert_eq!(
        report.rejected_components.len(),
        1,
        "{:?}",
        report.rejected_components
    );
    let rejected = &report.rejected_components[0];
    assert_eq!(rejected.uid, "org.lwjgl3");
    assert_eq!(rejected.version.as_deref(), Some("3.3.2"));
    assert_eq!(rejected.message_key, keys::REJECTED_COMPONENT);

    // **不静默丢**的断言：`mmc-pack.json` 里每一个 component 都能在报告里找到归宿。
    let pack: Value =
        serde_json::from_str(&read_text(&source.join("mmc-pack.json"))).expect("解析 mmc-pack");
    let components = pack["components"].as_array().expect("components 数组");
    assert!(!components.is_empty());
    for component in components {
        let uid = component["uid"].as_str().expect("uid");
        let version = component["version"].as_str().expect("version");
        if uid == "net.minecraft" {
            assert_eq!(
                report.game_version.as_deref(),
                Some(version),
                "{uid} 要变成游戏版本"
            );
            continue;
        }
        let as_loader = report
            .loader
            .as_ref()
            .is_some_and(|loader| loader.raw == uid);
        let as_rejected = report
            .rejected_components
            .iter()
            .any(|rejected| rejected.uid == uid);
        assert!(
            as_loader || as_rejected,
            "component {uid} 既没成为加载器、也没进未支持清单（= 静默丢了）"
        );
    }
}

#[test]
fn reads_memory_from_instance_cfg_only_when_override_memory_is_on() {
    let root = temp_root("mmc-mem");
    let source = root.join("instance");
    write_file(
        &source.join("mmc-pack.json"),
        br#"{"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"},
            {"uid": "net.fabricmc.fabric-loader", "version": "0.15.11"}]}"#,
    );
    write_file(
        &source.join("instance.cfg"),
        b"InstanceType=OneSix\nname=mem pack\nOverrideMemory=true\nMaxMemAlloc=6144\nMinMemAlloc=1024\n",
    );
    write_file(&source.join(".minecraft/mods/a.jar"), b"jar");

    let target = temp_root("mmc-mem-out").join("instance");
    let report = block_on(import_pack(&ImportRequest::new(&source, &target)));
    assert_eq!(report.max_memory_mb, Some(6144), "{:?}", report.error);
    assert!(report
        .warnings
        .iter()
        .any(|note| note.message_key == keys::NOTE_INSTANCE_CFG_MEMORY));

    // 开关没开时不采用（只看值不看开关会误判）。
    let gated = root.join("gated");
    write_file(
        &gated.join("mmc-pack.json"),
        br#"{"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"}]}"#,
    );
    write_file(
        &gated.join("instance.cfg"),
        b"InstanceType=OneSix\nname=gated\nOverrideMemory=false\nMaxMemAlloc=6144\n",
    );
    write_file(&gated.join(".minecraft/mods/a.jar"), b"jar");
    let gated_target = temp_root("mmc-mem-gated-out").join("instance");
    let gated_report = block_on(import_pack(&ImportRequest::new(&gated, &gated_target)));
    assert_eq!(gated_report.max_memory_mb, None);
    assert!(gated_report
        .warnings
        .iter()
        .any(|note| note.message_key == keys::NOTE_INSTANCE_CFG_MEMORY));
}

#[test]
fn registers_the_instance_when_a_launcher_layout_is_given() {
    // 登记阶段（读取清单 → 解压 → 补全 → 落地 → **注册实例**）的 happy path：
    // 内容落盘后写 profiles.json + config/instances/<id>.json。
    let root = temp_root("mmc-register");
    let source = fixtures::multimc("mmc-register-src");
    let target = root.join("instance");
    let home = root.join("data");
    let layout = crate::platform::layout::LauncherLayout::new(&home);
    let request = ImportRequest::new(&source, &target).with_registration(&layout, Some("my pack"));
    let report = block_on(import_pack(&request));

    assert_eq!(report.status, ImportStatus::Success, "{:?}", report.error);
    let registered = report.registered.as_ref().expect("要登记实例");
    assert_eq!(registered.name, "my pack");
    assert_eq!(registered.version_id, "1.20.1-fabric-0.15.11");
    assert!(registered.created);
    // profiles.json 真的写出来了，且指回同一个版本 id。
    let profiles = crate::game::install::instance::read_profiles(&layout).expect("读档案");
    assert_eq!(profiles.profiles.len(), 1);
    assert_eq!(profiles.profiles[0].version_id, "1.20.1-fabric-0.15.11");
    assert_eq!(profiles.profiles[0].name, "my pack");
    assert!(report
        .warnings
        .iter()
        .any(|note| note.message_key == keys::NOTE_REGISTERED));
    assert!(report.rollback.is_none(), "成功路径不该有回滚记录");
}

#[test]
fn rejects_mmc_pack_with_other_format_version() {
    // Prism 的规矩：`formatVersion` 必须恰为 1，否则整份丢弃（不是降级解析）。
    let root = temp_root("mmc-v2");
    let source = root.join("instance");
    write_file(
        &source.join("mmc-pack.json"),
        br#"{"formatVersion": 2, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"}]}"#,
    );
    write_file(
        &source.join("instance.cfg"),
        b"InstanceType=OneSix\nname=v2\n",
    );
    write_file(&source.join(".minecraft/mods/a.jar"), b"jar");

    let target = temp_root("mmc-v2-out").join("instance");
    let report = block_on(import_pack(&ImportRequest::new(&source, &target)));

    assert_eq!(report.status, ImportStatus::Failed);
    let error = report.error.as_ref().expect("必须有具名错误");
    assert_eq!(error.code(), "pack_metadata");
    assert!(
        error.message().contains("formatVersion"),
        "{}",
        error.message()
    );
    assert!(report.rolled_back);
    super::assert_absent(&target);
}
