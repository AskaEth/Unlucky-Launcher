//! 老协议（形状 B）与形状 C：1.12.2 之前的 Forge installer 没有独立 `version.json`。
//!
//! 夹具全是**实测 installer** 的 install_profile.json 裁剪版
//! （原包与逐条原始输出在 `DevDocs/artifacts/forge1122/`）：

use super::forge::profile;
use crate::addon::loader::forge::{build_release, InstallProfile};
use crate::addon::loader::{
    LoaderErrorKind, LoaderInstallPlan, LoaderKind, LoaderPlanInput, LoaderSupport,
};
use crate::game::arguments::Environment;
use crate::game::layout::GameLayout;

/// 老协议形状 B（**实测** Forge 1.12.2-14.23.5.2768 的 install_profile.json，裁剪过：
/// 去掉 `mirrorList`/`welcome`/`logo` 与 21 条库里的 17 条，`versionInfo` 的 `time`/`releaseTime`
/// 也去掉了）。特征：**没有** `json` 键、installer 里没有 `version.json`；
/// 版本主体是内联 `versionInfo`；本体坐标只在 `install.path`；库表里连 `downloads` 都没有。
pub(crate) const FORGE_2768_PROFILE: &str = r#"{
  "install": {
    "profileName": "forge",
    "target": "1.12.2-forge1.12.2-14.23.5.2768",
    "path": "net.minecraftforge:forge:1.12.2-14.23.5.2768",
    "version": "forge 1.12.2-14.23.5.2768",
    "filePath": "forge-1.12.2-14.23.5.2768-universal.jar",
    "welcome": "Welcome to the simple forge installer.",
    "minecraft": "1.12.2",
    "modList": "none"
  },
  "versionInfo": {
    "id": "1.12.2-forge1.12.2-14.23.5.2768",
    "type": "release",
    "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker",
    "mainClass": "net.minecraft.launchwrapper.Launch",
    "inheritsFrom": "1.12.2",
    "jar": "1.12.2",
    "libraries": [
      {"name": "net.minecraftforge:forge:1.12.2-14.23.5.2768", "url": "https://maven.minecraftforge.net/"},
      {"name": "net.minecraft:launchwrapper:1.12", "serverreq": true},
      {"name": "org.ow2.asm:asm-all:5.2", "url": "https://maven.minecraftforge.net/", "checksums": ["2ea49e08b876bbd33e0a7ce75c8f371d29e1f10a"], "serverreq": true, "clientreq": true}
    ]
  },
  "optionals": []
}"#;

/// 老协议形状 B 的 1.7.10 版（实测 `installer-1.7.10.jar`，裁剪到 2 条库）。
/// 它同时是**双后缀**那一档：maven 版本串是 `1.7.10-10.13.4.1614-1.7.10`。
pub(crate) const FORGE_1710_PROFILE: &str = r#"{
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

/// 形状 C（**实测** `1.12.2-14.23.5.2851` 的 install_profile.json 原文裁剪版）：
/// 逐行保留到第 17 行（含第 16 行的 `"data": [],`），icon/base64 与 19 条库去掉。
pub(crate) const FORGE_2851_PROFILE: &str = r#"{
  "_comment_": [
    "Please do not automate the download and installation of Forge.",
    "Our efforts are supported by ads from the download page.",
    "If you MUST automate this, please consider supporting the project through https://www.patreon.com/LexManos/"
  ],
  "spec": 0,
  "profile": "forge",
  "version": "1.12.2-forge-14.23.5.2851",
  "json": "/version.json",
  "path": "net.minecraftforge:forge:1.12.2-14.23.5.2851",
  "logo": "/big_logo.png",
  "minecraft": "1.12.2",
  "welcome": "Welcome to the simple Forge installer.",
  "data": [],
  "processors": [],
  "libraries": []
}"#;
/// 形状 B（1.12.2 的 355 个版本里 343 个是这一档）：内联 `versionInfo` 就是版本主体，
/// 本体坐标在 `install.path`，库表里没有 `downloads` ⇒ 补一条 `-universal` 的 artifact。
#[test]
fn legacy_inline_version_info_is_json_route() {
    let profile = profile(FORGE_2768_PROFILE);
    // 「老协议」这个事实仍在（installer 里没有独立 version.json），只是不再等于装不了。
    assert!(profile.is_legacy_protocol());
    assert_eq!(
        profile.body_coordinate(),
        Some("net.minecraftforge:forge:1.12.2-14.23.5.2768")
    );

    let release = build_release(
        LoaderKind::Forge,
        "1.12.2",
        "1.12.2-14.23.5.2768",
        &profile,
        None,
    )
    .expect("有内联 versionInfo 就该走 json 路线，不该再说 unsupported");
    assert_eq!(release.support, LoaderSupport::JsonRoute);
    assert!(
        !release.requires_installer(),
        "这一档没有 processor，整段跳过"
    );
    assert_eq!(release.version_id, "1.12.2-forge-14.23.5.2768");
    assert_eq!(release.loader_version, "14.23.5.2768");
    assert_eq!(
        release.version_json.main_class.as_deref(),
        Some("net.minecraft.launchwrapper.Launch")
    );
    assert!(release
        .version_json
        .minecraft_arguments
        .as_deref()
        .unwrap_or_default()
        .contains("FMLTweaker"));

    // 本体：库表里连 downloads 都没有 ⇒ 补一条 artifact（远端 -universal、落点不带 classifier）。
    let libraries = release.version_json.libraries.as_ref().expect("应有库");
    assert_eq!(libraries.len(), 3, "versionInfo 的库一条都不许丢");
    let body = libraries
        .iter()
        .find(|library| library.name == "net.minecraftforge:forge:1.12.2-14.23.5.2768")
        .expect("应有本体库");
    let artifact = body
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.artifact.as_ref())
        .expect("本体要补出 artifact");
    assert_eq!(
        artifact.url.as_deref(),
        Some(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2768/forge-1.12.2-14.23.5.2768-universal.jar"
        )
    );
    assert_eq!(
        artifact.path.as_deref(),
        Some("net/minecraftforge/forge/1.12.2-14.23.5.2768/forge-1.12.2-14.23.5.2768.jar"),
        "落点不带 classifier（与形状 A 的落点规则一致）"
    );
    assert!(
        artifact.sha1.is_none() && artifact.size.is_none(),
        "老协议库表没有 sha1/size，不许编一个"
    );

    // 别的库：有 url 的保持自己的基址，没有 downloads 的**不许补 forge 基址**
    // （Mojang 的库在 libraries.minecraft.net，补 forge maven 会 404——实测
    // launchwrapper-1.12 与 lzma-0.0.1）；都不许加 -universal。
    let asm = libraries
        .iter()
        .find(|library| library.name == "org.ow2.asm:asm-all:5.2")
        .expect("应有 asm-all");
    assert!(asm.downloads.is_none());
    assert_eq!(
        asm.url.as_deref(),
        Some("https://maven.minecraftforge.net/")
    );
    let launchwrapper = libraries
        .iter()
        .find(|library| library.name == "net.minecraft:launchwrapper:1.12")
        .expect("应有 launchwrapper");
    assert!(
        launchwrapper.url.is_none(),
        "没有 url 的老协议库不写基址：Mojang 的库走 libraries.minecraft.net（{:?}）",
        launchwrapper.url
    );

    // 产品路径：LoaderInstallPlan 的待下载清单（即 --dry-run 看到的那张表）。
    let layout = GameLayout::new("/g/.minecraft");
    let plan = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &release,
        environment: &Environment::host(),
        parent_installed: false,
    })
    .expect("计划应可组");
    let file = plan
        .files()
        .iter()
        .find(|file| file.name == "net.minecraftforge:forge:1.12.2-14.23.5.2768")
        .expect("计划里应有本体");
    assert!(
        file.url
            .ends_with("forge-1.12.2-14.23.5.2768-universal.jar"),
        "本体远端必须带 -universal：{}",
        file.url
    );
    assert!(file.dest.ends_with(
        "libraries/net/minecraftforge/forge/1.12.2-14.23.5.2768/forge-1.12.2-14.23.5.2768.jar"
    ));
    // Mojang 的库（没有 url 也没有 downloads）走默认库仓，而不是 forge maven：
    // forge maven 上这条是 404，libraries.minecraft.net 上是 200（实测 32999 字节）。
    let launchwrapper = plan
        .files()
        .iter()
        .find(|file| file.name == "net.minecraft:launchwrapper:1.12")
        .expect("计划里应有 launchwrapper");
    assert_eq!(
        launchwrapper.url,
        "https://libraries.minecraft.net/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar"
    );
}

/// 1.7.10 的双后缀：本体坐标是 `1.7.10-10.13.4.1614-1.7.10`（末尾再写一次原版版本），
/// 拼出的 `-universal` 地址在 maven 上真实存在（2026-11-15 GET 200 / 3018833 字节）。
#[test]
fn legacy_1710_double_suffix_body_url_matches_the_real_maven_coordinate() {
    let release = build_release(
        LoaderKind::Forge,
        "1.7.10",
        "1.7.10-10.13.4.1614-1.7.10",
        &profile(FORGE_1710_PROFILE),
        None,
    )
    .expect("1.7.10 老协议应走 json 路线");
    assert_eq!(release.version_id, "1.7.10-forge-10.13.4.1614-1.7.10");
    assert_eq!(release.loader_version, "10.13.4.1614-1.7.10");
    // versionInfo 的 assets 原样带出（1.7.10 的资源索引名不是 legacy）。
    assert_eq!(release.version_json.assets.as_deref(), Some("1.7.10"));

    let libraries = release.version_json.libraries.as_ref().expect("应有库");
    let body = libraries
        .iter()
        .find(|library| library.name == "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10")
        .expect("应有本体库");
    let artifact = body
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.artifact.as_ref())
        .expect("本体要补出 artifact");
    assert_eq!(
        artifact.url.as_deref(),
        Some(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-universal.jar"
        )
    );
}

/// 形状 C：真 profile 第 16 行是 `"data": [],`（空**数组**）——要当空表吃下，与 `{}` 等价。
#[test]
fn empty_array_data_is_the_same_as_an_empty_map() {
    let parsed = profile(FORGE_2851_PROFILE);
    assert!(parsed.data.is_empty(), "空数组要归一成空表");
    let empty_map =
        profile(r#"{"spec": 0, "json": "/version.json", "data": {}, "processors": []}"#);
    assert_eq!(parsed.data, empty_map.data);
    // 别的键照常解出来（不是靠整份跳过才过关的）。
    assert_eq!(parsed.spec, Some(0));
    assert_eq!(parsed.version_json_path.as_deref(), Some("/version.json"));
    assert_eq!(
        parsed.path.as_deref(),
        Some("net.minecraftforge:forge:1.12.2-14.23.5.2851")
    );
    // 非空数组仍然报错：那说明这一档 data 形状我们没读懂，不许静默丢掉变量表。
    let error = serde_json::from_str::<InstallProfile>(r#"{"data": [{"client": "[x]"}]}"#)
        .expect_err("非空数组不是空表");
    assert!(error.to_string().contains("非空数组"), "{error}");
}

/// 连内联 `versionInfo` 都没有的老协议 installer：读不出任何版本声明 ⇒ 仍明确 unsupported。
#[test]
fn installer_without_any_version_body_is_unsupported() {
    let profile = profile(
        r#"{"install": {"filePath": "forge-1.7.10-10.13.4.1614-universal.jar"},
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
    .expect_err("没有版本主体必须明确报错");
    assert_eq!(error.kind(), LoaderErrorKind::Unsupported);
    assert!(
        error.message().contains("versionInfo"),
        "{}",
        error.message()
    );
    assert!(error.hint().is_some());
}
