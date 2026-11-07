//! 内容库 DTO 的守门测试（M109）。从 `dto/content.rs` 拆出：那个文件已贴 400 行硬上限。

use super::*;

fn hit(game_versions: Vec<String>, loaders: Vec<String>) -> ContentHit {
    ContentHit {
        id: "1".to_string(),
        slug: "s".to_string(),
        title: "t".to_string(),
        summary: String::new(),
        author: String::new(),
        downloads: 0,
        updated: String::new(),
        source: ContentSource::Modrinth,
        game_versions,
        loaders,
        icon: None,
    }
}

#[test]
fn only_file_kinds_have_install_directories() {
    // 抓得住：把整合包也塞一个目录（那等于把「解压+补全+登记实例」偷偷降级成「拷一个文件进 mods」，
    // 正是任务书第 3 条禁止的事）。
    assert_eq!(ContentKind::Modpack.dir(), None);
    assert_eq!(ContentKind::Mod.dir(), Some("mods"));
    assert_eq!(ContentKind::Resourcepack.dir(), Some("resourcepacks"));
    assert_eq!(ContentKind::Shader.dir(), Some("shaderpacks"));
}

#[test]
fn kinds_map_to_the_official_facets_and_class_ids() {
    assert_eq!(ContentKind::Modpack.modrinth_type(), "modpack");
    assert_eq!(ContentKind::Shader.modrinth_type(), "shader");
    // CF 的四个 classId 是官方固定值，抄错就搜不到东西（而且不会报错，只会空列表）
    assert_eq!(ContentKind::Mod.curseforge_class(), 6);
    assert_eq!(ContentKind::Resourcepack.curseforge_class(), 12);
    assert_eq!(ContentKind::Modpack.curseforge_class(), 4471);
    assert_eq!(ContentKind::Shader.curseforge_class(), 6552);
    for kind in ContentKind::ALL {
        assert!(!kind.slug().is_empty());
    }
}

#[test]
fn a_page_keeps_its_filters_and_only_moves_the_cursor() {
    let mut query = ContentQuery::first(ContentKind::Mod);
    query.game_version = Some("1.20.1".to_string());
    query.text = "sodium".to_string();

    let next = query.next_page();
    assert_eq!(next.offset, ContentQuery::PAGE, "游标按一页走");
    assert!(next.same_filters(&query), "换页不改筛选条件");

    let mut other = next.clone();
    other.text = "lithium".to_string();
    assert!(!other.same_filters(&query), "关键词变了就不是同一批条件");
}

#[test]
fn compatibility_checks_treat_missing_site_data_as_unknown_not_incompatible() {
    let empty = hit(Vec::new(), Vec::new());
    // 站点没给数据时不许拦人（否则「不知道」会被画成「不兼容」）
    assert!(empty.supports_version("1.20.1"));
    assert!(empty.supports_loader(LoaderKind::Fabric));

    let filled = hit(vec!["1.20.1".to_string()], vec!["fabric".to_string()]);
    assert!(filled.supports_version("1.20.1"));
    assert!(!filled.supports_version("1.21.9"));
    assert!(filled.supports_loader(LoaderKind::Fabric));
    assert!(!filled.supports_loader(LoaderKind::Forge));
    // 原版没有 facet：不该被加载器过滤误伤
    assert!(filled.supports_loader(LoaderKind::Vanilla));
}

#[test]
fn has_more_is_driven_by_a_full_page() {
    let mut results = ContentResults {
        query: ContentQuery::first(ContentKind::Mod),
        hits: Vec::new(),
        total: 999,
        curseforge: false,
    };
    assert!(!results.has_more(), "空页 = 没有更多了");
    results.hits = vec![hit(Vec::new(), Vec::new()); ContentQuery::PAGE as usize];
    assert!(results.has_more(), "装满一页 = 还有下一页");
}
