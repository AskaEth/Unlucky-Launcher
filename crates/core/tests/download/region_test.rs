//! 地区判定的单测：只测纯判定与缓存，不真发探测请求。

use unlucky_core::download::region::{
    decide_from, load_cached, store_cached, RegionMode, RegionProfile, RegionTrigger, Signals,
    DECISION_TTL_MS, DEFAULT_GH_PROXY,
};

fn signals(timezone: Option<&str>, language: Option<&str>, google: Option<bool>) -> Signals {
    Signals {
        timezone: timezone.map(str::to_string),
        languages: language.map(str::to_string).into_iter().collect(),
        google_reachable: google,
        baidu_reachable: None,
    }
}

#[test]
fn timezone_signal_detects_greater_china() {
    assert!(signals(Some("Asia/Shanghai"), None, None).is_greater_china());
    assert!(signals(Some("Asia/Taipei"), None, None).is_greater_china());
    // 大小写/空白容错。
    assert!(signals(Some(" asia/hong_kong "), None, None).is_greater_china());
    assert!(!signals(Some("Europe/Berlin"), None, None).is_greater_china());
}

#[test]
fn language_signal_handles_posix_and_bcp47() {
    assert!(signals(None, Some("zh_CN.UTF-8"), None).is_greater_china());
    assert!(signals(None, Some("zh-Hant-TW"), None).is_greater_china());
    assert!(!signals(None, Some("en_US.UTF-8"), None).is_greater_china());
    // 时区不命中、语言命中也算。
    assert!(signals(Some("UTC"), Some("zh_TW"), None).is_greater_china());
}

#[test]
fn domestic_without_google_goes_mirror_first() {
    let profile = decide_from(&signals(Some("Asia/Shanghai"), None, Some(false)), 1);
    assert_eq!(profile.mode, RegionMode::MirrorFirst);
    assert!(profile.greater_china);
    assert_eq!(profile.trigger, RegionTrigger::Auto);
    assert_eq!(profile.gh_proxy.as_deref(), Some(DEFAULT_GH_PROXY));
    assert!(profile.gh_proxy_enabled());
}

#[test]
fn domestic_with_google_reachable_stays_direct() {
    // 已有全局代理：尊重用户代理，不再插镜像。
    let profile = decide_from(&signals(Some("Asia/Shanghai"), None, Some(true)), 1);
    assert_eq!(profile.mode, RegionMode::Direct);
    assert!(profile.gh_proxy.is_none());
    assert!(!profile.gh_proxy_enabled());
}

#[test]
fn non_greater_china_is_always_direct() {
    for google in [Some(true), Some(false), None] {
        let profile = decide_from(&signals(Some("America/New_York"), Some("en-US"), google), 1);
        assert_eq!(profile.mode, RegionMode::Direct, "google={google:?}");
        assert!(!profile.greater_china);
    }
}

#[test]
fn probes_disabled_defaults_to_mirror_first_for_chinese_env() {
    let profile = decide_from(&signals(None, Some("zh_CN.UTF-8"), None), 1);
    assert_eq!(profile.mode, RegionMode::MirrorFirst);
    assert!(!profile.probes_enabled);
}

#[test]
fn pinned_mode_never_goes_stale() {
    let pinned = RegionProfile::pinned(RegionMode::Direct, 10);
    assert_eq!(pinned.trigger, RegionTrigger::UserPinned);
    assert!(!pinned.is_stale(10 + DECISION_TTL_MS * 10));

    let auto = decide_from(&signals(Some("Asia/Shanghai"), None, Some(false)), 10);
    assert!(!auto.is_stale(10 + 1_000));
    assert!(auto.is_stale(10 + DECISION_TTL_MS + 1));
}

#[test]
fn gh_proxy_helpers() {
    let mut profile = RegionProfile::mirror_first(0);
    assert!(profile.gh_proxy_enabled());
    profile = profile.with_gh_proxy(Some("   "));
    assert!(!profile.gh_proxy_enabled(), "空白前缀视为不启用");
    profile = profile.with_gh_proxy(Some("https://proxy.example/"));
    assert!(profile.gh_proxy_enabled());

    // 直连模式下即便配了前缀也不生效（gh-proxy 只在国内模式用）。
    let direct = RegionProfile::direct(0).with_gh_proxy(Some("https://proxy.example/"));
    assert!(!direct.gh_proxy_enabled());
    assert_eq!(direct.label(), "直连");
    assert_eq!(RegionProfile::mirror_first(0).label(), "镜像优先");
}

#[test]
fn cache_roundtrip_and_ttl_expiry() {
    let dir = std::env::temp_dir().join(format!("ull-region-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("region.json");

    let profile = decide_from(&signals(Some("Asia/Shanghai"), None, Some(false)), 1_000);
    store_cached(&path, &profile).expect("写缓存失败");

    let cached = load_cached(&path, 1_000 + 60_000).expect("应有缓存");
    assert_eq!(cached, profile);

    // 过期 → 视为没有缓存。
    assert!(load_cached(&path, 1_000 + DECISION_TTL_MS + 1).is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_cache_file_is_not_an_error() {
    let path = std::env::temp_dir().join("ull-region-does-not-exist.json");
    let _ = std::fs::remove_file(&path);
    assert!(load_cached(&path, 0).is_none());
}
