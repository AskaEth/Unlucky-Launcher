//! maven 版本表：XML 扫描、NeoForge 版本反解、版本挑选、installer 地址。

use crate::addon::loader::maven::{
    display_loader_version, installer_url, matches_mc, metadata_url, neoforge_mc_version,
    normalize_maven_version, parse_metadata_versions, select_versions,
};
use crate::addon::loader::LoaderKind;

/// 裁剪过的 maven-metadata.xml（真实结构，只留 `<version>` 列表）。
const METADATA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>net.neoforged</groupId>
  <artifactId>neoforge</artifactId>
  <versioning>
    <latest>26.1.2.125</latest>
    <versions>
      <version>20.4.237</version>
      <version>21.1.9</version>
      <version>21.1.80</version>
      <version>26.1.2.125</version>
      <version>0.0.1-snapshot-1</version>
    </versions>
  </versioning>
</metadata>"#;

#[test]
fn metadata_versions_are_scanned() {
    let versions = parse_metadata_versions(METADATA);
    assert_eq!(
        versions,
        vec![
            "20.4.237",
            "21.1.9",
            "21.1.80",
            "26.1.2.125",
            "0.0.1-snapshot-1"
        ]
    );
    assert!(parse_metadata_versions("not xml").is_empty());
}

#[test]
fn neoforge_versions_map_back_to_minecraft() {
    // analyse/05 §1.3 的三条规则。
    assert_eq!(neoforge_mc_version("21.1.80").as_deref(), Some("1.21.1"));
    assert_eq!(neoforge_mc_version("20.4.237").as_deref(), Some("1.20.4"));
    assert_eq!(neoforge_mc_version("26.1.2.125").as_deref(), Some("26.1"));
    // 快照（主版本 0）不挂原版版本号。
    assert_eq!(neoforge_mc_version("0.0.1-snapshot-1"), None);
}

#[test]
fn select_versions_filters_and_sorts_numerically() {
    let versions = parse_metadata_versions(METADATA);
    let picked = select_versions(LoaderKind::NeoForge, "1.21.1", &versions);
    // 数字序：9 < 80（字符串序会反过来）。
    assert_eq!(picked, vec!["21.1.9", "21.1.80"]);
    assert!(select_versions(LoaderKind::NeoForge, "1.19.2", &versions).is_empty());
}

#[test]
fn forge_versions_carry_the_minecraft_prefix() {
    let versions = vec![
        "1.20.1-47.3.0".to_string(),
        "1.20.1-47.2.0".to_string(),
        "1.12.2-14.23.5.2860".to_string(),
    ];
    let picked = select_versions(LoaderKind::Forge, "1.20.1", &versions);
    assert_eq!(picked, vec!["1.20.1-47.2.0", "1.20.1-47.3.0"]);
    assert!(matches_mc(LoaderKind::Forge, "1.20.1", "1.20.1-47.3.0"));
    assert!(!matches_mc(LoaderKind::Forge, "1.20.2", "1.20.1-47.3.0"));
}

#[test]
fn neoforge_1201_uses_forge_layout() {
    // 1.20.1 的 NeoForge 早产版挂在 net/neoforged/forge 下，版本串带原版前缀。
    assert!(metadata_url(LoaderKind::NeoForge, "1.20.1")
        .expect("maven 族应有地址")
        .ends_with("net/neoforged/forge/maven-metadata.xml"));
    assert!(metadata_url(LoaderKind::NeoForge, "1.21.1")
        .expect("maven 族应有地址")
        .ends_with("net/neoforged/neoforge/maven-metadata.xml"));
    assert!(metadata_url(LoaderKind::Forge, "1.20.1")
        .expect("maven 族应有地址")
        .ends_with("net/minecraftforge/forge/maven-metadata.xml"));

    let versions = vec!["1.20.1-47.1.106".to_string(), "1.20.1-47.1.90".to_string()];
    let picked = select_versions(LoaderKind::NeoForge, "1.20.1", &versions);
    assert_eq!(picked.len(), 2);
    assert_eq!(
        display_loader_version(LoaderKind::NeoForge, "1.20.1", "1.20.1-47.1.106"),
        "47.1.106"
    );
}

#[test]
fn versions_are_normalized_and_displayed() {
    // Forge：用户给 "47.2.0"，maven 上是 "1.20.1-47.2.0"。
    assert_eq!(
        normalize_maven_version(LoaderKind::Forge, "1.20.1", "47.2.0"),
        "1.20.1-47.2.0"
    );
    assert_eq!(
        normalize_maven_version(LoaderKind::Forge, "1.20.1", "1.20.1-47.2.0"),
        "1.20.1-47.2.0"
    );
    assert_eq!(
        display_loader_version(LoaderKind::Forge, "1.20.1", "1.20.1-47.2.0"),
        "47.2.0"
    );
    // NeoForge（非 1.20.1 布局）本身就是完整 maven 版本。
    assert_eq!(
        normalize_maven_version(LoaderKind::NeoForge, "1.21.1", "21.1.80"),
        "21.1.80"
    );
    assert_eq!(
        display_loader_version(LoaderKind::NeoForge, "1.21.1", "21.1.80"),
        "21.1.80"
    );
}

#[test]
fn installer_urls_point_at_official_maven() {
    assert_eq!(
        installer_url(LoaderKind::Forge, "1.20.1", "1.20.1-47.2.0").expect("maven 族应有地址"),
        "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-installer.jar"
    );
    assert_eq!(
        installer_url(LoaderKind::NeoForge, "1.21.1", "21.1.80").expect("maven 族应有地址"),
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.80/neoforge-21.1.80-installer.jar"
    );
    // 1.20.1 的 NeoForge 走 forge 布局（文件名也是 forge-）。
    assert_eq!(
        installer_url(LoaderKind::NeoForge, "1.20.1", "1.20.1-47.1.106").expect("maven 族应有地址"),
        "https://maven.neoforged.net/releases/net/neoforged/forge/1.20.1-47.1.106/forge-1.20.1-47.1.106-installer.jar"
    );
}
