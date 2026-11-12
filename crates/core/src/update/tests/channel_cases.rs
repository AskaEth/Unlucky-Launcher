//! 渠道阶梯与准入：清单渠道比大小，预发布阶段按表收。

use crate::update::{Channel, Stage};

#[test]
fn parses_names_loosely() {
    assert_eq!(Channel::parse("stable").expect("stable"), Channel::Stable);
    assert_eq!(Channel::parse(" STABLE ").expect("大写"), Channel::Stable);
    assert_eq!(Channel::parse("Beta").expect("beta"), Channel::Beta);
    assert_eq!(
        Channel::parse("nightly").expect("nightly"),
        Channel::Nightly
    );
    let error = Channel::parse("canary").expect_err("canary 不认");
    assert!(error.to_string().contains("canary"));
    assert_eq!(
        Channel::ALL.map(Channel::as_str),
        ["stable", "beta", "alpha", "nightly"]
    );
}

#[test]
fn ladder_admits_only_more_stable_releases() {
    assert!(Channel::Stable.admits(Channel::Stable));
    assert!(!Channel::Stable.admits(Channel::Beta));
    assert!(Channel::Beta.admits(Channel::Stable));
    assert!(Channel::Beta.admits(Channel::Beta));
    assert!(!Channel::Beta.admits(Channel::Alpha));
    assert!(!Channel::Alpha.admits(Channel::Nightly));
    assert!(Channel::Alpha.admits(Channel::Beta));
    assert!(Channel::Nightly.admits(Channel::Alpha));
    assert!(Channel::Nightly.admits(Channel::Stable));
}

#[test]
fn prerelease_stages_are_gated_by_channel() {
    // 正式版对所有渠道都成立
    for channel in Channel::ALL {
        assert!(channel.admits_stage(Stage::Stable), "{channel}");
    }
    assert!(!Channel::Stable.admits_stage(Stage::ReleaseCandidate));
    assert!(Channel::Beta.admits_stage(Stage::ReleaseCandidate));
    assert!(!Channel::Beta.admits_stage(Stage::Beta));
    assert!(Channel::Alpha.admits_stage(Stage::Beta));
    assert!(!Channel::Alpha.admits_stage(Stage::Alpha));
    assert!(!Channel::Alpha.admits_stage(Stage::Unknown));
    assert!(Channel::Nightly.admits_stage(Stage::Alpha));
    assert!(Channel::Nightly.admits_stage(Stage::Unknown));
}

#[test]
fn display_and_from_str_agree() {
    for channel in Channel::ALL {
        assert_eq!(channel.as_str().parse::<Channel>().expect("回读"), channel);
        assert_eq!(channel.to_string(), channel.as_str());
    }
}
