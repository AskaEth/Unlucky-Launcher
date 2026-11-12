//! 端点拼装 + 响应解析 + 离线分类（纯离线）。

use crate::addon::online::modrinth::{encode, HashAlgorithm, MODRINTH_API};
use crate::addon::online::{
    ContentType, Modrinth, OnlineErrorKind, SearchQuery, SortIndex, VersionQuery,
};
use crate::download::blocking::block_on;

use super::fixtures;
use super::support::FixtureTransport;

fn query(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.to_string(),
        project_type: Some(ContentType::Mod),
        game_version: Some("1.20.1".to_string()),
        loader: Some("fabric".to_string()),
        index: SortIndex::Downloads,
        limit: 2,
        offset: 0,
    }
}

#[test]
fn search_url_carries_facets_and_query_in_the_sites_own_shape() {
    let transport = FixtureTransport::new();
    let client = Modrinth::new(&transport);
    let url = client.search_url(&query("sodium edge"));
    assert!(url.starts_with(&format!(
        "{MODRINTH_API}/search?limit=2&offset=0&index=downloads"
    )));
    // 站点要的是二维数组的 JSON，再整体百分号编码
    let facets = encode(&query("").facets());
    assert!(url.contains(&format!("&facets={facets}")), "url={url}");
    assert!(facets.contains("%5B%5B%22project_type%3Amod%22%5D"));
    assert!(url.ends_with("&query=sodium%20edge"));
}

#[test]
fn empty_text_omits_the_query_parameter() {
    let transport = FixtureTransport::new();
    let client = Modrinth::new(&transport);
    let url = client.search_url(&SearchQuery::default());
    assert!(!url.contains("&query="), "空关键词不该带 query：{url}");
    assert!(
        !url.contains("&facets="),
        "没有过滤条件不该带 facets：{url}"
    );
}

#[test]
fn search_parses_hits_and_filters_fake_loaders() {
    let transport =
        FixtureTransport::new().with(&format!("{MODRINTH_API}/search"), fixtures::SEARCH);
    let client = Modrinth::new(&transport);
    let page = block_on(client.search(&query("sodium"))).expect("解析应当成功");
    assert_eq!(page.total_hits, 1234);
    assert_eq!(page.hits.len(), 2);
    let sodium = &page.hits[0];
    assert_eq!(sodium.project_id, "AANobbMI");
    assert_eq!(sodium.title, "Sodium");
    // `optimization` 不是加载器：只留 fabric / quilt
    assert_eq!(
        sodium.loaders,
        vec!["fabric".to_string(), "quilt".to_string()]
    );
    assert!(sodium.supports_version("1.20.1"));
    assert!(!sodium.supports_version("1.19.2"));
    // 日期读的是 `date_modified`（旧实现读 `updated`，永远是空串）
    assert_eq!(sodium.updated, "2026-02-03T10:11:12Z");
    // 空 icon_url 当「没有」
    assert_eq!(page.hits[1].icon_url, None);
    // limit=2 且回了 2 条 ⇒ 本页装满 ⇒ 还有下一页（判据不是 total_hits）
    assert!(page.has_more());
}

#[test]
fn has_more_is_decided_by_a_full_page() {
    let mut page = crate::addon::online::SearchPage {
        limit: 2,
        hits: vec![Default::default(), Default::default()],
        ..Default::default()
    };
    assert!(page.has_more());
    page.hits.pop();
    assert!(!page.has_more());
}

#[test]
fn project_detail_parses_body_links_and_loader_list() {
    let transport =
        FixtureTransport::new().with(&format!("{MODRINTH_API}/project/sodium"), fixtures::PROJECT);
    let client = Modrinth::new(&transport);
    let project = block_on(client.project("sodium")).expect("解析应当成功");
    assert_eq!(project.id, "AANobbMI");
    assert_eq!(project.follows, 9912);
    assert!(project.body.contains("modern rendering engine"));
    assert_eq!(
        project.loaders,
        vec!["fabric".to_string(), "quilt".to_string()]
    );
    assert_eq!(
        project.source_url.as_deref(),
        Some("https://github.com/CaffeineMC/sodium")
    );
}

#[test]
fn version_list_prefers_the_primary_file() {
    let transport = FixtureTransport::new().with(
        &format!("{MODRINTH_API}/project/sodium/version"),
        fixtures::VERSIONS,
    );
    let client = Modrinth::new(&transport);
    let versions = block_on(client.versions(&VersionQuery {
        project: "sodium".to_string(),
        game_version: Some("1.21.1".to_string()),
        loader: Some("fabric".to_string()),
    }))
    .expect("解析应当成功");
    assert_eq!(versions.len(), 1);
    let file = versions[0].primary_file().expect("必须有文件");
    // 取第一个会拿到 sources.jar：必须是 primary 的那个
    assert_eq!(file.filename, "sodium-fabric-0.6.0.jar");
    assert_eq!(file.size, 2048);
    // 版本列表请求带了两个数组参数（不是被静默丢掉的空过滤）
    let seen = transport.seen();
    assert!(
        seen[0].contains("game_versions=%5B%221.21.1%22%5D"),
        "{}",
        seen[0]
    );
    assert!(
        seen[0].contains("loaders=%5B%22fabric%22%5D"),
        "{}",
        seen[0]
    );
}

#[test]
fn a_hash_shape_mismatch_never_hits_the_network() {
    let transport = FixtureTransport::new();
    let client = Modrinth::new(&transport);
    let error = block_on(client.version_by_hash(HashAlgorithm::Sha512, "not-a-hash"))
        .expect_err("形状不对必须报错");
    assert_eq!(error.kind(), OnlineErrorKind::Unsupported);
    assert!(
        transport.seen().is_empty(),
        "不该发请求：{:?}",
        transport.seen()
    );
}

#[test]
fn offline_is_reported_as_not_connected_not_as_an_empty_page() {
    let transport = FixtureTransport::new().offline(&format!("{MODRINTH_API}/search"));
    let client = Modrinth::new(&transport);
    let error = block_on(client.search(&query("sodium"))).expect_err("没网必须报错");
    assert_eq!(error.kind(), OnlineErrorKind::NotConnected);
    assert!(error.is_offline());
}

#[test]
fn a_broken_payload_is_malformed_not_empty() {
    let transport =
        FixtureTransport::new().with(&format!("{MODRINTH_API}/search"), "{\"hits\": 3}");
    let client = Modrinth::new(&transport);
    let error = block_on(client.search(&query("sodium"))).expect_err("形状不对必须报错");
    assert_eq!(error.kind(), OnlineErrorKind::Malformed);
}

// ------------------------------------------------ 按摘要反查（补全路径的那两个端点行为）

#[test]
fn version_file_lookup_parses_the_recorded_payload() {
    let transport = FixtureTransport::new().with(
        &format!("{MODRINTH_API}/version_file/"),
        fixtures::VERSION_FILE_A,
    );
    let client = Modrinth::new(&transport);
    let version = block_on(client.version_by_hash(HashAlgorithm::Sha512, fixtures::LOOKUP_SHA512))
        .expect("录制响应应当解析成功");
    assert_eq!(version.id, "ver-a");
    assert_eq!(version.game_versions, vec!["1.20.1".to_string()]);

    // 身份判定只认「sha512 逐字相同」的那条：同版本里的 sources.jar 不许被当成它
    let identity = crate::addon::online::complete::identity_of(&version, fixtures::LOOKUP_SHA512)
        .expect("摘要对得上就该给出身份");
    assert_eq!(identity.sha1, fixtures::LOOKUP_SHA1);
    assert_eq!(identity.size, Some(4096));
    let other = crate::addon::online::complete::identity_of(&version, fixtures::OTHER_SHA512)
        .expect("另一个文件也认得出来");
    assert_ne!(other.sha1, fixtures::LOOKUP_SHA1, "两档摘要不许串");
}

#[test]
fn a_hash_the_source_does_not_know_is_not_found_and_not_offline() {
    // 真实 mrpack 大多托管在 CurseForge CDN：Modrinth 对这个 sha512 必然 404。
    let transport = FixtureTransport::new().missing(&format!("{MODRINTH_API}/version_file/"));
    let client = Modrinth::new(&transport);
    let error = block_on(client.version_by_hash(HashAlgorithm::Sha512, &"a1".repeat(64)))
        .expect_err("站点不认得就该报错");
    assert_eq!(error.kind(), OnlineErrorKind::NotFound);
    assert!(!error.is_offline(), "404 不是「没联网」");
}
