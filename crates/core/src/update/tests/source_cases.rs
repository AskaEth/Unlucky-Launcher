//! 本地目录源：清单缺失不是错误、渠道对不上要报错、远端地址一律拒绝。

use std::fs;

use crate::update::manifest::ReleaseManifest;
use crate::update::{Channel, DirSource, UpdateErrorKind, UpdateSource};

use super::support::{artifact, manifest, release, TempDir};

const PAYLOAD: &[u8] = b"source payload";

fn publish(tmp: &TempDir, channel: Channel, json: &str) {
    tmp.write(&format!("{}.json", channel.as_str()), json.as_bytes());
}

#[test]
fn missing_channel_manifest_is_not_an_error() {
    let tmp = TempDir::new("source-missing");
    let source = DirSource::new(tmp.path());
    let manifest = source
        .fetch_manifest(Channel::Stable)
        .expect("缺清单不该报错");
    assert!(manifest.is_none(), "还没发过版 = Ok(None)");
}

#[test]
fn reads_and_validates_the_channel_manifest() {
    let tmp = TempDir::new("source-ok");
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact("win-x64", "portable-zip", "ull-test.zip", PAYLOAD)],
        )],
    );
    publish(&tmp, Channel::Stable, &json);
    let source = DirSource::new(tmp.path());
    let parsed = source
        .fetch_manifest(Channel::Stable)
        .expect("应取到清单")
        .expect("清单存在");
    assert_eq!(parsed.channel, Channel::Stable);
    assert_eq!(parsed.releases.len(), 1);
    assert!(source.source_id().starts_with("file:"));
    assert_eq!(source.root(), tmp.path());
    assert_eq!(source.manifest_path(Channel::Beta), tmp.join("beta.json"));
}

#[test]
fn channel_mismatch_is_an_error() {
    let tmp = TempDir::new("source-mismatch");
    // 清单自称 beta，却躺在 stable.json 里
    publish(&tmp, Channel::Stable, &manifest("beta", &[]));
    let error = DirSource::new(tmp.path())
        .fetch_manifest(Channel::Stable)
        .expect_err("渠道不符必须报错");
    assert_eq!(error.kind, UpdateErrorKind::ChannelMismatch);
}

#[test]
fn broken_manifest_json_is_reported_with_path() {
    let tmp = TempDir::new("source-broken");
    publish(&tmp, Channel::Stable, "{ not json");
    let error = DirSource::new(tmp.path())
        .fetch_manifest(Channel::Stable)
        .expect_err("坏 JSON 必须报错");
    assert_eq!(error.kind, UpdateErrorKind::InvalidManifest);
    assert!(error.path.is_some(), "错误应带清单路径");
}

#[test]
fn copies_artifact_from_a_relative_url() {
    let tmp = TempDir::new("source-copy");
    let file = tmp.write("dist/ull-test.zip", PAYLOAD);
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact(
                "win-x64",
                "portable-zip",
                "dist/ull-test.zip",
                PAYLOAD,
            )],
        )],
    );
    let parsed = ReleaseManifest::from_json(&json).expect("清单应可解析");
    let dest = tmp.join("staging/copy.zip");
    let written = DirSource::new(tmp.path())
        .fetch_artifact(&parsed.releases[0].artifacts[0], &dest)
        .expect("应复制成功");
    assert_eq!(written, PAYLOAD.len() as u64);
    assert_eq!(fs::read(&dest).expect("目标应存在"), PAYLOAD);
    assert!(file.exists());
    assert!(dest.parent().is_some_and(|parent| parent.exists()));
}

#[test]
fn accepts_file_url_for_the_artifact() {
    let tmp = TempDir::new("source-file-url");
    let file = tmp.write("dist/ull-test.zip", PAYLOAD);
    let url = format!("file://{}", file.display().to_string().replace('\\', "/"));
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact("win-x64", "portable-zip", &url, PAYLOAD)],
        )],
    );
    let parsed = ReleaseManifest::from_json(&json).expect("清单应可解析");
    let dest = tmp.join("staging/copy.zip");
    DirSource::new(tmp.join("a-different-root"))
        .fetch_artifact(&parsed.releases[0].artifacts[0], &dest)
        .expect("file:// 地址应可复制");
    assert_eq!(fs::read(&dest).expect("目标应存在"), PAYLOAD);
}

#[test]
fn rejects_remote_urls() {
    let tmp = TempDir::new("source-remote");
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact(
                "win-x64",
                "portable-zip",
                "https://example.invalid/ull-test.zip",
                PAYLOAD,
            )],
        )],
    );
    let parsed = ReleaseManifest::from_json(&json).expect("清单应可解析");
    let error = DirSource::new(tmp.path())
        .fetch_artifact(&parsed.releases[0].artifacts[0], &tmp.join("copy.zip"))
        .expect_err("本地源不该假装能取远端字节");
    assert_eq!(error.kind, UpdateErrorKind::UnsupportedSource);
}

#[test]
fn missing_artifact_file_is_source_missing() {
    let tmp = TempDir::new("source-no-file");
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact(
                "win-x64",
                "portable-zip",
                "not-there.zip",
                PAYLOAD,
            )],
        )],
    );
    let parsed = ReleaseManifest::from_json(&json).expect("清单应可解析");
    let error = DirSource::new(tmp.path())
        .fetch_artifact(&parsed.releases[0].artifacts[0], &tmp.join("copy.zip"))
        .expect_err("产物不存在应报错");
    assert_eq!(error.kind, UpdateErrorKind::SourceMissing);
}

#[test]
fn builds_a_source_from_a_file_url() {
    let tmp = TempDir::new("source-from-url");
    let url = format!(
        "file://{}",
        tmp.path().display().to_string().replace('\\', "/")
    );
    let source = DirSource::from_url(&url).expect("file:// 应被接受");
    assert_eq!(source.root(), tmp.path(), "根目录应与给定目录一致");

    let error = DirSource::from_url("https://example.invalid/dist").expect_err("https 不是本地源");
    assert_eq!(error.kind, UpdateErrorKind::UnsupportedSource);
}
