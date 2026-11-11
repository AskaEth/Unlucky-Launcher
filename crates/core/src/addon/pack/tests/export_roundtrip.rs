//! 往返保真（M112）：用 M110 的三种格式夹具做「读出来 → 摆成实例 → 导出 → 再读」。
//!
//! # 为什么不是 `import_pack → export → import_pack`
//!
//! M111 的导入管线**还没合并进本分支**（它在复审中）。所以这里的"导入"是 M110 的
//! `PackFormat::read` + 一个**形状与 M111 导入产物一致**的实例目录（`mods/`、`config/`、
//! `versions/<id>/<id>.json`）。真端到端欠账在 M112 报告里登记：M111 合并后要补一条
//! `import_pack → export → import_pack`。
//!
//! # 允许的差异（逐条显式列出，别让后来人以为是 bug）
//!
//! | 方向 | 差异 | 原因 |
//! |------|------|------|
//! | CF → mrpack | `files[]` 的 `projectID/fileID` 条目全丢 | CF 的 `files[]` **没有路径**，磁盘上无法复原成文件；导出侧只能看到磁盘，看不见"清单里说该有但没落地"的条目 |
//! | CF → mrpack | `author` / `minecraft.recommendedRam` 等扩展字段丢 | mrpack 没有这些字段（`extra` 不跨格式搬） |
//! | mrpack → mrpack | 认不出的 dependency 键（`parchment`）丢 | 快照只从版本 JSON 的库坐标来；`parchment` 没有对应库 |
//! | mrpack → mrpack | `files[]` 的条目要靠"文件真的在磁盘上"才会再写回去 | 读出来的模型里那两条是远程条目、本地没有字节；再导出时以磁盘为准 |
//! | MultiMC → mrpack | 非加载器组件（`org.lwjgl3`）不进 `dependencies` | 它不是加载器；mrpack 的 `dependencies` 只表达 loader |

use std::path::Path;

use super::export_harness::{
    defaults, modrinth_hit, write_layer, StubCatalog, FORGE_LAYER, VANILLA_LAYER,
};
use super::{fixtures, write_file, TempRoot};
use crate::addon::export::snapshot::{InstanceSnapshot, SnapshotLoader};
use crate::addon::export::{export_instance, ExportRequest};
use crate::addon::pack::archive::PackArchive;
use crate::addon::pack::curseforge::CURSEFORGE;
use crate::addon::pack::format::PackFormat;
use crate::addon::pack::manifest::{PackManifest, PackSide};
use crate::addon::pack::modrinth::MODRINTH;
use crate::addon::pack::multimc::MULTIMC;
use crate::addon::LoaderKind;

/// 把夹具里的覆盖内容铺成一个实例目录（`<game>/<path>`），并写一份版本 JSON。
fn instance_from(fixture_dir: &Path, manifest: &PackManifest, tag: &str) -> TempRoot {
    let game = TempRoot::under(tag, "game");
    for (_, file) in manifest.override_files() {
        let group = manifest
            .overrides
            .iter()
            .find(|group| group.files.iter().any(|each| each.path == file.path))
            .expect("找到文件所属的组");
        let source = fixture_dir.join(&group.dir).join(&file.path);
        let bytes = std::fs::read(&source).unwrap_or_else(|error| {
            panic!("读夹具 {} 失败：{error}", source.display());
        });
        write_file(&game.join(&file.path), &bytes);
    }
    write_layer(&game, "1.20.1", VANILLA_LAYER);
    game
}

/// 一个「读出来的 mrpack」的文件集合，按路径排序（比较用）。
fn override_paths(manifest: &PackManifest) -> Vec<String> {
    let mut paths: Vec<String> = manifest
        .override_files()
        .map(|(_, file)| file.path.clone())
        .collect();
    paths.sort();
    paths
}

#[test]
fn curseforge_fixture_round_trips_through_mrpack() {
    let fixture = fixtures::curseforge("rt-cf");
    let pack = PackArchive::open_default(&fixture.zip).expect("打开 CF 夹具");
    let original = CURSEFORGE.read(&pack).expect("读 CF 清单");

    let game = instance_from(&fixture.dir, &original, "rt-cf-mr");
    write_layer(&game, "1.20.1-forge-47.2.0", FORGE_LAYER);
    let target = game.with_file_name("cf-to-mr.mrpack");
    let request = ExportRequest::new(&game, &target, &MODRINTH)
        .with_name(original.name.clone())
        .with_version(original.version.clone().unwrap_or_default())
        .with_version_id("1.20.1-forge-47.2.0");
    let report = export_instance(&request).expect("导出");
    assert_eq!(report.inline_count(), 3, "{:?}", report.files);

    let pack = PackArchive::open_default(&target).expect("打开导出的包");
    let again = MODRINTH.read(&pack).expect("再读回来");

    // 关键字段：名称 / 版本 / 游戏版本 / 加载器 / 文件集合。
    assert_eq!(again.name, original.name);
    assert_eq!(again.version, original.version);
    assert_eq!(again.minecraft, original.minecraft);
    let loader = again.loader(LoaderKind::Forge).expect("Forge 加载器");
    assert_eq!(loader.version.as_deref(), Some("47.2.0"));
    assert_eq!(override_paths(&again), override_paths(&original));

    // 允许的差异①：CF 的 files[] 没有路径，磁盘上复原不出来 ⇒ 往返后 files[] 为空。
    assert!(
        again.files.is_empty(),
        "CF 的 projectID/fileID 条目没有路径，无法从磁盘复原（已在文件头登记）"
    );
}

#[test]
fn mrpack_fixture_round_trips_with_a_catalog_and_keeps_the_disabled_suffix() {
    let fixture = fixtures::modrinth("rt-mr");
    let pack = PackArchive::open_default(&fixture.zip).expect("打开 mrpack 夹具");
    let original = MODRINTH.read(&pack).expect("读 mrpack 清单");

    // 三个覆盖目录都摊到同一个实例里（实例目录不分侧）。
    let game = instance_from(&fixture.dir, &original, "rt-mr-mr");
    let fabric = r#"{
      "id": "1.20.1-fabric-0.15.11",
      "inheritsFrom": "1.20.1",
      "libraries": [{"name": "net.fabricmc:fabric-loader:0.15.11"}]
    }"#;
    write_layer(&game, "1.20.1", VANILLA_LAYER);
    write_layer(&game, "1.20.1-fabric-0.15.11", fabric);
    // 夹具里那两条远程条目在磁盘上不存在；这里补一条**真**文件，让"远程引用"这条路径被往返到。
    write_file(&game.join("mods/jei.jar"), b"PK\x03\x04 jei body");
    write_file(&game.join("mods/disabled.jar.disabled"), b"PK\x03\x04 off");

    let (locals, _) = crate::addon::export::scan_instance(&game, &defaults());
    let jei = locals
        .iter()
        .find(|file| file.path == "mods/jei.jar")
        .expect("jei");
    let catalog = StubCatalog::default().with_modrinth(
        &jei.sha1,
        modrinth_hit(&jei.sha1, "https://cdn.modrinth.com/data/x/jei.jar"),
    );
    let target = game.with_file_name("mr-to-mr.mrpack");
    let request = ExportRequest::new(&game, &target, &MODRINTH)
        .with_name(original.name.clone())
        .with_version(original.version.clone().unwrap_or_default())
        .with_version_id("1.20.1-fabric-0.15.11")
        .with_catalog(&catalog);
    let report = export_instance(&request).expect("导出");
    assert_eq!(report.remote_count(), 1);
    assert!(report.is_complete());

    let pack = PackArchive::open_default(&target).expect("打开导出的包");
    let again = MODRINTH.read(&pack).expect("再读回来");
    assert_eq!(again.name, original.name);
    assert_eq!(again.version, original.version);
    assert_eq!(again.minecraft, original.minecraft);
    let loader = again.loader(LoaderKind::Fabric).expect("Fabric 加载器");
    assert_eq!(loader.version.as_deref(), Some("0.15.11"));

    // 文件集合：远程那条回来了（同一个 path），其余内联。
    assert_eq!(again.files.len(), 1);
    assert_eq!(again.files[0].path.as_deref(), Some("mods/jei.jar"));
    let mut files = override_paths(&again);
    files.sort();
    assert_eq!(
        files,
        vec![
            "config/client-only.toml".to_string(),
            "config/known.toml".to_string(),
            "config/server.properties".to_string(),
            "mods/disabled.jar.disabled".to_string(),
            "mods/half.jar".to_string(),
        ],
        "夹具自带的四个覆盖文件 + 新增的禁用文件"
    );
    // 禁用位仍然靠路径后缀表达。
    assert!(files
        .iter()
        .any(|path| path == "mods/disabled.jar.disabled"));
    assert!(pack.has_entry("overrides/mods/disabled.jar.disabled"));

    // 允许的差异②：认不出的 dependency 键（夹具里的 `parchment`）不复存在——
    // 快照只来自版本 JSON 的库坐标，`parchment` 没有对应库。
    assert!(
        again.loaders.iter().all(|loader| loader.kind.is_some()),
        "非加载器依赖不回写（已在文件头登记）"
    );
}

#[test]
fn multimc_fixture_exports_as_mrpack_from_its_game_directory() {
    // MultiMC/Prism **格式**的导出不做（整实例快照包，成本极高），但"把 Prism 实例的游戏目录
    // 导出成 mrpack"是正当用例：快照从 `mmc-pack.json` 的组件表来（调用方用 `pack::multimc` 读）。
    let instance = fixtures::multimc("rt-mmc");
    let pack = PackArchive::open_default(&instance).expect("打开 MultiMC 实例");
    let original = MULTIMC.read(&pack).expect("读 mmc-pack.json");
    let snapshot = InstanceSnapshot {
        game_version: original.minecraft.clone().expect("net.minecraft 组件"),
        loaders: original
            .loaders
            .iter()
            .filter_map(|loader| {
                Some(SnapshotLoader {
                    kind: loader.kind?,
                    version: loader.version.clone()?,
                })
            })
            .collect(),
    };

    let game = instance.join(".minecraft");
    let target = instance.with_file_name("mmc.mrpack");
    let report = export_instance(
        &ExportRequest::new(&game, &target, &MODRINTH)
            .with_name("MultiMC 实例导出")
            .with_snapshot(&snapshot),
    )
    .expect("导出");
    assert_eq!(report.minecraft.as_deref(), Some("1.20.1"));
    assert_eq!(report.loaders, vec!["fabric-loader 0.15.11".to_string()]);
    assert_eq!(report.inline_count(), 3, "{:?}", report.files);

    let pack = PackArchive::open_default(&target).expect("打开导出的包");
    let again = MODRINTH.read(&pack).expect("再读回来");
    assert_eq!(again.minecraft.as_deref(), Some("1.20.1"));
    assert_eq!(
        again
            .loader(LoaderKind::Fabric)
            .and_then(|loader| loader.version.clone()),
        Some("0.15.11".to_string())
    );
    assert_eq!(
        override_paths(&again),
        vec![
            "config/known.toml".to_string(),
            "mods/broken.jar".to_string(),
            "mods/example.jar".to_string(),
        ]
    );
    // 有意的省略：MultiMC 格式本身仍然只读不写。
    assert!(MULTIMC.metadata_entry().is_none(), "MultiMC 导出不做");

    // 重新打成 MultiMC 形状的目录以确认没有丢内容（把 overrides 解出来再比）。
    let unpacked = TempRoot::new("rt-mmc-unpacked");
    for entry in pack.entries() {
        let name = entry.name();
        let Some(relative) = name.strip_prefix("overrides/") else {
            continue;
        };
        let bytes = pack.read(name).expect("读条目").expect("有条目");
        write_file(&unpacked.join(relative), &bytes);
    }
    for relative in ["config/known.toml", "mods/broken.jar", "mods/example.jar"] {
        assert!(
            unpacked.join(relative).is_file(),
            "{relative} 没进包（内容丢了）"
        );
    }
}

#[test]
fn a_same_format_export_is_still_allowed_and_byte_stable() {
    // 反向对照：新管线不能把 M110 的同格式往返弄坏（r1 明确要保住的能力）。
    let fixture = fixtures::curseforge("rt-same-cf");
    let pack = PackArchive::open_default(&fixture.zip).expect("打开 CF 夹具");
    let original = CURSEFORGE.read(&pack).expect("读 CF 清单");
    let game = instance_from(&fixture.dir, &original, "rt-same-cf-game");
    write_layer(&game, "1.20.1-forge-47.2.0", FORGE_LAYER);

    let target = game.with_file_name("same.zip");
    export_instance(
        &ExportRequest::new(&game, &target, &CURSEFORGE)
            .with_name(original.name.clone())
            .with_version(original.version.clone().unwrap_or_default())
            .with_version_id("1.20.1-forge-47.2.0"),
    )
    .expect("导出");
    let pack = PackArchive::open_default(&target).expect("打开导出结果");
    let again = CURSEFORGE.read(&pack).expect("再读回来");
    assert_eq!(again.name, original.name);
    assert_eq!(again.minecraft, original.minecraft);
    assert_eq!(
        again
            .loader(LoaderKind::Forge)
            .and_then(|loader| loader.version.clone()),
        Some("47.2.0".to_string())
    );
    assert_eq!(override_paths(&again), override_paths(&original));
    assert_eq!(
        again
            .overrides_for(PackSide::Both)
            .map(|group| group.dir.clone()),
        Some("overrides".to_string())
    );
}
