//! 检查更新：渠道准入、版本高低、目标平台漏发、强制更新。

use crate::platform::os_info::OsFamily;
use crate::update::{
    Channel, CheckOutcome, DirSource, Target, UpdateChecker, UpdateErrorKind, UpdateSource,
    Urgency, Version,
};

use super::support::{artifact, manifest, release, TempDir};

const PAYLOAD: &[u8] = b"checker payload";

fn target() -> Target {
    Target::new(OsFamily::Windows, crate::update::Arch::X64)
}

/// 新建一个只放了这一份清单的本地源；返回的 `TempDir` 要活到用例结束。
fn source_with(json: &str, channel: Channel) -> (TempDir, DirSource) {
    let dir = TempDir::new("checker");
    dir.write(&format!("{}.json", channel.as_str()), json.as_bytes());
    let source = DirSource::new(dir.path());
    (dir, source)
}

fn check_outcome(json: &str, current: &str, channel: Channel) -> CheckOutcome {
    let (_dir, source) = source_with(json, channel);
    let version = Version::parse(current).expect("当前版本应可解析");
    UpdateChecker::new(source, target())
        .check(&version, channel)
        .expect("检查不该失败")
}

fn stable_release(version: &str) -> String {
    manifest(
        "stable",
        &[release(
            version,
            "",
            &[artifact("win-x64", "portable-zip", "a.zip", PAYLOAD)],
        )],
    )
}

#[test]
fn offers_the_numeric_newer_version() {
    // 0.9 → 0.10 的字典序陷阱在 checker 这一层也要挡住
    let outcome = check_outcome(&stable_release("0.10.0"), "0.9.0", Channel::Stable);
    let candidate = outcome.candidate().expect("应有更新");
    assert_eq!(candidate.release.version.to_string(), "0.10.0");
    assert_eq!(
        candidate.artifact.format,
        crate::update::PackageFormat::PortableZip
    );
    assert_eq!(candidate.urgency, Urgency::Optional);
}

#[test]
fn same_or_older_release_is_up_to_date() {
    let outcome = check_outcome(&stable_release("0.2.1"), "0.2.1", Channel::Stable);
    match outcome {
        CheckOutcome::UpToDate { latest, .. } => {
            assert_eq!(latest.map(|v| v.to_string()).as_deref(), Some("0.2.1"));
        }
        other => panic!("同等版本应已是最新，实际 {other:?}"),
    }
    let outcome = check_outcome(&stable_release("0.2.0"), "0.3.0", Channel::Stable);
    assert!(!outcome.is_available(), "不该提示降级");
}

#[test]
fn missing_manifest_means_up_to_date() {
    let tmp = TempDir::new("checker-empty");
    let source = DirSource::new(tmp.path());
    let current = Version::parse("0.2.1").expect("版本");
    let outcome = UpdateChecker::new(source, target())
        .check(&current, Channel::Stable)
        .expect("没清单不是错误");
    match outcome {
        CheckOutcome::UpToDate { latest, .. } => assert_eq!(latest, None),
        other => panic!("应为已是最新，实际 {other:?}"),
    }
}

#[test]
fn prerelease_is_gated_by_channel_stage() {
    let json = manifest(
        "beta",
        &[release(
            "1.0.0-rc.1",
            "",
            &[artifact("win-x64", "portable-zip", "a.zip", PAYLOAD)],
        )],
    );
    // beta 渠道收 rc
    let outcome = check_outcome(&json, "0.9.0", Channel::Beta);
    assert_eq!(
        outcome.candidate().map(|c| c.release.version.to_string()),
        Some("1.0.0-rc.1".to_string())
    );
    // alpha 发布在 beta 渠道里不收（阶段阶梯）
    let alpha_json = manifest(
        "beta",
        &[release(
            "1.1.0-alpha.1",
            "",
            &[artifact("win-x64", "portable-zip", "a.zip", PAYLOAD)],
        )],
    );
    let outcome = check_outcome(&alpha_json, "0.9.0", Channel::Beta);
    assert!(!outcome.is_available(), "beta 渠道不该收 alpha 预发布");
}

#[test]
fn stable_channel_never_takes_a_prerelease_even_if_the_manifest_claims_stable() {
    let json = manifest(
        "stable",
        &[release(
            "1.0.0-beta.2",
            "",
            &[artifact("win-x64", "portable-zip", "a.zip", PAYLOAD)],
        )],
    );
    let outcome = check_outcome(&json, "0.9.0", Channel::Stable);
    assert!(
        !outcome.is_available(),
        "清单可以写错，版本号不会：正式版渠道不许收预发布"
    );
}

#[test]
fn forced_update_comes_from_force_or_min_supported() {
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            r#","force":true"#,
            &[artifact("win-x64", "portable-zip", "a.zip", PAYLOAD)],
        )],
    );
    assert_eq!(
        check_outcome(&json, "0.2.0", Channel::Stable)
            .candidate()
            .map(|candidate| candidate.urgency),
        Some(Urgency::Forced)
    );

    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            r#","min_supported":"0.2.5""#,
            &[artifact("win-x64", "portable-zip", "a.zip", PAYLOAD)],
        )],
    );
    assert_eq!(
        check_outcome(&json, "0.2.0", Channel::Stable)
            .candidate()
            .map(|candidate| candidate.urgency),
        Some(Urgency::Forced),
        "低于最低支持版本 = 强制"
    );
    assert_eq!(
        check_outcome(&json, "0.2.5", Channel::Stable)
            .candidate()
            .map(|candidate| candidate.urgency),
        Some(Urgency::Optional),
        "刚好等于最低支持版本不算强制"
    );
}

#[test]
fn missing_target_artifact_is_reported_not_hidden() {
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact("linux-x64", "appimage", "a.AppImage", PAYLOAD)],
        )],
    );
    let outcome = check_outcome(&json, "0.2.0", Channel::Stable);
    match outcome {
        CheckOutcome::MissingArtifact { release } => {
            assert_eq!(release.version.to_string(), "0.3.0");
        }
        other => panic!("应为「有新版但没本平台产物」，实际 {other:?}"),
    }
}

#[test]
fn falls_back_to_the_newest_release_that_does_have_the_target() {
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
    let outcome = check_outcome(&json, "0.2.0", Channel::Stable);
    assert_eq!(
        outcome.candidate().map(|c| c.release.version.to_string()),
        Some("0.3.0".to_string()),
        "0.4.0 没 win-x64 产物，应退回到 0.3.0"
    );
}

#[test]
fn channel_mismatch_in_the_source_is_an_error() {
    let tmp = TempDir::new("checker-mismatch");
    tmp.write("stable.json", manifest("beta", &[]).as_bytes());
    let source = crate::update::DirSource::new(tmp.path());
    let current = crate::update::Version::parse("0.2.1").expect("版本");
    let error = UpdateChecker::new(source, target())
        .check(&current, Channel::Stable)
        .expect_err("清单渠道不符应报错");
    assert_eq!(error.kind, UpdateErrorKind::ChannelMismatch);
}

#[test]
fn prepare_stages_the_selected_artifact() {
    let tmp = TempDir::new("checker-prepare");
    tmp.write("a.zip", PAYLOAD);
    tmp.write("stable.json", stable_release("0.3.0").as_bytes());
    let source = crate::update::DirSource::new(tmp.path());
    let checker = UpdateChecker::new(source, target());
    let current = crate::update::Version::parse("0.2.0").expect("版本");
    let outcome = checker
        .check(&current, Channel::Stable)
        .expect("检查不该失败");
    let candidate = outcome.candidate().expect("应有更新");
    let staged = checker
        .prepare(candidate, &tmp.join("staging"))
        .expect("取包应成功");
    assert_eq!(staged.size(), PAYLOAD.len() as u64);
    assert_eq!(checker.target(), target());
    assert!(checker.source().source_id().starts_with("file:"));
}
