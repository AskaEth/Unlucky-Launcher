//! 导出测试的共用夹具（M112）：实例目录、`versions/` 版本层、假的远端目录。
//!
//! 这里的东西**只给测试用**：本仓库没有 Modrinth/CF 客户端（要网络与 key），所以远端查表一律
//! 由 [`StubCatalog`] 注入；实例目录手造成"M111 导入产物"的形状（内容目录 + 版本 JSON）。

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

use super::{write_file, TempRoot};
use crate::addon::export::catalog::{RemoteCatalog, RemoteHit};
use crate::addon::export::scan::{scan_instance, DEFAULT_CONTENT_ROOTS};
use crate::addon::pack::curseforge::fingerprint;

pub(super) const VANILLA_LAYER: &str = r#"{"id": "1.20.1", "libraries": []}"#;
pub(super) const FABRIC_LAYER: &str = r#"{
  "id": "1.20.1-fabric-0.15.11",
  "inheritsFrom": "1.20.1",
  "libraries": [{"name": "net.fabricmc:fabric-loader:0.15.11"}]
}"#;
pub(super) const NEOFORGE_LAYER: &str = r#"{
  "id": "1.20.1-neoforge-47.1.106",
  "inheritsFrom": "1.20.1",
  "libraries": [
    {"name": "net.neoforged:neoforge:1.20.1-47.1.106"},
    {"name": "net.fancymodloader:loader:3.0.5"}
  ]
}"#;
pub(super) const FORGE_LAYER: &str = r#"{
  "id": "1.20.1-forge-47.2.0",
  "inheritsFrom": "1.20.1",
  "libraries": [{"name": "net.minecraftforge:forge:1.20.1-47.2.0:universal"}]
}"#;

/// 内容目录清单（与导出默认值同一份）。
pub(super) fn defaults() -> Vec<String> {
    DEFAULT_CONTENT_ROOTS.map(str::to_string).to_vec()
}

/// 写一份 `<id>/<id>.json`。
pub(super) fn write_layer(game: &Path, id: &str, json: &str) {
    write_file(
        &game.join(format!("versions/{id}/{id}.json")),
        json.as_bytes(),
    );
}

/// 一个 Fabric 实例目录：两个 mod（其中一个禁用）+ 一份 config + 一份白名单外的日志。
pub(super) fn fabric_instance(tag: &str) -> TempRoot {
    // 返回守卫本身（`Deref` 到 `<root>/game`）：实例目录活到调用点用完为止，drop 即删。
    let root = TempRoot::under(tag, "game");
    write_layer(&root, "1.20.1", VANILLA_LAYER);
    write_layer(&root, "1.20.1-fabric-0.15.11", FABRIC_LAYER);
    write_file(&root.join("mods/jei.jar"), b"PK\x03\x04 jei body");
    write_file(&root.join("mods/off.jar.disabled"), b"PK\x03\x04 off body");
    write_file(&root.join("config/known.toml"), b"key = \"value\"\n");
    write_file(&root.join("logs/latest.log"), b"not pack content\n");
    root
}

/// 一个 NeoForge 1.20.1 实例目录（CF `modLoaders[].id` 的 MC 前缀特例用它）。
pub(super) fn neoforge_instance(tag: &str) -> TempRoot {
    let root = TempRoot::under(tag, "game");
    write_layer(&root, "1.20.1", VANILLA_LAYER);
    write_layer(&root, "1.20.1-neoforge-47.1.106", NEOFORGE_LAYER);
    write_file(&root.join("mods/jei.jar"), b"PK\x03\x04 jei body");
    root
}

/// 目录里的一个文件（路径 + SHA-1 + CF 指纹），与 `scan_instance` 同源。
pub(super) fn digests(root: &Path, path: &str) -> (String, u32) {
    let (locals, _) = scan_instance(root, &defaults());
    let local = locals
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("夹具里没有 {path}"));
    let value = fingerprint::fingerprint_file(&local.disk).expect("算指纹");
    (local.sha1.clone(), value)
}

/// 一个能写进 mrpack 的 Modrinth 命中（sha1 + sha512 + 直链）。
pub(super) fn modrinth_hit(sha1: &str, url: &str) -> RemoteHit {
    let mut hashes = BTreeMap::new();
    hashes.insert("sha1".to_string(), sha1.to_string());
    hashes.insert("sha512".to_string(), "a".repeat(128));
    RemoteHit::modrinth(vec![url.to_string()], hashes, Some(11))
}

/// 一个**假**的远端目录：查表结果由测试塞，调用过的指纹记下来（哨兵值那条分支靠它断言）。
#[derive(Default)]
pub(super) struct StubCatalog {
    modrinth: BTreeMap<String, RemoteHit>,
    curse: BTreeMap<u32, RemoteHit>,
    curse_calls: RefCell<Vec<u32>>,
}

impl StubCatalog {
    pub(super) fn with_modrinth(mut self, sha1: &str, hit: RemoteHit) -> Self {
        self.modrinth.insert(sha1.to_string(), hit);
        self
    }

    pub(super) fn with_curse(mut self, value: u32, hit: RemoteHit) -> Self {
        self.curse.insert(value, hit);
        self
    }

    pub(super) fn curse_calls(&self) -> Vec<u32> {
        self.curse_calls.borrow().clone()
    }
}

impl RemoteCatalog for StubCatalog {
    fn modrinth_by_sha1(&self, sha1: &str) -> Option<RemoteHit> {
        self.modrinth.get(sha1).cloned()
    }

    fn curseforge_by_fingerprint(&self, value: u32) -> Option<RemoteHit> {
        self.curse_calls.borrow_mut().push(value);
        self.curse.get(&value).cloned()
    }
}
