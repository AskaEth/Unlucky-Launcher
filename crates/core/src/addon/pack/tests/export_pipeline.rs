//! 导出管线（M112）：三分类、`.disabled` 路径后缀、`game`/`dependencies` 来自实例快照。
//!
//! 夹具在 [`super::export_harness`]：手造的实例目录（形状与 M111 导入产物一致）+
//! 注入式假远端目录（本仓库没有 Modrinth/CF 客户端，测试也不许联网）。

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use super::export_harness::{
    defaults, digests, fabric_instance, modrinth_hit, neoforge_instance, StubCatalog,
};
use super::{write_file, TempRoot};
use crate::addon::export::classify::classify_with;
use crate::addon::export::scan::{is_disabled_path, scan_instance};
use crate::addon::export::{export_instance, ExportRequest, FileClass, FileReason, RemoteHit};
use crate::addon::pack::archive::PackArchive;
use crate::addon::pack::curseforge::{fingerprint, CURSEFORGE};
use crate::addon::pack::error::PackErrorKind;
use crate::addon::pack::format::PackFormat;
use crate::addon::pack::manifest::{PackFile, PackManifest};
use crate::addon::pack::modrinth::MODRINTH;
use crate::addon::LoaderKind;

fn entry_names(path: &Path) -> Vec<String> {
    let pack = PackArchive::open_default(path).expect("打开 zip");
    pack.entries()
        .iter()
        .map(|entry| entry.name().to_string())
        .collect()
}

#[test]
fn offline_mrpack_export_inlines_every_file_with_its_suffix() {
    let root = fabric_instance("export-offline-mr");
    let target = root.with_file_name("offline.mrpack");
    let report =
        export_instance(&ExportRequest::new(&root, &target, &MODRINTH)).expect("离线导出必须成功");

    assert_eq!(report.format, "modrinth");
    assert_eq!(report.remote_count(), 0);
    assert_eq!(report.inline_count(), 3, "{:?}", report.files);
    assert_eq!(report.skipped_count(), 0);
    assert!(report.is_complete());
    assert!(report.bytes > 0, "包路径与字节数必须结构化给出");
    assert_eq!(report.minecraft.as_deref(), Some("1.20.1"));
    assert_eq!(report.loaders, vec!["fabric-loader 0.15.11".to_string()]);

    let pack = PackArchive::open_default(&target).expect("打开导出的包");
    let manifest = MODRINTH.read(&pack).expect("读回 mrpack");
    assert_eq!(manifest.name, "game");
    assert_eq!(manifest.version.as_deref(), Some("1.0.0"));
    assert_eq!(manifest.minecraft.as_deref(), Some("1.20.1"));
    let loader = manifest.loader(LoaderKind::Fabric).expect("加载器不能漏");
    assert_eq!(loader.version.as_deref(), Some("0.15.11"));

    let paths: Vec<&str> = manifest
        .override_files()
        .map(|(_, file)| file.path.as_str())
        .collect();
    assert_eq!(
        paths,
        vec!["config/known.toml", "mods/jei.jar", "mods/off.jar.disabled"],
        "三个内容文件都要在 overrides/ 里，禁用位保留后缀"
    );
    assert!(pack.has_entry("overrides/mods/off.jar.disabled"));
    assert!(
        !pack.has_entry("overrides/logs/latest.log"),
        "白名单外（logs/）不打包"
    );
}

#[test]
fn mrpack_index_declares_game_and_every_dependency() {
    let root = fabric_instance("export-index-mr");
    let target = root.with_file_name("index.mrpack");
    export_instance(&ExportRequest::new(&root, &target, &MODRINTH)).expect("导出");

    let pack = PackArchive::open_default(&target).expect("打开");
    let index = pack
        .read_json("modrinth.index.json")
        .expect("读清单")
        .expect("有清单");
    assert_eq!(index["game"], "minecraft");
    assert_eq!(index["formatVersion"], 1);
    assert_eq!(index["name"], "game");
    assert_eq!(index["dependencies"]["minecraft"], "1.20.1");
    assert_eq!(
        index["dependencies"]["fabric-loader"], "0.15.11",
        "只写 minecraft 漏加载器是本任务点名的不许"
    );
    assert!(index["files"].as_array().is_some_and(Vec::is_empty));
}

#[test]
fn a_modrinth_hit_becomes_a_files_entry_with_both_catalogs_urls() {
    let root = fabric_instance("export-remote-mr");
    let (sha1, value) = digests(&root, "mods/jei.jar");
    let catalog = StubCatalog::default()
        .with_modrinth(
            &sha1,
            modrinth_hit(&sha1, "https://cdn.modrinth.com/data/x/jei.jar"),
        )
        .with_curse(
            value,
            RemoteHit::curseforge(238_222, 5_102_025)
                .with_download("https://edge.forgecdn.net/files/5102/025/jei.jar"),
        );
    let target = root.with_file_name("remote.mrpack");
    let report =
        export_instance(&ExportRequest::new(&root, &target, &MODRINTH).with_catalog(&catalog))
            .expect("导出");

    assert_eq!(report.remote_count(), 1);
    let entry = report
        .files
        .iter()
        .find(|file| file.path == "mods/jei.jar")
        .expect("jei 在报告里");
    assert_eq!(entry.class, FileClass::Remote);
    assert_eq!(entry.reason, FileReason::ModrinthSha1);
    assert_eq!(
        entry.downloads.len(),
        2,
        "两条目录的直链都要写进 downloads[]"
    );

    let pack = PackArchive::open_default(&target).expect("打开");
    let manifest = MODRINTH.read(&pack).expect("读回");
    assert_eq!(manifest.files.len(), 1);
    let file = &manifest.files[0];
    assert_eq!(file.path.as_deref(), Some("mods/jei.jar"));
    assert_eq!(file.downloads.len(), 2);
    assert_eq!(file.sha512().map(str::len), Some(128));
    assert_eq!(
        file.sha1(),
        Some(sha1.as_str()),
        "本地实测的 sha1 要覆盖目录给的值"
    );
    assert_eq!(file.size, Some(13), "字节数按磁盘实测");
    assert_eq!(manifest.override_files().count(), 2, "另外两个文件仍然内联");
}

#[test]
fn a_hit_without_sha512_is_inlined_with_a_reason() {
    let root = fabric_instance("export-no-sha512");
    let (sha1, _) = digests(&root, "mods/jei.jar");
    let mut hashes = BTreeMap::new();
    hashes.insert("sha1".to_string(), sha1.clone());
    let catalog = StubCatalog::default().with_modrinth(
        &sha1,
        RemoteHit::modrinth(
            vec!["https://cdn.modrinth.com/data/x/jei.jar".into()],
            hashes,
            None,
        ),
    );
    let target = root.with_file_name("no-sha512.mrpack");
    let report =
        export_instance(&ExportRequest::new(&root, &target, &MODRINTH).with_catalog(&catalog))
            .expect("导出");

    let entry = report
        .files
        .iter()
        .find(|file| file.path == "mods/jei.jar")
        .expect("jei");
    assert_eq!(entry.class, FileClass::Inline);
    assert_eq!(entry.reason, FileReason::MissingSha512);
    assert_eq!(report.remote_count(), 0);
}

#[test]
fn a_curseforge_only_hit_cannot_be_written_into_an_mrpack() {
    let root = fabric_instance("export-curse-only");
    let (_, value) = digests(&root, "mods/jei.jar");
    let catalog =
        StubCatalog::default().with_curse(value, RemoteHit::curseforge(238_222, 5_102_025));
    let target = root.with_file_name("curse-only.mrpack");
    let report =
        export_instance(&ExportRequest::new(&root, &target, &MODRINTH).with_catalog(&catalog))
            .expect("导出");

    let entry = report
        .files
        .iter()
        .find(|file| file.path == "mods/jei.jar")
        .expect("jei");
    assert_eq!(entry.reason, FileReason::CurseOnlyMatch);
    assert_eq!(entry.class, FileClass::Inline);
}

#[test]
fn curseforge_export_writes_only_known_project_and_file_ids() {
    let root = fabric_instance("export-cf");
    let (_, value) = digests(&root, "mods/jei.jar");
    let catalog =
        StubCatalog::default().with_curse(value, RemoteHit::curseforge(238_222, 5_102_025));
    let target = root.with_file_name("pack.zip");
    let report =
        export_instance(&ExportRequest::new(&root, &target, &CURSEFORGE).with_catalog(&catalog))
            .expect("导出");

    assert_eq!(report.format, "curseforge");
    let reasons: Vec<(String, FileReason)> = report
        .files
        .iter()
        .map(|file| (file.path.clone(), file.reason))
        .collect();
    assert_eq!(
        reasons,
        vec![
            ("config/known.toml".to_string(), FileReason::NoRemoteMatch),
            (
                "mods/jei.jar".to_string(),
                FileReason::CurseForgeFingerprint
            ),
            (
                "mods/off.jar.disabled".to_string(),
                FileReason::DisabledNotExpressible
            ),
        ]
    );

    let pack = PackArchive::open_default(&target).expect("打开");
    let manifest = CURSEFORGE.read(&pack).expect("读回 CF 清单");
    assert_eq!(manifest.files.len(), 1, "files[] 只写能确定 id 的项");
    assert_eq!(manifest.files[0].project_id, Some(238_222));
    assert_eq!(manifest.files[0].file_id, Some(5_102_025));
    let names = entry_names(&target);
    assert!(names.contains(&"overrides/mods/off.jar.disabled".to_string()));
    assert!(names.contains(&"overrides/config/known.toml".to_string()));
}

#[test]
fn curseforge_modloaders_use_the_official_id_shape() {
    let root = neoforge_instance("export-cf-neo");
    let target = root.with_file_name("neo.zip");
    export_instance(&ExportRequest::new(&root, &target, &CURSEFORGE)).expect("导出");
    let pack = PackArchive::open_default(&target).expect("打开");
    let manifest = pack
        .read_json("manifest.json")
        .expect("读清单")
        .expect("有清单");
    assert_eq!(manifest["minecraft"]["version"], "1.20.1");
    assert_eq!(
        manifest["minecraft"]["modLoaders"][0]["id"], "neoforge-1.20.1-47.1.106",
        "CF 官方导出在 1.20.1 上把 MC 版本拼回 NeoForge 版本号"
    );
    assert_eq!(manifest["minecraft"]["modLoaders"][0]["primary"], true);

    let root = fabric_instance("export-cf-fabric");
    let target = root.with_file_name("fabric.zip");
    export_instance(&ExportRequest::new(&root, &target, &CURSEFORGE)).expect("导出");
    let pack = PackArchive::open_default(&target).expect("打开");
    let manifest = pack
        .read_json("manifest.json")
        .expect("读清单")
        .expect("有清单");
    assert_eq!(
        manifest["minecraft"]["modLoaders"][0]["id"],
        "fabric-0.15.11"
    );
    assert_eq!(manifest["overrides"], "overrides");
}

#[test]
fn the_sentinel_fingerprint_never_reaches_the_catalog() {
    let root = fabric_instance("export-sentinel");
    let (locals, _) = scan_instance(&root, &defaults());
    let local = locals
        .iter()
        .find(|file| file.path == "mods/jei.jar")
        .expect("jei");
    let catalog = StubCatalog::default()
        .with_curse(fingerprint::SENTINEL, RemoteHit::curseforge(1, 2))
        .with_curse(12345, RemoteHit::curseforge(1, 2));

    let (reason, hit) = classify_with(local, &catalog, true, Some(fingerprint::SENTINEL));
    assert_eq!(reason, FileReason::NoRemoteMatch);
    assert!(hit.is_none());
    assert!(
        catalog.curse_calls().is_empty(),
        "哨兵值必须短路，不能拿去查表（HMCL issue #4597 的误命中）"
    );

    // 对照：真实指纹会去查表。
    let (reason, hit) = classify_with(local, &catalog, true, Some(12345));
    assert_eq!(catalog.curse_calls(), vec![12345]);
    assert_eq!(reason, FileReason::CurseForgeFingerprint, "塞了 id 就命中");
    assert!(hit.is_some());
}

#[test]
fn a_disabled_file_is_still_disabled_after_a_round_trip_through_the_package() {
    // 任务要求的那条：禁用文件导出后仍能被"重新导入"且保持禁用。
    // 本轮没有 M111 的导入器（M111 未合并），所以"重新导入"= 把包里的 overrides 解出来 +
    // 重新扫一遍实例目录：扫出来的 disabled 位必须是 true。
    let root = fabric_instance("export-disabled");
    let target = root.with_file_name("disabled.mrpack");
    export_instance(&ExportRequest::new(&root, &target, &MODRINTH)).expect("导出");

    let again = TempRoot::new("export-disabled-reimport");
    let file = std::fs::File::open(&target).expect("打开包");
    let mut archive = zip::ZipArchive::new(file).expect("读 zip");
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("条目");
        let name = entry.name().expect("条目名").to_string();
        let Some(relative) = name.strip_prefix("overrides/") else {
            continue;
        };
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("读条目内容");
        write_file(&again.join(relative), &bytes);
    }

    let (locals, skipped) = scan_instance(&again, &defaults());
    assert!(skipped.is_empty(), "{skipped:?}");
    let disabled: Vec<&str> = locals
        .iter()
        .filter(|file| file.disabled)
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(disabled, vec!["mods/off.jar.disabled"]);
    assert!(is_disabled_path("mods/off.jar.disabled.old"));
    assert!(
        !is_disabled_path("mods/off.jar.old"),
        "`.old` 只是旧残留，不是禁用"
    );
}

#[test]
fn a_file_bigger_than_u32_is_a_named_limits_error() {
    // 真造一个 4 GiB 文件不现实：直接测格式层的那条闸（它是唯一写 `fileSize` 的地方）。
    let mut manifest = PackManifest::new("超大文件");
    manifest.version = Some("1.0.0".to_string());
    manifest.minecraft = Some("1.20.1".to_string());
    let mut hashes = BTreeMap::new();
    hashes.insert("sha512".to_string(), "b".repeat(128));
    manifest.files.push(PackFile {
        path: Some("mods/huge.jar".to_string()),
        hashes,
        downloads: vec!["https://cdn.modrinth.com/data/x/huge.jar".to_string()],
        size: Some(u64::from(u32::MAX) + 1),
        ..PackFile::default()
    });

    let error = MODRINTH.render(&manifest).expect_err("超限必须报错");
    assert_eq!(error.kind(), PackErrorKind::Limits);
    assert!(error.message().contains("fileSize"), "{error}");
}

#[test]
fn content_roots_can_be_extended_without_touching_the_pipeline() {
    // 「社区标准的扩展性」的最小证明：加一个目录不用改管线。
    let root = fabric_instance("export-extra-root");
    write_file(&root.join("custom/extra.txt"), b"extra content\n");
    let target = root.with_file_name("extra.mrpack");
    let mut roots = defaults();
    roots.push("custom".to_string());
    let report =
        export_instance(&ExportRequest::new(&root, &target, &MODRINTH).with_content_roots(roots))
            .expect("导出");
    assert!(report
        .files
        .iter()
        .any(|file| file.path == "custom/extra.txt"));
    let pack = PackArchive::open_default(&target).expect("打开");
    assert!(pack.has_entry("overrides/custom/extra.txt"));
}
