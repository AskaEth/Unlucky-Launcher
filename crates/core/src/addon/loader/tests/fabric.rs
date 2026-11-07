//! Fabric / Quilt：profile 归一化的形状对拍。

use super::{FABRIC_PROFILE, QUILT_PROFILE};
use crate::addon::loader::fabric::{build_release, parse_loader_list, pick_default};
use crate::addon::loader::{LoaderErrorKind, LoaderKind, LoaderSupport};

#[test]
fn fabric_profile_becomes_inheriting_loader_version() {
    let release = build_release(LoaderKind::Fabric, "1.20.1", "0.15.11", FABRIC_PROFILE)
        .expect("fabric profile 应可归一化");

    assert_eq!(release.version_id, "1.20.1-fabric-0.15.11");
    let json = &release.version_json;
    assert_eq!(json.id.as_deref(), Some("1.20.1-fabric-0.15.11"));
    assert_eq!(json.inherits_from.as_deref(), Some("1.20.1"));
    // ★ 关键：jar 必须钉成原版 id，否则继承解析会把 jar 兜底成 loader 自身 id。
    assert_eq!(json.jar.as_deref(), Some("1.20.1"));
    assert_eq!(
        json.main_class.as_deref(),
        Some("net.fabricmc.loader.impl.launch.knot.KnotClient")
    );
    // jvm 参数（`-DFabricMcEmu`）必须原样保留：老版本启动靠它。
    let jvm = json
        .arguments
        .as_ref()
        .map(|arguments| arguments.jvm.len())
        .unwrap_or_default();
    assert_eq!(jvm, 1, "profile 的 jvm 参数丢了");
    assert_eq!(release.support, LoaderSupport::JsonRoute);
    assert!(release.is_installable());
    assert!(release.warnings.is_empty(), "{:?}", release.warnings);
}

#[test]
fn fabric_flat_digest_is_lifted_into_artifact() {
    let release = build_release(LoaderKind::Fabric, "1.20.1", "0.15.11", FABRIC_PROFILE)
        .expect("fabric profile 应可归一化");
    let libraries = release.version_json.libraries.expect("应有库");
    let asm = libraries
        .iter()
        .find(|library| library.name.starts_with("org.ow2.asm:asm:"))
        .expect("应有 asm");
    let artifact = asm
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.artifact.as_ref())
        .expect("扁平 sha1/size 应提升成 downloads.artifact");
    assert_eq!(
        artifact.sha1.as_deref(),
        Some("aa205cf0a06dbd8e04ece91c0b37c3f5d567546a")
    );
    assert_eq!(artifact.size, Some(123_598));
    assert_eq!(
        artifact.url.as_deref(),
        Some("https://maven.fabricmc.net/org/ow2/asm/asm/9.6/asm-9.6.jar")
    );
}

#[test]
fn quilt_profile_is_isomorphic() {
    let release = build_release(LoaderKind::Quilt, "1.20.1", "0.20.0-beta.9", QUILT_PROFILE)
        .expect("quilt profile 应可归一化");

    assert_eq!(release.version_id, "1.20.1-quilt-0.20.0-beta.9");
    assert_eq!(release.version_json.jar.as_deref(), Some("1.20.1"));
    assert_eq!(
        release.version_json.main_class.as_deref(),
        Some("org.quiltmc.loader.impl.launch.knot.KnotClient")
    );
    assert_eq!(release.support, LoaderSupport::JsonRoute);
    // Quilt 的 profile 没有 loading 库时不该编造；这里有 quilt-loader，因此无告警。
    assert!(release.warnings.is_empty(), "{:?}", release.warnings);
}

#[test]
fn missing_main_class_is_metadata_error() {
    let profile = r#"{"id":"x","inheritsFrom":"1.20.1","libraries":[]}"#;
    let error = build_release(LoaderKind::Fabric, "1.20.1", "0.15.11", profile)
        .expect_err("缺主类必须报错");
    assert_eq!(error.kind(), LoaderErrorKind::Metadata);
}

#[test]
fn missing_loader_library_is_appended_with_warning() {
    // 去掉 fabric-loader 那条，看归一化会不会补。
    let profile = r#"{
      "id": "fabric-loader-0.15.11-1.20.1",
      "inheritsFrom": "1.20.1",
      "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
      "libraries": [{"name": "org.ow2.asm:asm:9.6", "url": "https://maven.fabricmc.net/"}]
    }"#;
    let release = build_release(LoaderKind::Fabric, "1.20.1", "0.15.11", profile)
        .expect("缺 loader 库应降级为告警 + 补一条");
    assert_eq!(release.warnings.len(), 1);
    let libraries = release.version_json.libraries.expect("应有库");
    assert!(libraries
        .iter()
        .any(|library| library.name == "net.fabricmc:fabric-loader:0.15.11"));
}

#[test]
fn loader_list_parses_and_prefers_stable() {
    let list = r#"[
      {"loader": {"version": "0.16.0-beta.1", "stable": false, "maven": "net.fabricmc:fabric-loader:0.16.0-beta.1"}},
      {"loader": {"version": "0.15.11", "stable": true, "maven": "net.fabricmc:fabric-loader:0.15.11"}},
      {"loader": {"version": "0.15.10", "stable": true, "maven": "net.fabricmc:fabric-loader:0.15.10"}}
    ]"#;
    let entries = parse_loader_list(LoaderKind::Fabric, list).expect("列表应可解析");
    assert_eq!(entries.len(), 3);
    assert_eq!(
        pick_default(&entries).map(|entry| entry.version.as_str()),
        Some("0.15.11")
    );
}

#[test]
fn quilt_loader_list_without_stable_flag_still_picks_first() {
    // 实测 Quilt 的 loader 条目没有 stable 键。
    let list = r#"[
      {"loader": {"version": "0.20.0-beta.9", "maven": "org.quiltmc:quilt-loader:0.20.0-beta.9"}},
      {"loader": {"version": "0.19.2", "maven": "org.quiltmc:quilt-loader:0.19.2"}}
    ]"#;
    let entries = parse_loader_list(LoaderKind::Quilt, list).expect("列表应可解析");
    assert!(entries.iter().all(|entry| entry.stable));
    assert_eq!(
        pick_default(&entries).map(|entry| entry.version.as_str()),
        Some("0.20.0-beta.9")
    );
}

#[test]
fn empty_loader_list_is_not_found() {
    let error = parse_loader_list(LoaderKind::Fabric, "[]").expect_err("空列表应报错");
    assert_eq!(error.kind(), LoaderErrorKind::NotFound);
}
