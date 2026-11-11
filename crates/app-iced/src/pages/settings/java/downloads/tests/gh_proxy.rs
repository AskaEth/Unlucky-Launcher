//! `download.gh_proxy` 的两条守卫（M197）：取值映射 + 「真的接上了」。
//!
//! 从 `downloads/tests.rs` 拆出来守 400 行硬线（拆法与 `agents::tests` 一致：断言跟着它守的
//! 东西走）。这里只说一件事：设置值必须**改变候选链**，而不只是被读出来。

// --------------------------------------------------------------- P2-3：`download.gh_proxy` 的取值与真消费者

/// 设置原文 → 三态：空 = 跟随内置默认，`off` 一族 = 关，其余 = 覆盖前缀。
///
/// 变异证明：删掉 `gh_proxy_from` 里那段 `matches!(lowered, "off" | …)` ⇒ 本用例红
/// （`off` 会被当成一个前缀 `off`，拼出 `offhttps://…` 这种 URL）。
#[test]
fn the_gh_proxy_setting_maps_to_three_states() {
    use crate::pages::settings::java::downloads::engine::gh_proxy_from;
    use crate::pages::settings::java::downloads::GhProxy;

    assert_eq!(gh_proxy_from(""), GhProxy::Builtin, "空 = 跟随内置默认");
    assert_eq!(gh_proxy_from("   "), GhProxy::Builtin);
    for off in [
        "off", "OFF", " none ", "direct", "Disable", "disabled", "no",
    ] {
        assert_eq!(gh_proxy_from(off), GhProxy::Off, "{off} 应表示关掉");
    }
    assert_eq!(
        gh_proxy_from(" https://gh.example/ "),
        GhProxy::Prefix("https://gh.example/".to_string()),
        "前后空白要去掉，值原样保留"
    );
}

/// **真消费者**：`download.gh_proxy` 落盘 → 引擎档案 → **候选链里真的出现该前缀**。
///
/// 三步都要成立才算「接了」：① 键读得出来；② 档案被覆盖；③ `expand()` 用它生成候选。
/// 只断言 ① 会漏掉「读了但没用」（本 mission 修的就是这一类），只断言 ② 会漏掉「expand 不认
/// 档案里的前缀」。
///
/// 变异证明：让 `engine_with` 忽略 `gh_proxy` 参数（`let profile = profile;`）⇒ ②③ 两条红；
/// 把 `gh_proxy_of_config` 写成恒 `Builtin` ⇒ ① 先红。
#[test]
fn the_gh_proxy_setting_reaches_the_candidate_chain() {
    use crate::pages::settings::java::downloads::engine::engine_with;
    use crate::pages::settings::java::downloads::{gh_proxy_of_config, GhProxy};
    use unlucky_core::download::cache::now_ms;
    use unlucky_core::download::region::{store_cached, RegionProfile};
    use unlucky_core::download::types::{DownloadRequest, ResourceKind};
    use unlucky_core::download::ProgressEmitter;
    use unlucky_core::setting::keys::launcher_schema;
    use unlucky_core::setting::store::SettingStore;

    let dir = std::env::temp_dir().join(format!("ull-gh-proxy-{}-{}", std::process::id(), line!()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("cache")).expect("cache");
    std::fs::create_dir_all(dir.join("config")).expect("config");
    let config = dir.join("config").join("launcher.json");

    // ① 键读得出来
    let mut store = SettingStore::load(&config, launcher_schema());
    store
        .set(
            "download.gh_proxy",
            "https://gh.example/".to_string().into(),
        )
        .expect("schema 认识 download.gh_proxy");
    store.save().expect("写盘");
    assert_eq!(
        gh_proxy_of_config(&config),
        GhProxy::Prefix("https://gh.example/".to_string())
    );

    // region.json 钉成镜像优先：否则 gh-proxy 候选本来就不生成，测不出「设了有没有用」。
    let cache = dir.join("cache");
    store_cached(
        &cache.join("region.json"),
        &RegionProfile::mirror_first(now_ms()),
    )
    .expect("写 region.json");

    let engine = engine_with(
        &cache,
        ProgressEmitter::silent(),
        &gh_proxy_of_config(&config),
    );
    // ② 档案被覆盖
    assert_eq!(
        engine.region().gh_proxy.as_deref(),
        Some("https://gh.example/"),
        "设置值必须覆盖档案里的前缀"
    );

    // ③ 候选链里真的出现这个前缀
    let request = DownloadRequest::new(
        ResourceKind::Library,
        "https://github.com/owner/repo/releases/download/v1/a.jar".to_string(),
        dir.join("a.jar"),
    );
    let plan = engine.plan(&request, 0);
    assert!(
        plan.iter()
            .any(|candidate| candidate.url.starts_with("https://gh.example/")),
        "gh-proxy 候选没出现在候选链里：{:?}",
        plan.iter()
            .map(|candidate| candidate.url.clone())
            .collect::<Vec<_>>()
    );

    // 关掉那一档：一个代理候选都不生成（同一份 region.json，只换设置值）。
    let off = engine_with(&cache, ProgressEmitter::silent(), &GhProxy::Off);
    assert_eq!(off.region().gh_proxy, None, "off 必须把前缀清掉");
    let plan = off.plan(&request, 0);
    assert!(
        !plan
            .iter()
            .any(|candidate| candidate.url.starts_with("https://gh.example/")),
        "关掉之后不该再有 gh-proxy 候选"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
