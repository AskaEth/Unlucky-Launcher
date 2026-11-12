//! 接线：`rules.os.version` 用真实探测值求值（D19 §2 —— 之前 `os_version` 是空串，
//! 带版本规则的参数项永远命中不了）。

use crate::prelude::*;

fn argv_of(entries: Vec<ArgumentEntry>, ctx: &PlatformCtx) -> Vec<String> {
    let request = request_with_game(entries);
    assemble(&request, ctx).expect("组装失败").argv().to_vec()
}

#[test]
fn windows_10_rule_hits_with_detected_version() {
    let ctx = PlatformCtx::new(OsFamily::Windows, "x86_64").with_os_version("10.0.19045");
    let entries = vec![version_ruled_entry(r"^10\.", "--demo-windows10")];
    let argv = argv_of(entries, &ctx);
    assert!(
        argv.iter().any(|arg| arg == "--demo-windows10"),
        "argv 应包含带版本规则的参数: {argv:?}"
    );
}

#[test]
fn same_rule_misses_when_version_is_empty() {
    // 空版本串是修复前的常态：规则求值走保守分支（不放参数）。
    let ctx = PlatformCtx::new(OsFamily::Windows, "x86_64");
    assert!(ctx.os_version.is_empty());
    let entries = vec![version_ruled_entry(r"^10\.", "--demo-windows10")];
    let argv = argv_of(entries, &ctx);
    assert!(
        !argv.iter().any(|arg| arg == "--demo-windows10"),
        "空版本串不该命中: {argv:?}"
    );
}

#[test]
fn escaped_dot_patterns_match_real_versions() {
    // `^10\.15` 是 macOS 上常见的写法：反转义后按 `10.15` 前缀比较。
    let ctx = PlatformCtx::new(OsFamily::MacOs, "x86_64").with_os_version("10.15.7");
    let entries = vec![version_ruled_entry(r"^10\.15", "--demo-catalina")];
    let argv = argv_of(entries, &ctx);
    assert!(argv.iter().any(|arg| arg == "--demo-catalina"), "{argv:?}");
}

#[test]
fn regex_shaped_patterns_stay_conservative() {
    let ctx = PlatformCtx::new(OsFamily::Windows, "x86_64").with_os_version("10.0.19045");
    // 需要真正则的模式：宁可少加参数，也不猜。
    let entries = vec![version_ruled_entry(r"^10\.[0-9]+", "--demo-regex")];
    let argv = argv_of(entries, &ctx);
    assert!(!argv.iter().any(|arg| arg == "--demo-regex"), "{argv:?}");
}

#[test]
fn rule_context_uses_the_injected_version() {
    let features = FeatureSet::default();
    let entry = version_ruled_entry(r"^6\.1\.", "--demo-win7");
    assert!(entry.applies(&RuleContext::new(
        "windows", "x86_64", "6.1.7601", &features
    )));
    assert!(!entry.applies(&RuleContext::new(
        "windows",
        "x86_64",
        "10.0.19045",
        &features
    )));
    // 空版本串：任何版本规则都不命中。
    assert!(!entry.applies(&RuleContext::new("windows", "x86_64", "", &features)));
}

#[test]
fn platform_ctx_current_carries_the_probed_version() {
    let ctx = PlatformCtx::current();
    let info = os_info::current();
    assert_eq!(ctx.os, info.family);
    assert_eq!(ctx.arch, info.arch);
    assert_eq!(ctx.os_version, info.os_version());
    if cfg!(target_os = "windows") {
        assert!(!ctx.os_version.is_empty(), "Windows 上版本串不能为空");
        // 版本串应能被规则前缀匹配到（形如 10.0.19045）。
        assert!(
            ctx.os_version.starts_with(|c: char| c.is_ascii_digit()),
            "版本串形态异常: {}",
            ctx.os_version
        );
    }
}

#[test]
fn default_matches_current() {
    let ctx = PlatformCtx::default();
    assert_eq!(ctx.os_version, PlatformCtx::current().os_version);
    assert_eq!(
        ctx.path_separator(),
        PlatformCtx::current().path_separator()
    );
}
