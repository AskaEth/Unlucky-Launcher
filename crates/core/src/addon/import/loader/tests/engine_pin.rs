//! **产品路径**判据（M332 r1 P2-2）：真 `EngineLoaderInstaller` 必须把版本 JSON 落在**清单声明的
//! 那个 id** 下。
//!
//! # 为什么需要这一条
//!
//! 复审 r1 的变异 C：把 `engine.rs` 里那句 `version::adopt_declared_id(&mut release, target);`
//! 删掉，`cargo test -p unlucky-core addon` **全绿**——而产品会静默退回「导入成功、登记 id 下没有
//! 版本 JSON」。原因是当时所有用例都用 `FakeInstaller::installs_as` **替** engine 把 id 钉好了，
//! 「谁负责钉 id」这件事没有任何机器判据。
//!
//! 所以这条用例**走真 engine**：`EngineLoaderInstaller::install(...)` 从 installer jar（形状 B，
//! 内联 `versionInfo`）一路跑到写盘，断言两件事：①报回来的 `version_id` 是清单短形；②盘上
//! `versions/<清单 id>/<清单 id>.json` 真的存在、`versions/<双后缀 id>/` 不存在。删掉那一行 ⇒
//! ②立刻红（`done.version_id` 会变成双后缀 id）。
//!
//! # 为什么零网络
//!
//! 三处输入全部预置在盘上：① installer jar 落在 `meta_cache/`（`fetch_installer` 命中缓存就不发
//! 请求）；② `versions/1.7.10/1.7.10.json` 预置 ⇒ `parent_installed` 成立，不触发原版那一段
//! （那一段要真 assets）；③ loader 计划里的每个文件先铺一个空文件（`execute::current_bytes` 对
//! 「没有摘要也没有大小」的文件"存在即信"）。于是整条路一个请求都不发。
//!
//! # 一处如实说明（target 是怎么造的）
//!
//! 真实现到达闸①时，`target` 是清单算出来的（`loader_version = 10.13.4.1614`（短形）、
//! `version_id = 1.7.10-forge-10.13.4.1614`），而 `resolve` 要**先 404 掉单后缀坐标**才回退到
//! 双后缀（`maven_version_candidates` 第二条；M330 干跑 `out-1710short.json` 实测）。单测不联网，
//! 没法让那条 404 只发生在本地（引擎在 404 后会去官方源兜底），所以这里把 `loader_version` 直接
//! 写成**解析之后**的双后缀写法（= 真实现回退成功后手里那一份），`version_id` 仍是清单短形。
//! 也就是说：候选回退不在这条用例里（由 M330 的干跑与真跑覆盖），这条只钉「拿到同一个版本之后，
//! 落盘 id 必须是清单那一个」。

use std::io::Write;
use std::path::Path;

use crate::addon::import::{ImportCancel, LoaderTarget};
use crate::addon::loader::forge::{build_release, InstallProfile};
use crate::addon::loader::{LoaderInstallPlan, LoaderKind, LoaderPlanInput};
use crate::addon::temp::TempRoot;
use crate::download::blocking::block_on;
use crate::download::cache::ContentCache;
use crate::download::config::EngineConfig;
use crate::download::engine::DownloadEngine;
use crate::download::health::HealthRegistry;
use crate::download::mirrors::MirrorTable;
use crate::download::region::RegionProfile;
use crate::game::arguments::Environment;
use crate::game::layout::GameLayout;
use crate::game::manifest::VersionJson;
use crate::platform::layout::LauncherLayout;

use super::super::{EngineLoaderInstaller, LoaderInstallJob, LoaderInstaller};

/// 真 maven 元数据里这条发布的坐标（`1.7.10` 的 68 条双后缀之一，`GET` 实测）。
const MAVEN_1710: &str = "1.7.10-10.13.4.1614-1.7.10";
/// 清单声明的短形（CF `modLoaders[].id = "forge-10.13.4.1614"`）算出来的版本 id。
const DECLARED_ID: &str = "1.7.10-forge-10.13.4.1614";
/// `adopt_declared_id` 之前的 id（双后缀写法）——盘上**不该**有它。
const RESOLVED_ID: &str = "1.7.10-forge-10.13.4.1614-1.7.10";

/// 老协议形状 B 的 1.7.10 installer profile（与 `addon/loader/tests/forge_legacy.rs` 的
/// `FORGE_1710_PROFILE` 同源：实测 `C:/Temp/ull-forge1122/installer-1.7.10.jar` 的
/// `install_profile.json` 裁剪到 2 条库）。
const PROFILE_1710: &str = r#"{
  "install": {
    "profileName": "Forge",
    "target": "1.7.10-Forge10.13.4.1614-1.7.10",
    "path": "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10",
    "version": "Forge 10.13.4.1614-1.7.10",
    "filePath": "forge-1.7.10-10.13.4.1614-1.7.10-universal.jar",
    "minecraft": "1.7.10"
  },
  "versionInfo": {
    "id": "1.7.10-Forge10.13.4.1614-1.7.10",
    "type": "release",
    "minecraftArguments": "--username ${auth_player_name} --tweakClass cpw.mods.fml.common.launcher.FMLTweaker",
    "mainClass": "net.minecraft.launchwrapper.Launch",
    "inheritsFrom": "1.7.10",
    "jar": "1.7.10",
    "assets": "1.7.10",
    "libraries": [
      {"name": "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10", "url": "https://maven.minecraftforge.net/"},
      {"name": "net.minecraft:launchwrapper:1.12", "serverreq": true}
    ]
  }
}"#;

#[test]
fn the_production_engine_lands_the_version_json_under_the_declared_id() {
    let root = TempRoot::new("m332-engine-pin");
    let game = GameLayout::new(root.join("game"));
    let layout = LauncherLayout::new(root.join("data"));

    // ① installer 预置在 engine 的元数据缓存里（文件名规则与 `LoaderSource::installer_path` 同）。
    write_installer(
        &layout
            .meta_cache()
            .join(format!("forge-{MAVEN_1710}-installer.jar")),
    );
    // ② 原版那一层预置 ⇒ 不触发 `install_vanilla`（那一段要真 assets）。
    write_file(&game.version_json("1.7.10"), br#"{"id": "1.7.10"}"#);
    // ③ loader 计划里的每个文件先铺好（`execute` 对无摘要无大小的文件"存在即信"⇒ 一个都不下）。
    let profile: InstallProfile = serde_json::from_str(PROFILE_1710).expect("profile 解析");
    let release = build_release(LoaderKind::Forge, "1.7.10", MAVEN_1710, &profile, None)
        .expect("老协议 1.7.10 应走 json 路线");
    let seeded = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &game,
        release: &release,
        environment: &Environment::host(),
        parent_installed: true,
    })
    .expect("组计划");
    assert!(
        !seeded.files().is_empty(),
        "计划里得有库文件，否则这条用例什么也没验"
    );
    for file in seeded.files() {
        write_file(&file.dest, b"x");
    }

    // ④ 真 engine 真跑（零网络）。
    let engine = DownloadEngine::new(
        EngineConfig::default(),
        std::sync::Arc::new(MirrorTable::builtin()),
        std::sync::Arc::new(RegionProfile::direct(0)),
        std::sync::Arc::new(HealthRegistry::in_memory()),
        std::sync::Arc::new(ContentCache::open(root.join("cache"))),
    );
    let installer = EngineLoaderInstaller::new(&engine, &layout);
    let cancel = ImportCancel::new();
    let target = LoaderTarget {
        kind: LoaderKind::Forge,
        mc_version: "1.7.10".to_string(),
        // 解析之后的那一份（见模块文档「一处如实说明」）。
        loader_version: MAVEN_1710.to_string(),
        version_id: DECLARED_ID.to_string(),
        raw: "forge-10.13.4.1614".to_string(),
    };
    let done = block_on(installer.install(LoaderInstallJob {
        layout: &game,
        target: &target,
        cancel: &cancel,
        steps: None,
        progress: None,
    }))
    .expect("真 engine 装老协议 loader 不该失败（全部输入已预置）");

    // ① 报回来的 id 是清单短形（= 落盘 id 与登记 id 同一个）。
    assert_eq!(
        done.version_id, DECLARED_ID,
        "落盘 id 必须钉成清单声明的那个"
    );
    // ② 清单 id 下真的有版本 JSON，双后缀 id 下什么都不该有。
    assert!(
        game.is_version_installed(DECLARED_ID),
        "versions/{DECLARED_ID}/ 里必须有版本 JSON（删掉 adopt_declared_id 就会红在这里）"
    );
    assert!(
        !game.is_version_installed(RESOLVED_ID),
        "不许把版本 JSON 落在 {RESOLVED_ID}（登记的是清单 id，那会变成读不到的实例）"
    );
    let text = std::fs::read_to_string(game.version_json(DECLARED_ID)).expect("读版本 JSON");
    let manifest = VersionJson::from_json_str(&text).expect("解析版本 JSON");
    assert_eq!(
        manifest.id.as_deref(),
        Some(DECLARED_ID),
        "JSON 里的 id 也要是清单 id"
    );
    assert_eq!(manifest.inherits_from.as_deref(), Some("1.7.10"));
    // ③ 库清单不跟着 id 走：本体坐标仍是官方 maven 的双后缀写法（只改写法，不改内容）。
    let libraries = manifest.libraries.expect("应有库");
    assert!(
        libraries
            .iter()
            .any(|library| library.name == format!("net.minecraftforge:forge:{MAVEN_1710}")),
        "{:?}",
        libraries
            .iter()
            .map(|library| &library.name)
            .collect::<Vec<_>>()
    );
    // ④ 「官方 maven 上叫另一个名字」这件事要进告警（人读与 --json 同一份）。
    let notes: Vec<&String> = done
        .warnings
        .iter()
        .filter(|note| note.contains(MAVEN_1710))
        .collect();
    assert_eq!(notes.len(), 1, "{:?}", done.warnings);
    assert!(notes[0].contains(DECLARED_ID), "{:?}", done.warnings);
}

/// 写一个只含 `install_profile.json` 的 installer jar（形状 B：没有独立 `version.json`）。
fn write_installer(path: &Path) {
    write_file(path, b"");
    let file = std::fs::File::create(path).expect("建 installer");
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("install_profile.json", options)
        .expect("写条目");
    zip.write_all(PROFILE_1710.as_bytes()).expect("写 profile");
    zip.finish().expect("收尾 zip");
}

/// 建父目录并写一个文件（预铺用；内容不重要，`execute` 只看"在不在"）。
fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建父目录");
    }
    std::fs::write(path, bytes).expect("写预铺文件");
}
