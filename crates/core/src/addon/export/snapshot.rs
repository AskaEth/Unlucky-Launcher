//! 实例快照：`game` 与 `dependencies` 的唯一来源（**不解析实例 id 字符串**）。
//!
//! # 为什么要读版本 JSON 而不是拆 `versions/<id>` 的名字
//!
//! 实例 id 的形状是社区约定，不是规范：`1.20.1-fabric-0.15.11`（本仓库 `LoaderKind::version_id`
//! 的拼法）、`1.20.1-forge-47.2.0`、`Fabric 1.20.1`、`我的整合包 v3`……按 `-` 切串去猜
//! "哪段是加载器、哪段是版本"必然在真实实例上翻车。**证据只能在版本 JSON 里**：
//!
//! - 游戏版本 = `inheritsFrom` 链**最深的那个祖先**的 `id`（加载器层都叠在原版之上）；
//! - 加载器 = 各层 `libraries[]` 里的 Maven 坐标：`net.fabricmc:fabric-loader:<ver>`、
//!   `org.quiltmc:quilt-loader:<ver>`、`net.minecraftforge:forge:<ver>`、
//!   `net.neoforged:neoforge:<ver>`（`fmlloader` 也认，它是 Forge 的拆分包）。
//!
//! 这条口径由 tower 批准（M112）："解析 `<mc>-<loader>-<ver>` 字符串是猜"。
//!
//! # 版本号里的 MC 前缀
//!
//! NeoForge 在 MC 1.20.1 上的库坐标版本是 `1.20.1-47.1.106`（CF 导出的
//! `neoforge-1.20.1-47.1.106` 同源）。这里统一剥成 `47.1.106`：mrpack 的
//! `dependencies["neoforge"]` 要裸版本，CF 的 `modLoaders[].id` 由
//! `PackLoader::curse_id_for` 在 1.20.1 上再把前缀拼回去。
//!
//! # 没有加载器是合法的
//!
//! 原版实例的 `dependencies` 只有 `minecraft`——那不是"漏了加载器"，是真的没装。任务书说的
//! 「不得只写 `minecraft` 漏加载器」针对的是**装了加载器却漏写**，两者靠这条链区分得清清楚楚。

use std::path::Path;

use crate::addon::pack::error::{PackError, PackResult};
use crate::addon::LoaderKind;
use crate::game::library::MavenCoordinate;
use crate::game::manifest::VersionJson;

/// 实例里认出的一条加载器依赖。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotLoader {
    /// 加载器种类。
    pub kind: LoaderKind,
    /// 加载器版本（已剥掉可能存在的 `<mc>-` 前缀）。
    pub version: String,
}

/// 实例快照：游戏版本 + 加载器集合（顺序即 `libraries[]` 里出现的顺序）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceSnapshot {
    /// 原版游戏版本（`1.20.1`）。
    pub game_version: String,
    /// 加载器依赖（原版实例为空）。
    pub loaders: Vec<SnapshotLoader>,
}

/// 读一个实例目录的快照。
///
/// `version_id` 给定时只读 `versions/<id>/<id>.json`；`None` 时取 `versions/` 里**继承链的叶子**
/// （真实实例有"原版 + 加载器层"两个目录，叶子才是实例自己）。叶子多于一个时无法判断，
/// 宁可报错也不猜。
pub fn read_instance_snapshot(
    root: &Path,
    version_id: Option<&str>,
) -> PackResult<InstanceSnapshot> {
    let versions_dir = root.join("versions");
    let id = match version_id {
        Some(id) => id.to_string(),
        None => instance_version_id(&versions_dir)?,
    };
    let layers = load_chain(&versions_dir, &id)?;

    let deepest = layers
        .last()
        .and_then(|layer| layer.id.clone())
        .ok_or_else(|| {
            PackError::metadata(format!("versions/{id}/{id}.json 的继承链末端没有 id"))
        })?;

    let mut loaders: Vec<SnapshotLoader> = Vec::new();
    for layer in &layers {
        for library in layer.libraries.iter().flatten() {
            let Ok(coordinate) = MavenCoordinate::parse(&library.name) else {
                continue;
            };
            let Some(kind) = loader_kind_of(&coordinate.group, &coordinate.artifact) else {
                continue;
            };
            if loaders.iter().any(|seen| seen.kind == kind) {
                continue;
            }
            loaders.push(SnapshotLoader {
                kind,
                version: strip_game_prefix(&deepest, &coordinate.version),
            });
        }
    }

    Ok(InstanceSnapshot {
        game_version: deepest,
        loaders,
    })
}

/// `versions/` 下"这个实例"的版本 id：**继承链的叶子**（没有被别的版本当作 `inheritsFrom`）。
///
/// 为什么不是"只有一个版本目录"：真实实例必然有"原版 + 加载器层"两个目录（`1.20.1` 与
/// `1.20.1-fabric-0.15.11`），叶子才是实例自己。这条判据不看目录名形状，只看 JSON 里的
/// `inheritsFrom` 关系——与"不解析实例 id 字符串"是同一条纪律。
fn instance_version_id(versions_dir: &Path) -> PackResult<String> {
    let entries = std::fs::read_dir(versions_dir).map_err(|error| {
        PackError::unsupported(format!(
            "实例里没有可读的 {}：{error}",
            versions_dir.display()
        ))
        .with_hint("导出需要实例的版本 JSON（versions/<id>/<id>.json 给出游戏版本与加载器）")
    })?;
    let mut versions: Vec<(String, Option<String>)> = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path().join(format!("{name}.json"));
        if !path.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|error| {
            PackError::metadata(format!("读版本 JSON {} 失败：{error}", path.display()))
        })?;
        let layer: VersionJson = serde_json::from_str(&text).map_err(|error| {
            PackError::metadata(format!("{} 不是合法版本 JSON：{error}", path.display()))
        })?;
        versions.push((name, layer.inherits_from));
    }
    if versions.is_empty() {
        return Err(
            PackError::unsupported("实例的 versions/ 下没有 <id>/<id>.json".to_string()).with_hint(
                "先装好游戏/加载器再导出；导出需要版本 JSON 才能写 game 与 dependencies",
            ),
        );
    }
    let referenced: Vec<&str> = versions
        .iter()
        .filter_map(|(_, parent)| parent.as_deref())
        .collect();
    let mut leaves: Vec<String> = versions
        .iter()
        .filter(|(id, _)| !referenced.contains(&id.as_str()))
        .map(|(id, _)| id.clone())
        .collect();
    leaves.sort();
    match leaves.len() {
        1 => Ok(leaves.remove(0)),
        _ => Err(PackError::unsupported(format!(
            "实例的 versions/ 下有 {} 条继承链末端（{}），无法判断哪个属于这个实例",
            leaves.len(),
            leaves.join(", ")
        ))
        .with_hint("调用方应显式给出 versionId（实例档案的 version_id）")),
    }
}

/// 从 `id` 起沿 `inheritsFrom` 收集各层（第一层是实例自己，最后一层是最深的祖先）。
fn load_chain(versions_dir: &Path, id: &str) -> PackResult<Vec<VersionJson>> {
    let mut layers: Vec<VersionJson> = Vec::new();
    let mut visited: Vec<String> = Vec::new();
    let mut current = Some(id.to_string());
    while let Some(next) = current {
        if visited.iter().any(|seen| seen == &next) {
            return Err(PackError::metadata(format!(
                "版本 JSON 的继承链成环：{} → {next}",
                visited.join(" → ")
            )));
        }
        visited.push(next.clone());
        let path = versions_dir.join(&next).join(format!("{next}.json"));
        let text = std::fs::read_to_string(&path).map_err(|error| {
            PackError::metadata(format!("读版本 JSON {} 失败：{error}", path.display()))
                .with_hint("继承链上的父版本也必须装在实例里")
        })?;
        let layer: VersionJson = serde_json::from_str(&text).map_err(|error| {
            PackError::metadata(format!("{} 不是合法版本 JSON：{error}", path.display()))
        })?;
        current = layer.inherits_from.clone();
        layers.push(layer);
    }
    Ok(layers)
}

/// Maven 坐标 → 加载器种类（认不出的返回 `None`，不影响别的库）。
fn loader_kind_of(group: &str, artifact: &str) -> Option<LoaderKind> {
    match (group, artifact) {
        ("net.fabricmc", "fabric-loader") => Some(LoaderKind::Fabric),
        ("org.quiltmc", "quilt-loader") => Some(LoaderKind::Quilt),
        ("net.minecraftforge", "forge" | "fmlloader") => Some(LoaderKind::Forge),
        ("net.neoforged", "neoforge") => Some(LoaderKind::NeoForge),
        _ => None,
    }
}

/// 剥掉版本号里又写了一遍的 `<mc>-` 前缀（`1.20.1-47.1.106` → `47.1.106`）。
fn strip_game_prefix(game_version: &str, version: &str) -> String {
    let prefix = format!("{game_version}-");
    version
        .strip_prefix(&prefix)
        .filter(|rest| !rest.is_empty())
        .unwrap_or(version)
        .to_string()
}
