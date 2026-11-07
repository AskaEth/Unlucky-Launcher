//! 清单解析与校验：字段拼错、重复版本、零长度、摘要格式、产物优先级。

use crate::download::HashAlgo;
use crate::platform::os_info::OsFamily;
use crate::update::manifest::ReleaseManifest;
use crate::update::{Arch, Channel, PackageFormat, Target, UpdateErrorKind};

use super::support::{artifact, manifest, release, sha256};

const PAYLOAD: &[u8] = b"fake portable zip payload";

#[test]
fn parses_a_full_manifest() {
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            r#","released_at":"2026-01-02T03:04:05Z","notes":"修掉启动崩溃","min_supported":"0.2.0""#,
            &[
                artifact("win-x64", "portable-zip", "ull-0.3.0-win-x64.zip", PAYLOAD),
                artifact(
                    "linux-x64",
                    "appimage",
                    "ull-0.3.0-linux-x64.AppImage",
                    PAYLOAD,
                ),
            ],
        )],
    );
    let parsed = ReleaseManifest::from_json(&json).expect("清单应可解析");
    assert_eq!(parsed.channel, Channel::Stable);
    assert_eq!(parsed.releases.len(), 1);
    let first = &parsed.releases[0];
    assert_eq!(first.version.to_string(), "0.3.0");
    assert_eq!(first.channel, Channel::Stable, "渠道字段缺省时继承清单渠道");
    assert_eq!(first.notes.as_deref(), Some("修掉启动崩溃"));
    assert_eq!(
        first.min_supported.as_ref().map(ToString::to_string),
        Some("0.2.0".to_string())
    );
    assert!(!first.force);
    assert_eq!(first.artifacts.len(), 2);
    let windows = &first.artifacts[0];
    assert_eq!(windows.target, Target::new(OsFamily::Windows, Arch::X64));
    assert_eq!(windows.format, PackageFormat::PortableZip);
    assert_eq!(windows.integrity.algo, HashAlgo::Sha256);
    assert_eq!(windows.integrity.hex, sha256(PAYLOAD));
    assert_eq!(windows.size, PAYLOAD.len() as u64);
}

#[test]
fn release_can_override_the_manifest_channel() {
    let json = manifest("beta", &[release("0.3.0", r#","channel":"stable""#, &[])]);
    let parsed = ReleaseManifest::from_json(&json).expect("清单应可解析");
    assert_eq!(parsed.releases[0].channel, Channel::Stable);
}

#[test]
fn rejects_duplicate_versions() {
    let json = manifest(
        "stable",
        &[
            release("0.3.0", "", &[]),
            release("0.3.0", r#","notes":"重复""#, &[]),
        ],
    );
    let error = ReleaseManifest::from_json(&json).expect_err("重复版本应失败");
    assert_eq!(error.kind, UpdateErrorKind::InvalidManifest);
    assert!(error.to_string().contains("0.3.0"));
}

#[test]
fn rejects_zero_size_artifact() {
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[format!(
                r#"{{"target":"win-x64","format":"msi","url":"a.msi","size":0,"sha256":"{}"}}"#,
                sha256(b"")
            )],
        )],
    );
    let error = ReleaseManifest::from_json(&json).expect_err("size=0 应失败");
    assert_eq!(error.kind, UpdateErrorKind::InvalidManifest);
    assert!(error.to_string().contains("size"));
}

#[test]
fn rejects_unknown_fields() {
    let json = r#"{"channel":"stable","relases":[]}"#;
    let error = ReleaseManifest::from_json(json).expect_err("拼错的字段名应失败");
    assert_eq!(error.kind, UpdateErrorKind::InvalidManifest);
}

#[test]
fn rejects_artifact_without_digest() {
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[r#"{"target":"win-x64","format":"msi","url":"a.msi","size":12}"#.to_string()],
        )],
    );
    let error = ReleaseManifest::from_json(&json).expect_err("缺摘要应失败");
    assert!(error.to_string().contains("sha256"));
}

#[test]
fn rejects_malformed_digest() {
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[
                r#"{"target":"win-x64","format":"msi","url":"a.msi","size":12,"sha256":"abc"}"#
                    .to_string(),
            ],
        )],
    );
    let error = ReleaseManifest::from_json(&json).expect_err("摘要长度不对应失败");
    assert_eq!(error.kind, UpdateErrorKind::InvalidManifest);
}

#[test]
fn accepts_sha1_only_and_normalises_case() {
    // 64 位 hex 挂 sha1 名头是错长度的，先确认它真的被挡下
    let wrong_length = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[format!(
                r#"{{"target":"win-x64","format":"msi","url":"a.msi","size":12,"sha1":"{}"}}"#,
                sha256(PAYLOAD)
            )],
        )],
    );
    assert!(ReleaseManifest::from_json(&wrong_length).is_err());

    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[format!(
                r#"{{"target":"win-x64","format":"msi","url":"a.msi","size":12,"sha1":"{}"}}"#,
                "A".repeat(40)
            )],
        )],
    );
    let parsed = ReleaseManifest::from_json(&json).expect("只有 sha1 也应可解析");
    let integrity = &parsed.releases[0].artifacts[0].integrity;
    assert_eq!(integrity.algo, HashAlgo::Sha1);
    assert_eq!(integrity.hex, "a".repeat(40), "摘要统一归一化为小写");
}

#[test]
fn sha256_wins_when_both_digests_are_present() {
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[format!(
                r#"{{"target":"win-x64","format":"msi","url":"a.msi","size":12,"sha1":"{}","sha256":"{}"}}"#,
                "a".repeat(40),
                sha256(PAYLOAD)
            )],
        )],
    );
    let parsed = ReleaseManifest::from_json(&json).expect("两个摘要都在时应可解析");
    assert_eq!(
        parsed.releases[0].artifacts[0].integrity.algo,
        HashAlgo::Sha256
    );
}

#[test]
fn rejects_unknown_target_and_format() {
    let bad_target = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact("solaris-x64", "msi", "a.msi", PAYLOAD)],
        )],
    );
    let error = ReleaseManifest::from_json(&bad_target).expect_err("未知平台应失败");
    assert!(error.to_string().contains("solaris-x64"));

    let bad_format = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact("win-x64", "tar.gz", "a.tgz", PAYLOAD)],
        )],
    );
    let error = ReleaseManifest::from_json(&bad_format).expect_err("未知格式应失败");
    assert!(error.to_string().contains("tar.gz"));
}

#[test]
fn artifact_order_is_the_priority() {
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[
                artifact("win-x64", "portable-zip", "a.zip", PAYLOAD),
                artifact("win-x64", "msi", "a.msi", PAYLOAD),
            ],
        )],
    );
    let parsed = ReleaseManifest::from_json(&json).expect("清单应可解析");
    let target = Target::new(OsFamily::Windows, Arch::X64);
    let artifact = parsed.releases[0]
        .artifact_for(target)
        .expect("应有 win-x64 产物");
    assert_eq!(artifact.format, PackageFormat::PortableZip, "先写先选");
}

#[test]
fn newest_artifact_for_skips_releases_without_the_target() {
    let json = manifest(
        "stable",
        &[
            release(
                "0.3.0",
                "",
                &[artifact("win-x64", "portable-zip", "a.zip", PAYLOAD)],
            ),
            release(
                "0.4.0",
                "",
                &[artifact("linux-x64", "appimage", "b.AppImage", PAYLOAD)],
            ),
        ],
    );
    let parsed = ReleaseManifest::from_json(&json).expect("清单应可解析");
    let (release, _) = parsed
        .newest_artifact_for(Target::new(OsFamily::Windows, Arch::X64))
        .expect("win-x64 只有 0.3.0 有货");
    assert_eq!(release.version.to_string(), "0.3.0");
}

#[test]
fn empty_release_list_is_valid() {
    let parsed = ReleaseManifest::from_json(r#"{"channel":"nightly","releases":[]}"#)
        .expect("空清单是合法状态（还没发过版）");
    assert_eq!(parsed.channel, Channel::Nightly);
    assert!(parsed.releases.is_empty());
}
