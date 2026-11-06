//! 计划正确性：多级继承折叠后的文件清单、URL/sha1/落点逐项、JSON 形状。

use std::sync::atomic::{AtomicU64, Ordering};

use crate::game::arguments::Environment;
use crate::game::inherit::resolve;
use crate::game::install::assets::{AssetIndex, AssetObject};
use crate::game::install::plan::{FileRole, InstallPlan, PlanInput};
use crate::game::layout::GameLayout;
use crate::game::manifest::VersionJson;

fn temp_root(tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ull-install-plan-{}-{tag}-{unique}",
        std::process::id()
    ))
}

/// 三层继承：base（客户端 jar + 资源索引 + 库）← mid（库）← top（库）。
fn resolved_three_levels() -> VersionJson {
    let base = VersionJson::from_json_str(
        r#"{
            "id": "base",
            "mainClass": "net.minecraft.client.main.Main",
            "downloads": {"client": {"sha1": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "size": 100, "url": "https://piston-data.mojang.com/v1/objects/aa/client.jar"}},
            "assetIndex": {"id": "17", "sha1": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "size": 200, "totalSize": 300, "url": "https://piston-meta.mojang.com/v1/packages/bb/17.json"},
            "assets": "17",
            "libraries": [
                {"name": "org.ow2.asm:asm:9.6", "downloads": {"artifact": {"path": "org/ow2/asm/asm/9.6/asm-9.6.jar", "sha1": "c1", "size": 11, "url": "https://libraries.minecraft.net/org/ow2/asm/asm/9.6/asm-9.6.jar"}}},
                {"name": "org.lwjgl:lwjgl:3.3.2", "rules": [{"action": "allow", "os": {"name": "linux"}}], "downloads": {"artifact": {"url": "https://libraries.minecraft.net/lwjgl-linux-only.jar", "sha1": "c2", "size": 5}}}
            ]
        }"#,
    )
    .expect("base 清单应可解析");
    let mid = VersionJson::from_json_str(
        r#"{"id": "mid", "inheritsFrom": "base", "libraries": [
            {"name": "net.minecraftforge:forge:1.20.1-47.2.0", "downloads": {"artifact": {"path": "net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0.jar", "sha1": "c3", "size": 22, "url": "https://maven.minecraftforge.net/forge.jar"}}}
        ]}"#,
    )
    .expect("mid 清单应可解析");
    let top = VersionJson::from_json_str(
        r#"{"id": "top", "inheritsFrom": "mid", "libraries": [
            {"name": "top.patch:core:1.0", "downloads": {"artifact": {"path": "top/patch/core/1.0/core-1.0.jar", "sha1": "c4", "size": 33, "url": "https://example.invalid/core-1.0.jar"}}}
        ], "logging": {"client": {"argument": "-Dlog4j.configurationFile=${path}", "file": {"id": "client-1.12.xml", "sha1": "dd", "size": 44, "url": "https://piston-data.mojang.com/v1/objects/dd/client-1.12.xml"}}}}"#,
    )
    .expect("top 清单应可解析");

    let mut table = std::collections::BTreeMap::new();
    table.insert("base".to_string(), base);
    table.insert("mid".to_string(), mid);
    table.insert("top".to_string(), top.clone());
    resolve(&table, &top).expect("三层继承应解析成功")
}

fn asset_index() -> AssetIndex {
    let mut index = AssetIndex::default();
    index.objects.insert(
        "minecraft/lang/zh_cn.json".to_string(),
        AssetObject {
            hash: "0f9d6b1c0d5f4e3b2a1908070605040302010f0e".to_string(),
            size: 55,
        },
    );
    index
}

fn build(layout: &GameLayout, resolved: &VersionJson, index: Option<&AssetIndex>) -> InstallPlan {
    InstallPlan::build(PlanInput {
        layout,
        version_id: "top",
        resolved,
        asset_index: index,
        environment: &Environment::host(),
    })
    .expect("组计划应成功")
}

#[test]
fn folds_inheritance_chain_into_one_plan() {
    let root = temp_root("chain");
    let layout = GameLayout::new(&root);
    let resolved = resolved_three_levels();
    // 三层库都保留（父在前、子在后，去重后按角色/路径排序）。
    assert_eq!(resolved.libraries.as_ref().map(Vec::len), Some(4));

    let index = asset_index();
    let plan = build(&layout, &resolved, Some(&index));

    // 客户端 jar 1 + 库 4（linux 规则那条在 Windows 上被过滤）+ 索引 1 + 资源 1 + 日志配置 1。
    let host_linux = Environment::host().os == "linux";
    let expected_libraries = if host_linux { 4 } else { 3 };
    assert_eq!(
        plan.by_role()
            .get(&FileRole::ClientJar)
            .map(|entry| entry.0),
        Some(1)
    );
    assert_eq!(
        plan.by_role().get(&FileRole::Library).map(|entry| entry.0),
        Some(expected_libraries)
    );
    assert_eq!(
        plan.by_role()
            .get(&FileRole::AssetIndex)
            .map(|entry| entry.0),
        Some(1)
    );
    assert_eq!(
        plan.by_role().get(&FileRole::Asset).map(|entry| entry.0),
        Some(1)
    );
    assert_eq!(
        plan.by_role()
            .get(&FileRole::LogConfig)
            .map(|entry| entry.0),
        Some(1)
    );
    assert!(plan.warnings.is_empty(), "不应有告警：{:?}", plan.warnings);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn client_jar_lands_in_version_dir_with_manifest_hash() {
    let root = temp_root("jar");
    let layout = GameLayout::new(&root);
    let resolved = resolved_three_levels();
    let plan = build(&layout, &resolved, None);

    let jar = plan
        .files
        .iter()
        .find(|file| file.role == FileRole::ClientJar)
        .expect("应有客户端 jar");
    assert_eq!(plan.jar_id, "top");
    assert!(jar.dest.ends_with("versions/top/top.jar"));
    assert_eq!(
        jar.sha1.as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert_eq!(jar.size, Some(100));
    assert_eq!(
        jar.url,
        "https://piston-data.mojang.com/v1/objects/aa/client.jar"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn asset_object_paths_and_hashes_are_exact() {
    let root = temp_root("assets");
    let layout = GameLayout::new(&root);
    let resolved = resolved_three_levels();
    let index = asset_index();
    let plan = build(&layout, &resolved, Some(&index));

    let object = plan
        .files
        .iter()
        .find(|file| file.role == FileRole::Asset)
        .expect("应有资源对象");
    assert_eq!(
        object.url,
        "https://resources.download.minecraft.net/0f/0f9d6b1c0d5f4e3b2a1908070605040302010f0e"
    );
    assert_eq!(
        object.sha1.as_deref(),
        Some("0f9d6b1c0d5f4e3b2a1908070605040302010f0e")
    );
    assert_eq!(object.size, Some(55));
    assert!(object
        .dest
        .ends_with("assets/objects/0f/0f9d6b1c0d5f4e3b2a1908070605040302010f0e"));

    let index_file = plan
        .files
        .iter()
        .find(|file| file.role == FileRole::AssetIndex)
        .expect("应有资源索引");
    assert!(index_file.dest.ends_with("assets/indexes/17.json"));
    assert_eq!(
        index_file.sha1.as_deref(),
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn shares_libraries_across_versions_and_sums_totals() {
    let root = temp_root("totals");
    let layout = GameLayout::new(&root);
    let resolved = resolved_three_levels();
    let index = asset_index();
    let plan = build(&layout, &resolved, Some(&index));

    let (total, unknown) = plan.total_bytes();
    // jar 100 + 索引 200 + 资源 55 + 日志 44 + 各库大小（host 上生效的那几条）。
    assert_eq!(unknown, 0);
    assert!(total >= 399, "总大小应至少含 jar+索引+资源+日志：{total}");

    // 库落恒共享层（不是实例私有）。
    for file in plan
        .files
        .iter()
        .filter(|file| file.role == FileRole::Library)
    {
        assert!(file.dest.starts_with(root.join("libraries")));
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn warns_instead_of_inventing_urls_for_legacy_manifests() {
    let root = temp_root("legacy");
    let layout = GameLayout::new(&root);
    let legacy = VersionJson::from_json_str(r#"{"id": "c0.30_01c", "assets": "legacy"}"#)
        .expect("旧清单应可解析");
    let plan = build(&layout, &legacy, None);

    assert!(plan.files.is_empty(), "旧清单没有可下文件");
    assert_eq!(
        plan.warnings.len(),
        2,
        "应分别告警缺 jar 与缺索引：{:?}",
        plan.warnings
    );
    assert!(plan.warnings.iter().any(|w| w.contains("downloads.client")));
    assert!(plan.warnings.iter().any(|w| w.contains("assetIndex")));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn json_shape_is_stable_for_dry_run_consumers() {
    let root = temp_root("json");
    let layout = GameLayout::new(&root);
    let resolved = resolved_three_levels();
    let index = asset_index();
    let plan = build(&layout, &resolved, Some(&index));

    let summary = plan.summary_json();
    assert_eq!(summary["version_id"], "top");
    assert_eq!(summary["jar_id"], "top");
    assert_eq!(summary["file_count"], plan.file_count());
    assert!(summary["total_bytes"].as_u64().unwrap_or(0) > 0);
    assert!(summary["by_role"]["client_jar"]["files"].as_u64() == Some(1));
    assert!(summary["warnings"].is_array());

    let files = plan.files_json();
    assert_eq!(files.len(), plan.file_count());
    for row in &files {
        assert!(row["role"].as_str().is_some());
        assert!(row["url"].as_str().is_some());
        assert!(row["dest"].as_str().is_some());
        assert!(row["resource"].as_str().is_some());
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn deduplicates_same_destination_and_prefers_hashed_copy() {
    let root = temp_root("dedupe");
    let layout = GameLayout::new(&root);
    let resolved = VersionJson::from_json_str(
        r#"{"id": "1.20.1", "libraries": [
            {"name": "a.b:c:1", "downloads": {"artifact": {"path": "a/b/c/1/c-1.jar", "url": "https://libraries.minecraft.net/c-1.jar"}}},
            {"name": "a.b:c:1", "downloads": {"artifact": {"path": "a/b/c/1/c-1.jar", "sha1": "ff", "size": 9, "url": "https://libraries.minecraft.net/c-1.jar"}}}
        ]}"#,
    )
    .expect("清单应可解析");
    let plan = build(&layout, &resolved, None);

    let libraries: Vec<_> = plan
        .files
        .iter()
        .filter(|file| file.role == FileRole::Library)
        .collect();
    assert_eq!(libraries.len(), 1, "同一落点只应留一条");
    assert_eq!(
        libraries[0].sha1.as_deref(),
        Some("ff"),
        "应保留带 sha1 的那条"
    );
    std::fs::remove_dir_all(&root).ok();
}
