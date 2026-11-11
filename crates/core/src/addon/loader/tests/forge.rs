//! NeoForge / Forge：install profile 解析与三条路线的判定。

use crate::addon::loader::forge::{build_release, InstallProfile};
use crate::addon::loader::{LoaderErrorKind, LoaderKind, LoaderSupport};
use crate::game::manifest::VersionJson;

/// spec 1 + client 侧 processor（实测 NeoForge 21.1.80 / Forge 1.20.1 的形状，裁剪过）。
pub(crate) const NEOFORGE_PROFILE: &str = r#"{
  "spec": 1,
  "profile": "NeoForge",
  "version": "neoforge-21.1.80",
  "minecraft": "1.21.1",
  "json": "/version.json",
  "path": null,
  "processors": [
    {"sides": ["server"], "jar": "net.neoforged.installertools:installertools:2.1.2"},
    {"sides": ["client"], "jar": "net.neoforged.installertools:installertools:2.1.2"},
    {"jar": "net.neoforged.installertools:installertools:2.1.2"},
    {"sides": ["client"], "jar": "net.neoforged.installertools:installertools:2.1.2"}
  ],
  "libraries": [
    {"name": "com.google.code.gson:gson:2.8.9@jar"},
    {"name": "org.apache.commons:commons-text:1.3@jar"}
  ],
  "data": {"MC_SLIM": {"client": "[net.neoforged:neoforge:21.1.80:client-slim]"}}
}"#;

/// spec 1 版本 JSON 的裁剪版（内容形状照 21.1.80）。
pub(crate) const NEOFORGE_VERSION_JSON: &str = r#"{
  "id": "neoforge-21.1.80",
  "inheritsFrom": "1.21.1",
  "type": "release",
  "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
  "arguments": {"game": ["--launchTarget", "forgeclient"], "jvm": ["-DignoreList=bootstraplauncher"]},
  "libraries": [
    {"name": "cpw.mods:securejarhandler:2.1.10",
     "downloads": {"artifact": {"path": "cpw/mods/securejarhandler/2.1.10/securejarhandler-2.1.10.jar",
       "url": "https://maven.neoforged.net/releases/cpw/mods/securejarhandler/2.1.10/securejarhandler-2.1.10.jar",
       "sha1": "51e6a22c6c716beb11e244bf5b8be480f51dd6b5", "size": 88749}}}
  ]
}"#;

/// spec 0 + 无 processors（实测 Forge 1.12.2-14.23.5.2860），universal jar 的 url 是空串。
pub(crate) const FORGE_LEGACY_PROFILE: &str = r#"{
  "spec": 0,
  "profile": "forge",
  "version": "1.12.2-forge-14.23.5.2860",
  "minecraft": "1.12.2",
  "json": "/version.json",
  "path": "net.minecraftforge:forge:1.12.2-14.23.5.2860",
  "processors": [],
  "libraries": [{"name": "net.minecraftforge:forge:1.12.2-14.23.5.2860"}]
}"#;

pub(crate) const FORGE_LEGACY_VERSION_JSON: &str = r#"{
  "id": "1.12.2-forge-14.23.5.2860",
  "inheritsFrom": "1.12.2",
  "mainClass": "net.minecraft.launchwrapper.Launch",
  "minecraftArguments": "--username ${auth_player_name} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker",
  "libraries": [
    {"name": "net.minecraftforge:forge:1.12.2-14.23.5.2860",
     "downloads": {"artifact": {"path": "net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860.jar",
       "url": "", "sha1": "029250575d3aa2cf80b56dffb66238a1eeaea2ac", "size": 4466148}}}
  ]
}"#;

fn profile(json: &str) -> InstallProfile {
    serde_json::from_str(json).expect("夹具 profile 应可解析")
}

fn version_json(json: &str) -> VersionJson {
    VersionJson::from_json_str(json).expect("夹具版本 JSON 应可解析")
}

#[test]
fn client_processors_mark_version_as_installer_required() {
    let release = build_release(
        LoaderKind::NeoForge,
        "1.21.1",
        "21.1.80",
        &profile(NEOFORGE_PROFILE),
        Some(&version_json(NEOFORGE_VERSION_JSON)),
    )
    .expect("版本 JSON 齐备时应能生成清单");

    // 2 条 sides 含 client + 1 条无 sides（两侧都要）= 3 条 client 侧。
    assert_eq!(profile(NEOFORGE_PROFILE).client_processors(), 3);
    // M35 起 RequiresInstaller 是可真装的路线（外调 java 跑 processors），
    // 但"要跑 installer"这个事实仍要能被上层看见。
    assert!(release.is_installable());
    assert!(release.requires_installer());
    let reason = release
        .installer_reason()
        .expect("应给需要 installer 的原因");
    assert!(reason.contains("3 个 client 侧 processor"), "{reason}");
    // 版本清单本身仍然正确生成（--dry-run 能看到它）。
    assert_eq!(release.version_id, "1.21.1-neoforge-21.1.80");
    assert_eq!(
        release.version_json.inherits_from.as_deref(),
        Some("1.21.1")
    );
    assert_eq!(release.version_json.jar.as_deref(), Some("1.21.1"));
    // processor 依赖不进启动 classpath，只记数量进告警。
    assert_eq!(release.version_json.libraries.map_or(0, |l| l.len()), 1);
    assert!(release
        .warnings
        .iter()
        .any(|warning| warning.contains("processor 依赖")));
}

#[test]
fn spec_zero_profile_is_json_route_and_fills_empty_url() {
    let release = build_release(
        LoaderKind::Forge,
        "1.12.2",
        "1.12.2-14.23.5.2860",
        &profile(FORGE_LEGACY_PROFILE),
        Some(&version_json(FORGE_LEGACY_VERSION_JSON)),
    )
    .expect("spec 0 应走 json 路线");

    assert_eq!(release.support, LoaderSupport::JsonRoute);
    assert_eq!(release.loader_version, "14.23.5.2860");
    assert_eq!(release.version_id, "1.12.2-forge-14.23.5.2860");
    assert_eq!(
        release.version_json.main_class.as_deref(),
        Some("net.minecraft.launchwrapper.Launch")
    );

    // 空 url 必须补成 maven 地址：本体 jar 在官方 maven 上**只发布在 -universal classifier 下**
    // （无 classifier 路径官方 404，仅靠 BMCLAPI 302 别名兜底），所以 URL 要带 -universal。
    let library = release
        .version_json
        .libraries
        .as_ref()
        .and_then(|libraries| libraries.first())
        .expect("应有 forge 本体库");
    let artifact = library
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.artifact.as_ref())
        .expect("应有 artifact");
    assert_eq!(
        artifact.url.as_deref(),
        Some(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860-universal.jar"
        ),
        "URL 必须带 -universal（官方 maven 只托管这个 classifier）"
    );
    assert_eq!(artifact.size, Some(4_466_148));
    assert!(library
        .url
        .as_deref()
        .unwrap_or_default()
        .contains("minecraftforge"));

    // 磁盘落点仍用原 path（与官方 installer 内嵌条目同名，启动 classpath 布局不变）。
    assert_eq!(
        artifact.path.as_deref(),
        Some("net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860.jar")
    );
    let dest = library.relative_path().expect("落点应可推导");
    assert!(
        dest.ends_with("forge-1.12.2-14.23.5.2860.jar"),
        "落点不该带 classifier：{}",
        dest.display()
    );
}

#[test]
fn legacy_inline_protocol_is_unsupported() {
    // 老协议：没有独立 version.json，只有内联 versionInfo + 解包信息。
    let profile = profile(
        r#"{"spec": 1, "minecraft": "1.7.10",
            "install": {"filePath": "forge-1.7.10-10.13.4.1614-universal.jar"},
            "versionInfo": {"id": "1.7.10-Forge10.13.4.1614", "mainClass": "net.minecraft.launchwrapper.Launch"},
            "processors": []}"#,
    );
    assert!(profile.is_legacy_protocol());
    let error = build_release(
        LoaderKind::Forge,
        "1.7.10",
        "1.7.10-10.13.4.1614",
        &profile,
        None,
    )
    .expect_err("老协议必须明确 unsupported");
    assert_eq!(error.kind(), LoaderErrorKind::Unsupported);
    assert!(error.message().contains("老协议"), "{}", error.message());
    assert!(error.hint().is_some());
}

#[test]
fn universal_classifier_rewrite_is_scoped_to_the_installer_body() {
    // 只有 install profile 的 `path` 声明的本体构件走 -universal；
    // 其它同样"空 url + 有 path"的库必须保持原路径（乱加 classifier 会 404）。
    let embedded = version_json(
        r#"{"id": "1.12.2-forge-14.23.5.2860", "inheritsFrom": "1.12.2",
            "mainClass": "net.minecraft.launchwrapper.Launch",
            "libraries": [
              {"name": "net.minecraftforge:forge:1.12.2-14.23.5.2860",
               "downloads": {"artifact": {"path": "net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860.jar", "url": ""}}},
              {"name": "net.minecraftforge:mergetool:1.1.5",
               "downloads": {"artifact": {"path": "net/minecraftforge/mergetool/1.1.5/mergetool-1.1.5.jar", "url": ""}}}
            ]}"#,
    );
    let release = build_release(
        LoaderKind::Forge,
        "1.12.2",
        "1.12.2-14.23.5.2860",
        &profile(FORGE_LEGACY_PROFILE),
        Some(&embedded),
    )
    .expect("spec 0 应走 json 路线");
    let libraries = release.version_json.libraries.expect("应有库");
    let url_of = |name: &str| {
        libraries
            .iter()
            .find(|library| library.name == name)
            .and_then(|library| library.downloads.as_ref())
            .and_then(|downloads| downloads.artifact.as_ref())
            .and_then(|artifact| artifact.url.clone())
            .unwrap_or_default()
    };
    assert!(url_of("net.minecraftforge:forge:1.12.2-14.23.5.2860").ends_with("-universal.jar"));
    assert_eq!(
        url_of("net.minecraftforge:mergetool:1.1.5"),
        "https://maven.minecraftforge.net/net/minecraftforge/mergetool/1.1.5/mergetool-1.1.5.jar"
    );
}

#[test]
fn version_mismatch_between_installer_and_target_is_metadata_error() {
    let error = build_release(
        LoaderKind::NeoForge,
        "1.20.1",
        "21.1.80",
        &profile(NEOFORGE_PROFILE),
        Some(&version_json(NEOFORGE_VERSION_JSON)),
    )
    .expect_err("原版版本不符必须报错");
    assert_eq!(error.kind(), LoaderErrorKind::Metadata);
    assert!(error.message().contains("1.21.1"), "{}", error.message());
}

#[test]
fn missing_main_class_is_metadata_error() {
    let embedded = version_json(r#"{"id": "x", "inheritsFrom": "1.12.2", "libraries": []}"#);
    let error = build_release(
        LoaderKind::Forge,
        "1.12.2",
        "1.12.2-14.23.5.2860",
        &profile(FORGE_LEGACY_PROFILE),
        Some(&embedded),
    )
    .expect_err("缺主类必须报错");
    assert_eq!(error.kind(), LoaderErrorKind::Metadata);
}
