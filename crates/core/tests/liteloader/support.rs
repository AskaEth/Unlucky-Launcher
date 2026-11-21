//! 集成测试公用件：临时游戏目录、离线下载引擎、元数据夹具、父版本清单夹具。

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use unlucky_core::addon::loader::liteloader::{self, LiteLoaderEntry};
use unlucky_core::addon::loader::LoaderRelease;
use unlucky_core::download::backoff::BackoffPolicy;
use unlucky_core::download::cache::ContentCache;
use unlucky_core::download::chunked::ChunkedConfig;
use unlucky_core::download::config::EngineConfig;
use unlucky_core::download::engine::DownloadEngine;
use unlucky_core::download::health::HealthRegistry;
use unlucky_core::download::mirrors::MirrorTable;
use unlucky_core::download::region::RegionProfile;
use unlucky_core::game::layout::GameLayout;
use unlucky_core::game::manifest::VersionJson;

/// 真实响应的裁剪版（见 `tests/liteloader.rs` 文件头的来源说明）。
pub const VERSION_TABLE: &str = include_str!("fixtures/liteloader-versions.json");

/// 1.12.2-SNAPSHOT 的 maven-metadata（真实响应原样）。
pub const METADATA_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>com.mumfrey</groupId>
  <artifactId>liteloader</artifactId>
  <version>1.12.2-SNAPSHOT</version>
  <versioning>
    <snapshot>
      <timestamp>20171128.144431</timestamp>
      <buildNumber>4</buildNumber>
    </snapshot>
    <lastUpdated>20171128144431</lastUpdated>
  </versioning>
</metadata>"#;

/// 原版 1.12.2（老式 `minecraftArguments`，与官方清单同形）。
pub const VANILLA_1122: &str = r#"{
  "id": "1.12.2",
  "type": "release",
  "mainClass": "net.minecraft.client.main.Main",
  "assets": "1.12",
  "assetIndex": {"id": "1.12"},
  "minecraftArguments": "--username ${auth_player_name} --version ${version_name}",
  "libraries": [
    {"name": "com.mojang:patchy:1.1", "url": "https://libraries.minecraft.net/"}
  ]
}"#;

/// Forge 1.12.2-14.23.5.2860（spec-0 的 version.json：主类 launchwrapper + FMLTweaker）。
pub const FORGE_1122: &str = r#"{
  "id": "1.12.2-forge-14.23.5.2860",
  "inheritsFrom": "1.12.2",
  "mainClass": "net.minecraft.launchwrapper.Launch",
  "minecraftArguments": "--tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker",
  "libraries": [
    {"name": "net.minecraftforge:forge:1.12.2-14.23.5.2860", "url": "https://maven.minecraftforge.net/"}
  ]
}"#;

/// 唯一临时目录守卫（`drop` 时递归删除）。
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(tag: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "ull-liteloader-{tag}-{}-{stamp}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("临时目录应可创建");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 离线引擎：单次尝试、短超时、分块关闭（测试里只用来跑「全部跳过」的执行段）。
pub fn engine(cache_root: &Path) -> DownloadEngine {
    let config = EngineConfig {
        max_attempts_per_source: 1,
        backoff: BackoffPolicy {
            base_ms: 1,
            max_ms: 2,
            jitter_ratio: 0.0,
            max_attempts: 1,
        },
        connect_timeout: Duration::from_secs(3),
        read_timeout: Duration::from_secs(3),
        chunked: ChunkedConfig::disabled(),
        ..EngineConfig::default()
    };
    DownloadEngine::new(
        config,
        Arc::new(MirrorTable::builtin()),
        Arc::new(RegionProfile::direct(0)),
        Arc::new(HealthRegistry::in_memory()),
        Arc::new(ContentCache::open(cache_root.join("cache"))),
    )
}

/// 解析夹具表里某个原版版本的条目。
pub fn entries(mc_version: &str) -> Vec<LiteLoaderEntry> {
    let table = liteloader::parse_versions(VERSION_TABLE).expect("夹具版本表应可解析");
    table.get(mc_version).cloned().unwrap_or_default()
}

/// 取 1.12.2 的 SNAPSHOT 条目并合成 release（父版本由调用方给）。
pub fn liteloader_release(parent_version_id: &str) -> LoaderRelease {
    let entries = entries("1.12.2");
    let entry = liteloader::default_entry(&entries).expect("1.12.2 应有一条版本");
    let url = liteloader::snapshot_artifact_url(entry, METADATA_XML).expect("快照直链");
    liteloader::build_release("1.12.2", parent_version_id, entry, &url).expect("合成清单")
}

/// 把一份版本 JSON 写进 `versions/<id>/<id>.json`（模拟「父版本已装好」）。
pub fn put_version(layout: &GameLayout, id: &str, json: &str) {
    let path = layout.version_json(id);
    std::fs::create_dir_all(path.parent().expect("版本目录")).expect("建版本目录");
    std::fs::write(&path, json).expect("写版本 JSON");
}

/// 读盘上的版本 JSON。
pub fn read_version(layout: &GameLayout, id: &str) -> VersionJson {
    let path = layout.version_json(id);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读不到 {}：{error}", path.display()));
    VersionJson::from_json_str(&text).expect("版本 JSON 应可解析")
}

/// 按计划把每个待下文件写成占位字节（`execute` 的跳过判据 = 无 sha1 且存在即信）。
pub fn prefill(plan: &unlucky_core::game::install::InstallPlan) -> usize {
    for file in &plan.files {
        std::fs::create_dir_all(file.dest.parent().expect("文件父目录")).expect("建库目录");
        std::fs::write(&file.dest, b"placeholder").expect("写占位文件");
    }
    plan.files.len()
}
