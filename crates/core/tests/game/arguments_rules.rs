//! 规则求值、参数展开、natives 选择（launch 直接消费这些结果）。

use std::collections::BTreeMap;

use unlucky_core::game::arguments::{
    matches_simple_pattern, rules_allow, Argument, ArgumentValue, ConditionalArgument, Environment,
    OsRestriction, Rule, RuleAction,
};
use unlucky_core::game::library::Library;

use crate::helpers::parse;

fn env(os: &str, arch: &str) -> Environment {
    Environment {
        os: os.to_owned(),
        os_version: "10.0".to_owned(),
        arch: arch.to_owned(),
        features: BTreeMap::new(),
    }
}

fn allow_os(name: &str) -> Rule {
    Rule {
        action: RuleAction::Allow,
        os: Some(OsRestriction {
            name: Some(name.to_owned()),
            ..OsRestriction::default()
        }),
        features: None,
    }
}

fn disallow_os(name: &str) -> Rule {
    Rule {
        action: RuleAction::Disallow,
        ..allow_os(name)
    }
}

#[test]
fn empty_rules_always_allow() {
    assert!(rules_allow(&[], &env("linux", "x86_64")));
}

#[test]
fn allow_rule_must_match_environment() {
    let rules = [allow_os("windows")];
    assert!(rules_allow(&rules, &env("windows", "x86_64")));
    assert!(!rules_allow(&rules, &env("linux", "x86_64")));
}

#[test]
fn later_rules_override_earlier_ones() {
    // 白名单语义：列表非空时从 DISALLOW 起步。
    let allow_after_disallow = [disallow_os("windows"), allow_os("windows")];
    assert!(rules_allow(
        &allow_after_disallow,
        &env("windows", "x86_64")
    ));

    let disallow_after_allow = [allow_os("windows"), disallow_os("windows")];
    assert!(!rules_allow(
        &disallow_after_allow,
        &env("windows", "x86_64")
    ));
}

#[test]
fn feature_gate_blocks_argument() {
    let mut features = BTreeMap::new();
    features.insert("has_custom_resolution".to_owned(), true);
    let rule = Rule {
        action: RuleAction::Allow,
        os: None,
        features: Some(features),
    };

    let enabled = env("windows", "x86_64").with_feature("has_custom_resolution", true);
    let disabled = env("windows", "x86_64");

    assert_eq!(rule.action_in(&enabled), Some(RuleAction::Allow));
    assert_eq!(rule.action_in(&disabled), None);
}

#[test]
fn host_environment_is_mojang_shaped() {
    let host = Environment::host();
    assert!(matches!(host.os.as_str(), "windows" | "osx" | "linux"));
    assert!(!host.arch.is_empty());
    assert!(!host.feature("has_custom_resolution"));
    // 空串是「没探」，真值来自 platform::os_info::current()；只要探得到就不能再是空的。
    let probed = unlucky_core::platform::os_info::current();
    assert_eq!(host.os_version, probed.os_version());
    assert_eq!(host.os, probed.family.manifest_name());
    assert_eq!(host.arch, probed.arch);
}

#[test]
fn environment_from_an_injected_os_snapshot_carries_os_version() {
    use unlucky_core::platform::linux_caps::SessionKind;
    use unlucky_core::platform::os_info::{support_level_for, OsFamily, OsInfo};
    use unlucky_core::platform::vk_probe::VkProbe;

    let fake = OsInfo {
        family: OsFamily::Windows,
        arch: "x86_64".to_string(),
        major_minor: Some((10, 0)),
        build: Some(19_045),
        product: "Windows 10".to_string(),
        version_string: "10.0.19045".to_string(),
        session: SessionKind::Unknown,
        gl: unlucky_core::platform::linux_caps::GlProbe::Unknown,
        vk: VkProbe::Available,
        kernel: None,
        glibc: None,
        support_level: support_level_for(OsFamily::Windows, Some((10, 0)), None, None),
    };

    let environment = Environment::from_os_info(&fake);
    assert_eq!(environment.os, "windows");
    assert_eq!(environment.os_version, "10.0.19045");
    assert_eq!(environment.arch, "x86_64");
    assert!(environment.features.is_empty(), "注入快照不该带特性开关");

    // 系统版本真的参与规则求值（这正是不再用空串的原因）。
    let rule = Rule {
        os: Some(OsRestriction {
            name: None,
            version: Some("^10\\.".to_string()),
            arch: None,
        }),
        ..Rule::default()
    };
    assert_eq!(rule.action_in(&environment), Some(RuleAction::Allow));

    let mac_rule = Rule {
        os: Some(OsRestriction {
            name: None,
            version: Some("^11\\.".to_string()),
            arch: None,
        }),
        ..Rule::default()
    };
    assert_eq!(mac_rule.action_in(&environment), None);
}

#[test]
fn os_version_patterns_match_conservatively() {
    assert!(matches_simple_pattern("^10\\.", "10.0"));
    assert!(!matches_simple_pattern("^10\\.", "11.0"));
    assert!(matches_simple_pattern("x86_64", "x86_64"));
    assert!(!matches_simple_pattern("x86_64", "x86"));
    assert!(matches_simple_pattern("10\\.0$", "10.0"));
    // 不认识的复杂正则放宽为匹配（宁可多带参数也不漏参数）。
    assert!(matches_simple_pattern("^(10|11)\\.", "11.0"));
}

#[test]
fn expand_arguments_by_environment() {
    let version = parse(
        r#"{
          "id": "x",
          "arguments": {
            "game": [
              "--username",
              { "rules": [{ "action": "allow", "features": { "is_demo_user": true } }],
                "value": "--demo" },
              { "value": ["--pair", "1"] }
            ],
            "jvm": [ "-Dfoo=1" ]
          }
        }"#,
    );
    let arguments = version.arguments.as_ref().expect("arguments 应解析");

    let plain = arguments.expand(&env("windows", "x86_64"));
    assert_eq!(plain.game, vec!["--username", "--pair", "1"]);
    assert_eq!(plain.jvm, vec!["-Dfoo=1"]);

    let demo = env("windows", "x86_64").with_feature("is_demo_user", true);
    assert!(arguments.expand(&demo).game.contains(&"--demo".to_owned()));
}

#[test]
fn conditional_argument_expands_single_value() {
    let argument = Argument::Conditional(ConditionalArgument {
        rules: vec![allow_os("windows")],
        value: ArgumentValue::One("--single".to_owned()),
    });
    let mut out = Vec::new();
    argument.expand_into(&env("windows", "x86_64"), &mut out);
    assert_eq!(out, vec!["--single"]);

    let mut out = Vec::new();
    argument.expand_into(&env("linux", "x86_64"), &mut out);
    assert!(out.is_empty());
}

#[test]
fn native_classifier_expands_arch() {
    let version = parse(
        r#"{ "id": "x",
             "libraries": [{
               "name": "org.lwjgl:lwjgl:3.3.2",
               "natives": { "windows": "natives-windows-${arch}", "linux": "natives-linux" },
               "downloads": {
                 "artifact": { "path": "org/lwjgl/lwjgl/3.3.2/lwjgl-3.3.2.jar" },
                 "classifiers": {
                   "natives-windows-64": { "path": "org/lwjgl/lwjgl/3.3.2/lwjgl-3.3.2-natives-windows-64.jar" }
                 }
               }
             }]
           }"#,
    );
    let library = &version.libraries.as_ref().expect("libraries 应解析")[0];

    assert_eq!(
        library.native_classifier(&env("windows", "x86_64")),
        Some("natives-windows-64".to_owned())
    );
    assert_eq!(
        library.native_classifier(&env("windows", "x86")),
        Some("natives-windows-".to_owned())
    );
    assert_eq!(
        library.native_classifier(&env("linux", "x86_64")),
        Some("natives-linux".to_owned())
    );
    assert_eq!(library.native_classifier(&env("osx", "arm64")), None);

    let classifier = "natives-windows-64";
    assert!(library.native_artifact(classifier).is_some());
    assert_eq!(
        library
            .native_relative_path(classifier)
            .expect("路径应可推导"),
        std::path::PathBuf::from("org/lwjgl/lwjgl/3.3.2/lwjgl-3.3.2-natives-windows-64.jar")
    );
}

#[test]
fn library_rules_and_local_hint() {
    let library = Library {
        name: "com.example:inst:1".to_owned(),
        rules: vec![allow_os("linux")],
        hint: Some("local".to_owned()),
        ..Library::default()
    };
    assert!(library.applies(&env("linux", "x86_64")));
    assert!(!library.applies(&env("windows", "x86_64")));
    assert!(library.is_local());
    assert_eq!(
        library.relative_path().expect("路径应可推导"),
        std::path::PathBuf::from("com/example/inst/1/inst-1.jar")
    );
}
