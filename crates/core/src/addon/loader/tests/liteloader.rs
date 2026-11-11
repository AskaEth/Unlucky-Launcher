//! LiteLoader 数据源与合成：官方版本表（**真实响应的裁剪版**）→ 版本条目 → 合成清单。
//!
//! 夹具 `LITELOADER_VERSIONS` 是 2026-11 从 `https://dl.liteloader.com/versions/versions.json`
//! 真取的一份响应（200 / 49129 字节 / 16 个原版版本）里挑三个原版版本原样留下：
//! `1.7.10`（只有 release 分支）、`1.10.2`（release + snapshots，且 `repo.url` 是 `…/repo/`
//! 形态——专门用来钉「快照仓固定」这条）、`1.12.2`（只有 snapshots 分支）。删掉的是另外 13 个原版版本与
//! 每个版本下的多余分支；**字段名与结构一个没改**（键仍是 md5、`latest` 别名也留着，用来
//! 钉「跳过别名 + 按 version 去重」）。`LITELOADER_METADATA` 是同一次探测里取的
//! `repo.mumfrey.com/…/liteloader/1.12.2-SNAPSHOT/maven-metadata.xml`（200 / 358 字节）。

use crate::addon::loader::liteloader::{
    build_release, default_entry, find_entry, parse_versions, snapshot_artifact_url, CENTRAL_REPO,
};
use crate::addon::loader::{LoaderErrorKind, LoaderKind};
use crate::game::loader::{InstanceLoader, LoaderKind as Wide};
use crate::game::version::ReleaseType;

/// 真实响应的裁剪版（见文件头）。
const LITELOADER_VERSIONS: &str = r#"{
  "meta": {
    "description": "LiteLoader is a lightweight mod bootstrap designed to provide basic loader functionality for mods which don't need to modify game mechanics.",
    "authors": "Mumfrey",
    "url": "http://dl.liteloader.com",
    "updated": "2017-11-29T16:31:41+00:00",
    "updatedTime": 1511973101
  },
  "versions": {
    "1.7.10": {
      "repo": {"stream": "RELEASE", "type": "ivy", "url": "http://dl.liteloader.com/versions/", "classifier": "mcpnames"},
      "artefacts": {"com.mumfrey:liteloader": {
        "63ada46e033d0cb6782bada09ad5ca4e": {
          "tweakClass": "com.mumfrey.liteloader.launch.LiteLoaderTweaker",
          "libraries": [{"name": "net.minecraft:launchwrapper:1.11"}, {"name": "org.ow2.asm:asm-all:5.0.3"}],
          "srcJar": "liteloader-1.7.10_04-mcpnames-sources.jar",
          "mcpJar": "liteloader-1.7.10_04-mcpnames.jar",
          "stream": "RELEASE",
          "file": "liteloader-1.7.10_04.jar",
          "version": "1.7.10_04",
          "md5": "63ada46e033d0cb6782bada09ad5ca4e",
          "timestamp": "1414368553"
        },
        "db7235aefd407ac1fde09a7baba50839": {
          "tweakClass": "com.mumfrey.liteloader.launch.LiteLoaderTweaker",
          "libraries": [
            {"name": "net.minecraft:launchwrapper:1.9"},
            {"name": "org.ow2.asm:asm-all:5.0.3", "url": "http://repo.maven.apache.org/maven2/"}
          ],
          "stream": "RELEASE",
          "file": "liteloader-1.7.10_00.jar",
          "version": "1.7.10_00",
          "md5": "db7235aefd407ac1fde09a7baba50839",
          "timestamp": "1404330030"
        }
      }}
    },
    "1.10.2": {
      "repo": {"stream": "RELEASE", "type": "m2", "url": "http://dl.liteloader.com/repo/", "classifier": ""},
      "artefacts": {"com.mumfrey:liteloader": {
        "8a7c21f32d77ee08b393dd3921ced8eb": {
          "tweakClass": "com.mumfrey.liteloader.launch.LiteLoaderTweaker",
          "libraries": [{"name": "net.minecraft:launchwrapper:1.12"}, {"name": "org.ow2.asm:asm-all:5.0.3"}],
          "stream": "RELEASE",
          "file": "liteloader-1.10.2.jar",
          "version": "1.10.2",
          "md5": "8a7c21f32d77ee08b393dd3921ced8eb",
          "timestamp": "1479473570"
        }
      }},
      "snapshots": {
        "libraries": [
          {"name": "net.minecraft:launchwrapper:1.12"},
          {"name": "org.spongepowered:mixin:0.5.17-SNAPSHOT", "url": "https://repo.spongepowered.org/maven/"}
        ],
        "com.mumfrey:liteloader": {
          "1cdd72f7232e45551f16cc8ffd27ccf3": {
            "tweakClass": "com.mumfrey.liteloader.launch.LiteLoaderTweaker",
            "libraries": [{"name": "net.minecraft:launchwrapper:1.12"}, {"name": "org.ow2.asm:asm-all:5.0.3"}],
            "stream": "SNAPSHOT",
            "file": "liteloader-1.10.2-SNAPSHOT.jar",
            "version": "1.10.2-SNAPSHOT",
            "build": "1.10.2-SNAPSHOT-rE85C8AE-b11-11",
            "md5": "1cdd72f7232e45551f16cc8ffd27ccf3",
            "timestamp": "1485987183",
            "lastSuccessfulBuild": 11
          }
        }
      }
    },
    "1.12.2": {
      "repo": {"stream": "SNAPSHOT", "type": "m2", "url": "http://repo.mumfrey.com/content/repositories/snapshots/", "classifier": ""},
      "snapshots": {
        "libraries": [
          {"name": "net.minecraft:launchwrapper:1.12"},
          {"name": "org.spongepowered:mixin:0.7.4-SNAPSHOT", "url": "https://repo.spongepowered.org/maven/"}
        ],
        "com.mumfrey:liteloader": {
          "1420785ecbfed5aff4a586c5c9dd97eb": {
            "tweakClass": "com.mumfrey.liteloader.launch.LiteLoaderTweaker",
            "libraries": [{"name": "net.minecraft:launchwrapper:1.12"}, {"name": "org.ow2.asm:asm-all:5.2"}],
            "stream": "SNAPSHOT",
            "file": "liteloader-1.12.2-SNAPSHOT.jar",
            "version": "1.12.2-SNAPSHOT",
            "build": "1.12.2-SNAPSHOT-r4CC2BB0-b4-4",
            "md5": "1420785ecbfed5aff4a586c5c9dd97eb",
            "timestamp": "1511880271",
            "lastSuccessfulBuild": 4
          },
          "latest": {
            "tweakClass": "com.mumfrey.liteloader.launch.LiteLoaderTweaker",
            "libraries": [{"name": "net.minecraft:launchwrapper:1.12"}, {"name": "org.ow2.asm:asm-all:5.2"}],
            "stream": "SNAPSHOT",
            "file": "liteloader-1.12.2-SNAPSHOT.jar",
            "version": "1.12.2-SNAPSHOT",
            "md5": "1420785ecbfed5aff4a586c5c9dd97eb",
            "timestamp": "1511880271",
            "lastSuccessfulBuild": 4
          }
        }
      }
    }
  }
}"#;

/// 同一次探测取的 maven-metadata（真实 358 字节响应的原样）。
const LITELOADER_METADATA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
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

#[test]
fn parses_the_real_version_table() {
    let table = parse_versions(LITELOADER_VERSIONS).expect("真实响应必须能解析");
    assert_eq!(table.len(), 3, "只留了三个原版版本");

    // release 分支：按 version 去重、新的排前面。
    let releases = &table["1.7.10"];
    assert_eq!(
        releases
            .iter()
            .map(|e| e.version.as_str())
            .collect::<Vec<_>>(),
        ["1.7.10_04", "1.7.10_00"]
    );
    // 目录名是**原版**版本（1.7.10），不是 loader 版本（1.7.10_04）。
    assert_eq!(
        releases[0].url.as_deref(),
        Some("https://dl.liteloader.com/versions/com/mumfrey/liteloader/1.7.10/liteloader-1.7.10_04.jar")
    );
    assert!(!releases[0].snapshot);

    // 记录级库：没 url 的非 Mojang 库补 Central，Mojang 的库保持默认基址。
    let names: Vec<&str> = releases[0]
        .libraries
        .iter()
        .map(|library| library.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "net.minecraft:launchwrapper:1.11",
            "org.ow2.asm:asm-all:5.0.3"
        ]
    );
    assert_eq!(
        releases[0].libraries[0].url, None,
        "Mojang 库仓才有 launchwrapper"
    );
    assert_eq!(releases[0].libraries[1].url.as_deref(), Some(CENTRAL_REPO));

    // 记录自己写了 url 的，保留但 http → https。
    assert_eq!(
        releases[1].libraries[1].url.as_deref(),
        Some("https://repo.maven.apache.org/maven2/")
    );

    // snapshot 分支：`latest` 别名跳过，直链留给 maven-metadata 现拼。
    let snapshots = &table["1.12.2"];
    assert_eq!(snapshots.len(), 1, "latest 别名不算一个版本");
    assert!(snapshots[0].snapshot);
    assert!(snapshots[0].url.is_none());
    assert_eq!(
        snapshots[0].snapshot_dir.as_deref(),
        Some("https://repo.mumfrey.com/content/repositories/snapshots/com/mumfrey/liteloader/1.12.2-SNAPSHOT/")
    );
    // 分支级 libraries 里那条 mixin 不该混进记录（那是 ForgeGradle 开发环境用的）。
    assert!(!snapshots[0]
        .libraries
        .iter()
        .any(|library| library.name.contains("mixin")));
}

#[test]
fn snapshot_url_comes_from_maven_metadata() {
    let table = parse_versions(LITELOADER_VERSIONS).expect("解析");
    let entry = &table["1.12.2"][0];
    assert_eq!(
        snapshot_artifact_url(entry, LITELOADER_METADATA).expect("应拼得出直链"),
        "https://repo.mumfrey.com/content/repositories/snapshots/com/mumfrey/liteloader/1.12.2-SNAPSHOT/liteloader-1.12.2-20171128.144431-4-release.jar"
    );

    // 缺 <buildNumber>：报错，不猜一个指向 404 的直链。
    let broken = LITELOADER_METADATA.replace("<buildNumber>4</buildNumber>", "");
    let error = snapshot_artifact_url(entry, &broken).expect_err("缺字段必须报错");
    assert_eq!(error.kind(), LoaderErrorKind::Metadata);
}

#[test]
fn build_release_synthesizes_launchwrapper_manifest() {
    let table = parse_versions(LITELOADER_VERSIONS).expect("解析");
    let entry = find_entry(&table["1.12.2"], "1.12.2-snapshot").expect("版本号大小写不敏感");
    let url = snapshot_artifact_url(entry, LITELOADER_METADATA).expect("直链");
    let release = build_release("1.12.2", "1.12.2", entry, &url).expect("合成");

    assert_eq!(release.version_id, "1.12.2-liteloader-1.12.2-SNAPSHOT");
    let json = &release.version_json;
    assert_eq!(
        json.main_class.as_deref(),
        Some("net.minecraft.launchwrapper.Launch")
    );
    // 父子链靠 arguments.game 追加，不写 minecraftArguments（写了会顶掉 Forge 的 tweakClass）。
    assert!(json.minecraft_arguments.is_none());
    let game: Vec<String> = json
        .arguments
        .as_ref()
        .expect("arguments 必须有")
        .expand(&Default::default())
        .game;
    assert_eq!(
        game,
        [
            "--tweakClass",
            "com.mumfrey.liteloader.launch.LiteLoaderTweaker"
        ]
    );
    assert_eq!(json.inherits_from.as_deref(), Some("1.12.2"));
    assert_eq!(json.jar.as_deref(), Some("1.12.2"), "jar 恒在原版那层");
    assert_eq!(json.version_type, Some(ReleaseType::Snapshot));

    // 本体 jar 那一条：绝对直链 + maven 坐标（快照路径与坐标不一致，所以必须给 url）。
    let loader = json
        .libraries
        .as_ref()
        .expect("libraries")
        .iter()
        .find(|library| library.name == "com.mumfrey:liteloader:1.12.2-SNAPSHOT")
        .expect("缺本体库");
    assert_eq!(
        loader
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.as_ref())
            .and_then(|artifact| artifact.url.as_deref()),
        Some(url.as_str())
    );
    assert!(!release.requires_installer(), "LiteLoader 不跑任何外部进程");
    assert!(release.warnings.is_empty(), "{:?}", release.warnings);
}

#[test]
fn synthesized_version_id_parses_back_to_liteloader() {
    for (mc, loader_version) in [("1.12.2", "1.12.2-SNAPSHOT"), ("1.7.10", "1.7.10_04")] {
        let id = LoaderKind::LiteLoader.version_id(mc, loader_version);
        let parsed = InstanceLoader::parse(&id);
        assert_eq!(parsed.kind, Wide::LiteLoader, "`{id}` 认错了家");
        assert_eq!(parsed.version, loader_version, "`{id}` 的版本段");
    }
}

#[test]
fn missing_tweak_class_is_a_metadata_error() {
    let table = parse_versions(LITELOADER_VERSIONS).expect("解析");
    let mut entry = table["1.7.10"][0].clone();
    entry.tweak_class = "  ".to_string();
    let error = build_release("1.7.10", "1.7.10", &entry, "https://example.invalid/x.jar")
        .expect_err("缺 tweakClass 必须报错");
    assert_eq!(error.kind(), LoaderErrorKind::Metadata);
}

#[test]
fn default_entry_prefers_release_builds() {
    let table = parse_versions(LITELOADER_VERSIONS).expect("解析");
    // 1.7.10 只有 release：取最新那条。
    assert_eq!(
        default_entry(&table["1.7.10"]).map(|entry| entry.version.as_str()),
        Some("1.7.10_04")
    );
    // 1.12.2 只有快照：没有 release 可选时退回最新。
    assert_eq!(
        default_entry(&table["1.12.2"]).map(|entry| entry.version.as_str()),
        Some("1.12.2-SNAPSHOT")
    );
}

#[test]
fn unknown_version_does_not_fall_back() {
    let table = parse_versions(LITELOADER_VERSIONS).expect("解析");
    assert!(find_entry(&table["1.7.10"], "1.7.10_99").is_none());
    // 形状不符的表（顶层不是 versions）也要报 metadata，而不是当空表。
    let error = parse_versions(r#"{"versions": []}"#).expect_err("形状不符必须报错");
    assert_eq!(error.kind(), LoaderErrorKind::Metadata);
}

/// 快照仓**固定**在 `repo.mumfrey.com/content/repositories/snapshots/`，不跟 `repo.url` 走。
///
/// 抓得住：1.10.2 的 `repo.url` 是 `…/repo/`，照它拼快照路径会 302 到
/// `…/repositories/liteloader/…` 再 404（实测）。1.12.2 的 `repo.url` 恰好就是快照仓，
/// 所以只测 1.12.2 会漏掉这个 bug。
#[test]
fn snapshot_repo_is_fixed_not_taken_from_repo_url() {
    let table = parse_versions(LITELOADER_VERSIONS).expect("解析");
    let entries = &table["1.10.2"];
    let snapshot = entries
        .iter()
        .find(|entry| entry.snapshot)
        .expect("1.10.2 应有快照条目");
    assert_eq!(
        snapshot.snapshot_dir.as_deref(),
        Some("https://repo.mumfrey.com/content/repositories/snapshots/com/mumfrey/liteloader/1.10.2-SNAPSHOT/")
    );
    // release 条目仍走该版本自己的 `repo.url`（上游 `/repo/` 会 302 到真地址，引擎跟重定向）。
    let release = entries
        .iter()
        .find(|entry| !entry.snapshot)
        .expect("1.10.2 应有 release 条目");
    assert_eq!(
        release.url.as_deref(),
        Some("https://dl.liteloader.com/repo/com/mumfrey/liteloader/1.10.2/liteloader-1.10.2.jar")
    );
}
