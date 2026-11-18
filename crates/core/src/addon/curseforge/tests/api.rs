//! 端点拼装 + 响应解析（纯离线：素材全在 `fixtures.rs` 里）。

use crate::addon::curseforge::{
    CfClass, CfErrorKind, CfLoader, CfSort, CurseForge, ModFileQuery, ModQuery, CF_API,
    MAX_PAGE_SIZE, MINECRAFT_GAME_ID,
};
use crate::download::blocking::block_on;

use super::fixtures;
use super::support::{key, FixtureTransport};

fn query(text: &str) -> ModQuery {
    ModQuery {
        text: text.to_string(),
        class: CfClass::Mods,
        game_version: Some("1.20.1".to_string()),
        loader: Some(CfLoader::Fabric),
        sort: CfSort::TotalDownloads,
        index: 20,
        page_size: 10,
    }
}

#[test]
fn search_url_carries_every_filter_in_the_sites_own_shape() {
    let transport = FixtureTransport::new();
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let url = client.search_url(&ModQuery {
        text: "sodium edge".to_string(),
        ..query("")
    });
    assert!(
        url.starts_with(&format!(
            "{CF_API}/mods/search?gameId={MINECRAFT_GAME_ID}&classId=6&index=20&pageSize=10&sortField=6&sortOrder=desc"
        )),
        "url={url}"
    );
    // 关键词要百分号编码（空格 ⇒ %20），否则站点会把整串当成一个词。
    assert!(url.contains("&searchFilter=sodium%20edge"), "url={url}");
    assert!(url.contains("&gameVersion=1.20.1"), "url={url}");
    // Fabric 的 modLoaderType 是 4（数字只在 `CfLoader::code` 里出现）
    assert!(url.ends_with("&modLoaderType=4"), "url={url}");
}

#[test]
fn an_empty_keyword_and_no_filters_do_not_add_parameters() {
    let transport = FixtureTransport::new();
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let url = client.search_url(&ModQuery::default());
    assert!(
        !url.contains("&searchFilter="),
        "空关键词不该带 searchFilter：{url}"
    );
    assert!(
        !url.contains("&gameVersion="),
        "没选版本不该带 gameVersion：{url}"
    );
    assert!(
        !url.contains("&modLoaderType="),
        "没选加载器不该带 modLoaderType：{url}"
    );
}

#[test]
fn page_size_is_clamped_to_the_site_maximum_and_never_zero() {
    let transport = FixtureTransport::new();
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let too_big = client.search_url(&ModQuery {
        page_size: 999,
        ..ModQuery::default()
    });
    assert!(
        too_big.contains(&format!("pageSize={MAX_PAGE_SIZE}")),
        "上限要钳到 {MAX_PAGE_SIZE}：{too_big}"
    );
    let zero = client.search_url(&ModQuery {
        page_size: 0,
        ..ModQuery::default()
    });
    // 0 会让「本页装满了」永假（翻页卡死），所以钳到 1。
    assert!(zero.contains("pageSize=1"), "url={zero}");
}

#[test]
fn search_parses_hits_and_derives_loaders_from_the_latest_files_indexes() {
    let transport =
        FixtureTransport::new().with(&format!("{CF_API}/mods/search"), fixtures::SEARCH);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let page = block_on(client.search(&query("sodium"))).expect("解析应当成功");

    assert_eq!(page.total_count, 12_345);
    assert_eq!(page.hits.len(), 2);
    let sodium = &page.hits[0];
    assert_eq!(sodium.id, 394_468);
    assert_eq!(sodium.slug, "sodium");
    assert_eq!(sodium.name, "Sodium");
    assert_eq!(sodium.authors, vec!["jellysquid3".to_string()]);
    assert_eq!(sodium.download_count, 42_123_456);
    assert_eq!(sodium.date_modified, "2026-02-03T10:11:12Z");
    // 分类取 slug（`Optimization` 的 slug 是 `optimization`），不是展示名
    assert_eq!(
        sodium.categories,
        vec!["optimization".to_string(), "fabric".to_string()]
    );
    // 游戏版本来自 latestFilesIndexes，去重且保持站点顺序
    assert_eq!(
        sodium.game_versions(),
        vec!["1.21.1".to_string(), "1.20.1".to_string()]
    );
    assert_eq!(
        sodium.loaders(),
        vec!["fabric".to_string(), "neoforge".to_string()]
    );
    assert!(sodium.supports_loader("fabric"));
    assert!(!sodium.supports_loader("quilt"));
    assert!(sodium.supports_version("1.20.1"));
    assert_eq!(
        sodium.icon_url.as_deref(),
        Some("https://media.forgecdn.net/avatars/sodium.png")
    );
    assert_eq!(sodium.latest_files[0].file_id, 5_512_345);

    let jei = &page.hits[1];
    assert!(jei.icon_url.is_none(), "没 logo 就是 None，不编空串");
    assert_eq!(jei.loaders(), vec!["forge".to_string()]);
    assert_eq!(jei.latest_files[0].release_type.as_str(), "beta");
}

#[test]
fn a_full_page_says_there_is_more() {
    let transport =
        FixtureTransport::new().with(&format!("{CF_API}/mods/search"), fixtures::SEARCH);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    // 站点回显 pageSize=2、恰好 2 条 ⇒ 装满了 ⇒ 还有下一页
    let page = block_on(client.search(&ModQuery::default())).expect("解析应当成功");
    assert_eq!(page.page_size, 2);
    assert!(page.has_more());
}

#[test]
fn a_missing_page_size_falls_back_to_the_requested_one_so_paging_does_not_stall() {
    let transport =
        FixtureTransport::new().with(&format!("{CF_API}/mods/search"), fixtures::SEARCH_SPARSE);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);

    // 残页里没有 pagination：用请求值补齐（否则 page_size=0 ⇒ has_more 恒假）
    let exact = block_on(client.search(&ModQuery {
        page_size: 3,
        ..ModQuery::default()
    }))
    .expect("解析应当成功");
    assert_eq!(exact.hits.len(), 3);
    assert_eq!(exact.page_size, 3, "站点没回显时要补成请求值");
    assert!(exact.has_more(), "3 条装满 3 条 ⇒ 还有下一页");

    let roomy = block_on(client.search(&ModQuery {
        page_size: 4,
        ..ModQuery::default()
    }))
    .expect("解析应当成功");
    assert_eq!(roomy.page_size, 4);
    assert!(!roomy.has_more(), "3 条填不满 4 条 ⇒ 没有下一页");
}

#[test]
fn sparse_entries_do_not_fail_the_page() {
    let transport =
        FixtureTransport::new().with(&format!("{CF_API}/mods/search"), fixtures::SEARCH_SPARSE);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let page = block_on(client.search(&ModQuery::default())).expect("缺字段不该让整页失败");
    assert_eq!(page.hits.len(), 3);
    let bare = &page.hits[0];
    assert_eq!(bare.id, 7);
    assert!(bare.name.is_empty());
    assert!(bare.slug.is_empty());
    assert!(bare.icon_url.is_none());
    assert_eq!(bare.game_versions(), Vec::<String>::new());
    assert_eq!(page.hits[1].name, "Only A Name");
    // 空串 logo 与缺失等价
    assert!(page.hits[2].icon_url.is_none());
}

#[test]
fn mod_detail_parses_a_single_object_envelope() {
    let transport =
        FixtureTransport::new().with(&format!("{CF_API}/mods/394468"), fixtures::MOD_DETAIL);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let project = block_on(client.project(394_468)).expect("解析应当成功");
    assert_eq!(project.id, 394_468);
    assert_eq!(project.slug, "sodium");
    assert_eq!(project.loaders(), vec!["fabric".to_string()]);
    assert_eq!(
        transport.seen()[0].url,
        format!("{CF_API}/mods/394468"),
        "详情端点不要带多余参数"
    );
}

#[test]
fn files_url_carries_the_version_and_loader_filters() {
    let transport = FixtureTransport::new();
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let url = client.files_url(
        &ModFileQuery::new(238_222)
            .with_game_version("1.20.1")
            .with_loader(CfLoader::Forge),
    );
    assert!(
        url.starts_with(&format!(
            "{CF_API}/mods/238222/files?index=0&pageSize={MAX_PAGE_SIZE}"
        )),
        "url={url}"
    );
    assert!(
        url.ends_with("&gameVersion=1.20.1&modLoaderType=1"),
        "url={url}"
    );
}

#[test]
fn files_parse_splits_game_versions_from_loader_words_and_keeps_known_hashes() {
    let transport =
        FixtureTransport::new().with(&format!("{CF_API}/mods/238222/files"), fixtures::MOD_FILES);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let files = block_on(client.files(&ModFileQuery::new(238_222))).expect("解析应当成功");
    assert_eq!(files.len(), 2);

    let jei = &files[0];
    // `Forge` / `Client` / `Server` 不是游戏版本
    assert_eq!(
        jei.game_versions,
        vec!["1.20.1".to_string(), "1.20.2".to_string()],
        "gameVersions 是混装的：加载器名与环境标记不许混进版本列表"
    );
    assert_eq!(jei.loaders, vec!["forge".to_string()]);
    // algo 1 = sha1；站点给的是大写，解析归一成小写（sha1 比对要能直接用）
    assert_eq!(jei.sha1(), Some("9f0c6b5a1d2e3f405162738495a6b7c8d9e0f102"));
    assert_eq!(
        jei.hashes.get("md5").map(String::as_str),
        Some("0a1b2c3d4e5f60718293a4b5c6d7e8f9")
    );
    // algo 99 认不出：丢掉这一条，不是让整页失败
    assert!(!jei.hashes.contains_key("99"));
    assert!(jei.has_download_url());
    assert_eq!(jei.release_type.as_str(), "release");
    assert_eq!(jei.file_length, 1_234_567);
    assert!(jei.fits("1.20.1", Some("forge")));
    assert!(!jei.fits("1.21.1", Some("forge")), "1.21.1 不在这个文件里");
    assert!(!jei.fits("1.20.1", Some("fabric")), "这个文件只给 Forge");

    let neo = &files[1];
    assert!(!neo.has_download_url(), "downloadUrl: null ⇒ 没有地址");
    assert_eq!(neo.loaders, vec!["neoforge".to_string()]);
    assert_eq!(neo.release_type.as_str(), "beta");
    assert!(neo.display_name.is_empty());
    assert!(neo.hashes.is_empty());
}

#[test]
fn download_url_uses_the_files_own_url_without_asking_the_site() {
    let transport =
        FixtureTransport::new().with(&format!("{CF_API}/mods/238222/files"), fixtures::MOD_FILES);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let files = block_on(client.files(&ModFileQuery::new(238_222))).expect("解析应当成功");
    let url = block_on(client.download_url(&files[0])).expect("文件自带地址");
    assert_eq!(
        url,
        "https://edge.forgecdn.net/files/4455/667/jei-1.20.1-forge-15.2.0.27.jar"
    );
    assert_eq!(transport.calls(), 1, "自带地址时不该多问一次端点");
}

#[test]
fn download_url_falls_back_to_the_endpoint_when_the_site_left_it_null() {
    let transport = FixtureTransport::new()
        .with(&format!("{CF_API}/mods/238222/files"), fixtures::MOD_FILES)
        .with(
            &format!("{CF_API}/mods/238222/files/4455668/download-url"),
            fixtures::DOWNLOAD_URL,
        );
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let files = block_on(client.files(&ModFileQuery::new(238_222))).expect("解析应当成功");
    let url = block_on(client.download_url(&files[1])).expect("端点应当给出地址");
    assert_eq!(
        url,
        "https://edge.forgecdn.net/files/4455/668/jei-1.21.1-neoforge-19.0.0.1.jar"
    );
    let seen = transport.seen();
    assert_eq!(
        seen.last().expect("至少发过一次").url,
        format!("{CF_API}/mods/238222/files/4455668/download-url")
    );
}

#[test]
fn a_missing_download_url_is_unsupported_and_never_an_empty_string() {
    let transport = FixtureTransport::new().with(
        &format!("{CF_API}/mods/238222/files/4455668/download-url"),
        fixtures::DOWNLOAD_URL_ABSENT,
    );
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let file = crate::addon::curseforge::ModFile {
        id: 4_455_668,
        mod_id: 238_222,
        ..Default::default()
    };
    let error = block_on(client.download_url(&file)).expect_err("没有地址就是失败");
    assert_eq!(error.kind(), &CfErrorKind::Unsupported);
    assert!(
        error.message().contains("第三方分发"),
        "{}",
        error.message()
    );
}

#[test]
fn a_file_without_ids_is_rejected_before_any_request() {
    let transport = FixtureTransport::new();
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let error = block_on(client.download_url(&crate::addon::curseforge::ModFile::default()))
        .expect_err("没有 id 就没有端点");
    assert_eq!(error.kind(), &CfErrorKind::Unsupported);
    assert_eq!(transport.calls(), 0);
}
