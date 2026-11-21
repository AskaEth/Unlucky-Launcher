//! `inheritsFrom` 解析：逐字段 merge 语义、patch 优先级、环检测、缺失父版本。

use unlucky_core::game::error::GameError;
use unlucky_core::game::inherit::{apply_patches, merge, resolve, sorted_patches};
use unlucky_core::game::version::ReleaseType;

use crate::helpers::{parse, source};

const VANILLA: &str = r#"{
  "id": "1.20.1",
  "type": "release",
  "releaseTime": "2023-06-12T13:25:51+00:00",
  "mainClass": "net.minecraft.client.main.Main",
  "minecraftArguments": "--username ${auth_player_name}",
  "assets": "5",
  "assetIndex": { "id": "5" },
  "complianceLevel": 1,
  "minimumLauncherVersion": 21,
  "javaVersion": { "component": "java-runtime-gamma", "majorVersion": 17 },
  "libraries": [ { "name": "lib:a:1" }, { "name": "lib:b:1" } ],
  "downloads": {
    "client": { "url": "https://example.invalid/client.jar", "sha1": "c", "size": 1 },
    "server": { "url": "https://example.invalid/server.jar", "sha1": "s", "size": 1 }
  },
  "arguments": { "game": ["--game-parent"], "jvm": ["-Dparent=1"] },
  "logging": { "client": { "argument": "-Dlog4j", "file": { "id": "client-1.12.xml" } } }
}"#;

const FORGE: &str = r#"{
  "id": "1.20.1-forge-47.2.0",
  "inheritsFrom": "1.20.1",
  "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
  "time": "2024-01-01T00:00:00+00:00",
  "minimumLauncherVersion": 14,
  "javaVersion": { "component": "java-runtime-delta", "majorVersion": 21 },
  "libraries": [ { "name": "net.minecraftforge:forge:1" } ],
  "downloads": { "server": { "url": "https://example.invalid/server2.jar" } },
  "arguments": { "game": ["--game-child"], "jvm": ["-Dchild=1"] }
}"#;

fn vanilla_and_forge(
) -> std::collections::BTreeMap<String, unlucky_core::game::manifest::VersionJson> {
    source(&[("1.20.1", VANILLA), ("1.20.1-forge-47.2.0", FORGE)])
}

#[test]
fn merge_table_follows_analyse_03() {
    let sources = vanilla_and_forge();
    let child = parse(FORGE);
    let resolved = resolve(&sources, &child).expect("继承链应可解析");

    // 子覆盖父。
    assert_eq!(
        resolved.main_class.as_deref(),
        Some("cpw.mods.bootstraplauncher.BootstrapLauncher")
    );
    // 子没写就继承父。
    assert_eq!(
        resolved.minecraft_arguments.as_deref(),
        Some("--username ${auth_player_name}")
    );
    assert_eq!(resolved.assets.as_deref(), Some("5"));
    assert_eq!(resolved.version_type, Some(ReleaseType::Release));

    // arguments 父在前、子在后追加。
    let arguments = resolved.arguments.as_ref().expect("arguments 应保留");
    assert_eq!(arguments.game.len(), 2);
    assert_eq!(arguments.jvm.len(), 2);

    // libraries 父在前、子在后。
    let libraries = resolved.libraries.as_ref().expect("libraries 应保留");
    let names: Vec<&str> = libraries.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["lib:a:1", "lib:b:1", "net.minecraftforge:forge:1"]
    );

    // downloads 整体覆盖：子只写 server，client 就没了（非直觉点 1，必须显式守门）。
    let downloads = resolved.downloads.as_ref().expect("downloads 应存在");
    assert!(downloads.client().is_none(), "整体覆盖会让父层 client 消失");
    assert_eq!(
        downloads.server().and_then(|s| s.url.as_deref()),
        Some("https://example.invalid/server2.jar")
    );

    // complianceLevel 只取子（非直觉点 2）：父层 1 被丢弃。
    assert_eq!(resolved.compliance_level, None);

    // javaVersion 子覆盖父。
    let java = resolved.java_version.as_ref().expect("javaVersion 应存在");
    assert_eq!(java.major_version, Some(21));

    // minimumLauncherVersion 取 max(21, 14)=21。
    assert_eq!(resolved.minimum_launcher_version, Some(21));

    // logging 子没有 → 继承父。
    assert!(resolved.logging.is_some());

    // 解析结果的不变量。
    assert_eq!(resolved.jar.as_deref(), Some("1.20.1-forge-47.2.0"));
    assert!(resolved.inherits_from.is_none());
    assert!(resolved.patches.is_none());
    assert_eq!(resolved.root, Some(true));
    assert!(resolved.is_resolved());
}

#[test]
fn three_level_chain_orders_ancestor_first() {
    const A: &str = r#"{ "id": "a", "mainClass": "a.Main", "libraries": [{ "name": "l:a:1" }] }"#;
    const B: &str = r#"{ "id": "b", "inheritsFrom": "a", "libraries": [{ "name": "l:b:1" }] }"#;
    const C: &str = r#"{ "id": "c", "inheritsFrom": "b", "mainClass": "c.Main",
                        "libraries": [{ "name": "l:c:1" }] }"#;
    let sources = source(&[("a", A), ("b", B), ("c", C)]);

    let resolved = resolve(&sources, &parse(C)).expect("三层链应可解析");

    assert_eq!(resolved.main_class.as_deref(), Some("c.Main"));
    let names: Vec<&str> = resolved
        .libraries
        .as_ref()
        .expect("libraries 应存在")
        .iter()
        .map(|l| l.name.as_str())
        .collect();
    assert_eq!(names, vec!["l:a:1", "l:b:1", "l:c:1"]);
}

#[test]
fn patch_priority_sorts_ascending() {
    let root = parse(
        r#"{
          "id": "patched",
          "inheritsFrom": "1.20.1",
          "patches": [
            { "id": "forge", "priority": 30000, "mainClass": "loader.Main",
              "libraries": [{ "name": "loader:a:1" }] },
            { "id": "base", "mainClass": "base.Main",
              "arguments": { "game": ["--base"] } }
          ]
        }"#,
    );
    let resolved = resolve(&vanilla_and_forge(), &root).expect("带 patches 的清单应可解析");

    // 无 priority 的补丁排最前（Integer.MIN_VALUE 语义），loader 后合并 → 覆盖 mainClass。
    assert_eq!(resolved.main_class.as_deref(), Some("loader.Main"));
    let libraries = resolved.libraries.as_ref().expect("libraries 应存在");
    assert_eq!(
        libraries.last().map(|l| l.name.as_str()),
        Some("loader:a:1")
    );

    let arguments = resolved.arguments.as_ref().expect("arguments 应存在");
    let flat: Vec<String> = arguments.game.iter().map(|a| format!("{a:?}")).collect();
    assert!(flat.iter().any(|a| a.contains("--base")));
}

#[test]
fn equal_priority_patches_keep_insertion_order() {
    let root = parse(
        r#"{
          "id": "patched",
          "patches": [
            { "id": "first", "priority": 10, "mainClass": "first.Main" },
            { "id": "second", "priority": 10, "mainClass": "second.Main" }
          ]
        }"#,
    );
    let resolved = resolve(&source(&[]), &root).expect("无继承的清单应可解析");
    assert_eq!(
        resolved.main_class.as_deref(),
        Some("second.Main"),
        "同优先级按插入顺序合并，后者覆盖前者"
    );
}

#[test]
fn sorted_patches_puts_missing_priority_first() {
    let patches = vec![
        parse(r#"{ "id": "loader", "priority": 30000 }"#),
        parse(r#"{ "id": "base" }"#),
        parse(r#"{ "id": "mc", "priority": 0 }"#),
    ];
    let order: Vec<&str> = sorted_patches(&patches)
        .iter()
        .filter_map(|p| p.id.as_deref())
        .collect();
    assert_eq!(order, vec!["base", "mc", "loader"]);
}

#[test]
fn apply_patches_stacks_layers() {
    let base =
        parse(r#"{ "id": "root", "mainClass": "base.Main", "libraries": [{ "name": "l:a:1" }] }"#);
    let patched = apply_patches(
        base,
        &[parse(
            r#"{ "id": "loader", "priority": 30000, "mainClass": "loader.Main",
                      "libraries": [{ "name": "l:loader:1" }] }"#,
        )],
    );
    assert_eq!(patched.main_class.as_deref(), Some("loader.Main"));
    let names: Vec<&str> = patched
        .libraries
        .as_ref()
        .expect("libraries 应存在")
        .iter()
        .map(|l| l.name.as_str())
        .collect();
    assert_eq!(names, vec!["l:a:1", "l:loader:1"]);
}

#[test]
fn circular_inheritance_is_reported() {
    let sources = source(&[
        ("a", r#"{ "id": "a", "inheritsFrom": "b" }"#),
        ("b", r#"{ "id": "b", "inheritsFrom": "a" }"#),
    ]);
    let error = resolve(&sources, &parse(r#"{ "id": "a", "inheritsFrom": "b" }"#))
        .expect_err("成环必须报错而不是静默降级");
    match error {
        GameError::CircularInheritance { chain } => {
            assert_eq!(chain, vec!["a", "b", "a"], "链要能看出环的位置");
        }
        other => panic!("期望 CircularInheritance，得到 {other:?}"),
    }
}

#[test]
fn missing_parent_is_reported() {
    let error = resolve(&source(&[]), &parse(FORGE)).expect_err("缺失父版本必须报错");
    match error {
        GameError::MissingParent {
            parent,
            required_by,
        } => {
            assert_eq!(parent, "1.20.1");
            assert_eq!(required_by, "1.20.1-forge-47.2.0");
        }
        other => panic!("期望 MissingParent，得到 {other:?}"),
    }
}

#[test]
fn manifest_without_id_is_rejected() {
    let error =
        resolve(&source(&[]), &parse(r#"{ "mainClass": "a.B" }"#)).expect_err("缺 id 应报错");
    assert!(matches!(error, GameError::MissingVersionId));
}

#[test]
fn merge_preserves_unknown_fields_with_child_winning() {
    let parent = parse(r#"{ "id": "p", "customParentOnly": 1, "shared": "parent" }"#);
    let child = parse(r#"{ "id": "c", "shared": "child" }"#);
    let merged = merge(&child, &parent);

    assert_eq!(
        merged.extra.get("customParentOnly"),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        merged.extra.get("shared"),
        Some(&serde_json::json!("child"))
    );
    assert_eq!(merged.id.as_deref(), Some("c"));
}
