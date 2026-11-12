//! 计划形状 / 继承链两级解析 / 镜像选择。

use std::collections::BTreeMap;

use super::{FABRIC_PROFILE, VANILLA_1201};
use crate::addon::loader::fabric::{build_release, loader_list_url, profile_url};
use crate::addon::loader::maven;
use crate::addon::loader::{
    LoaderInstallPlan, LoaderKind, LoaderPlanInput, LoaderRelease, LoaderSupport,
};
use crate::download::mirrors::MirrorTable;
use crate::download::region::RegionProfile;
use crate::download::types::ResourceKind;
use crate::game::arguments::Environment;
use crate::game::install::FileRole;
use crate::game::{inherit, layout::GameLayout, VersionJson};

fn fabric_release() -> LoaderRelease {
    build_release(LoaderKind::Fabric, "1.20.1", "0.15.11", FABRIC_PROFILE).expect("夹具应可归一化")
}

fn environment() -> Environment {
    Environment {
        os: "windows".to_string(),
        os_version: String::new(),
        arch: "x86_64".to_string(),
        ..Environment::host()
    }
}

#[test]
fn plan_contains_only_loader_libraries() {
    let layout = GameLayout::new("/g/.minecraft");
    let release = fabric_release();
    let plan = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &release,
        environment: &environment(),
        parent_installed: false,
    })
    .expect("计划应可组");

    assert_eq!(plan.install.version_id, "1.20.1-fabric-0.15.11");
    // jar 与资源归原版层，loader 计划里只有库。
    assert_eq!(plan.install.jar_id, "1.20.1");
    assert!(plan
        .files()
        .iter()
        .all(|file| file.role == FileRole::Library));
    assert!(
        plan.files()
            .iter()
            .any(|file| file.name.contains("fabric-loader")),
        "缺 fabric-loader：{:?}",
        plan.files().iter().map(|f| &f.name).collect::<Vec<_>>()
    );
    assert!(plan.install.version_json_path.ends_with(
        std::path::Path::new("versions")
            .join("1.20.1-fabric-0.15.11")
            .join("1.20.1-fabric-0.15.11.json")
    ));
    // 只下 loader 那几个库，不该冒出资产/客户端 jar。
    assert!(plan.install.file_count() >= 4);
    assert!(plan.install.client_jar().is_none());
}

#[test]
fn plan_marks_missing_vanilla_and_unsupported() {
    let layout = GameLayout::new("/g/.minecraft");
    let release = fabric_release();
    let plan = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &release,
        environment: &environment(),
        parent_installed: false,
    })
    .expect("计划应可组");
    assert!(plan.vanilla_install_needed);
    assert!(plan
        .warnings()
        .iter()
        .any(|warning| warning.contains("原版 1.20.1 尚未安装")));

    let installed = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &release,
        environment: &environment(),
        parent_installed: true,
    })
    .expect("计划应可组");
    assert!(!installed.vanilla_install_needed);

    // 把 support 改成"需要 installer"，计划里要出现"要外调 java"的告警（M35 起可真装）。
    let mut blocked = fabric_release();
    blocked.support = LoaderSupport::RequiresInstaller {
        reason: "需要 processors".to_string(),
    };
    let plan = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &blocked,
        environment: &environment(),
        parent_installed: true,
    })
    .expect("计划应可组");
    assert!(plan
        .warnings()
        .iter()
        .any(|warning| warning.contains("需要外调 java 跑 installer processors")));
}

#[test]
fn summary_json_carries_loader_block() {
    let layout = GameLayout::new("/g/.minecraft");
    let release = fabric_release();
    let plan = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &release,
        environment: &environment(),
        parent_installed: true,
    })
    .expect("计划应可组");
    let summary = plan.summary_json();
    assert_eq!(summary["loader"]["kind"], "fabric");
    assert_eq!(summary["loader"]["version_id"], "1.20.1-fabric-0.15.11");
    assert_eq!(summary["loader"]["support"]["mode"], "json_route");
    assert_eq!(summary["loader"]["inherits_from"], "1.20.1");
    assert_eq!(summary["vanilla_install_needed"], false);
}

#[test]
fn loader_json_resolves_two_level_chain() {
    // loader → 原版 两级继承：解析后必须是扁平的、jar 指原版、库两侧合并。
    let vanilla = VersionJson::from_json_str(VANILLA_1201).expect("原版夹具");
    let loader = fabric_release().version_json;
    let mut table: BTreeMap<String, VersionJson> = BTreeMap::new();
    table.insert("1.20.1".to_string(), vanilla);
    table.insert("1.20.1-fabric-0.15.11".to_string(), loader.clone());

    let resolved = inherit::resolve(&table, &loader).expect("两级链应可解析");
    assert!(resolved.is_resolved());
    assert_eq!(resolved.inherits_from, None);
    assert_eq!(resolved.jar.as_deref(), Some("1.20.1"));
    assert_eq!(
        resolved.main_class.as_deref(),
        Some("net.fabricmc.loader.impl.launch.knot.KnotClient")
    );
    // 资源来自原版层。
    assert_eq!(resolved.assets.as_deref(), Some("5"));
    assert!(resolved.client_jar().is_some());
    // 库两层的并集（父在前，子在后）。
    let libraries = resolved.libraries.expect("应有库");
    assert_eq!(libraries.len(), 5);
    assert_eq!(libraries[0].name, "com.mojang:brigadier:1.0.18");
}

#[test]
fn loader_meta_and_maven_urls_hit_bmclapi_mirrors() {
    let table = MirrorTable::builtin();
    let mirror_first = RegionProfile::mirror_first(0);

    let fabric_meta = loader_list_url(LoaderKind::Fabric, "1.20.1");
    let candidates = table.expand(&fabric_meta, ResourceKind::VersionManifest, &mirror_first);
    let sources: Vec<&str> = candidates.iter().map(|c| c.source.as_str()).collect();
    assert_eq!(
        sources.first(),
        Some(&"bmclapi"),
        "镜像优先时应先走 BMCLAPI"
    );
    assert!(candidates.iter().any(|candidate| candidate
        .url
        .contains("/fabric-meta/v2/versions/loader/1.20.1")));

    let quilt_profile = profile_url(LoaderKind::Quilt, "1.20.1", "0.20.0-beta.9");
    let candidates = table.expand(&quilt_profile, ResourceKind::VersionManifest, &mirror_first);
    assert_eq!(candidates[0].source, "bmclapi");
    assert!(candidates[0].url.contains("/quilt-meta/"));

    let neoforge_installer =
        maven::installer_url(LoaderKind::NeoForge, "1.21.1", "21.1.80").expect("maven 族应有地址");
    let candidates = table.expand(&neoforge_installer, ResourceKind::Library, &mirror_first);
    assert_eq!(candidates[0].source, "bmclapi");
    assert!(candidates[0]
        .url
        .starts_with("https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge/"));

    let forge_meta = maven::metadata_url(LoaderKind::Forge, "1.20.1").expect("maven 族应有地址");
    let candidates = table.expand(&forge_meta, ResourceKind::VersionManifest, &mirror_first);
    assert_eq!(candidates[0].source, "bmclapi");
    assert!(candidates[0]
        .url
        .starts_with("https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/"));

    // 直连模式：官方在前，镜像只兜底。
    let direct = RegionProfile::direct(0);
    let candidates = table.expand(&fabric_meta, ResourceKind::VersionManifest, &direct);
    assert_eq!(candidates[0].source, "official");
    assert_eq!(candidates[0].url, fabric_meta);
}
