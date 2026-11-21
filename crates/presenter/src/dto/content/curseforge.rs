//! CurseForge 的远端条目 → 内容库 DTO（M316）。
//!
//! # 为什么这一层薄
//!
//! 站点的 JSON 方言、端点拼装、错误分类全在 `unlucky_core::addon::curseforge`：这里是
//! `dto` 层该做的事——**字段搬运 + 一一映射**，不重新判断任何东西（与 Modrinth 那一半在
//! `host::addon::sources` 里的做法同源，只是 CF 现在的解析也在 core）。
//!
//! 三件事刻意放在这里：
//!
//! 1. **两条源汇到同一个 DTO**（[`ContentHit`] / [`ContentVersionView`]）：页面不为来源写两套渲染；
//! 2. **失败一一映射**（[`cf_failure`]）：`CfErrorKind` 有几档，DTO 就有几档对得上，**不靠字符串猜**；
//! 3. **「有没有 Key」变成一个 `bool`**（[`cf_key_available`]）：`ContentResults::curseforge` 那个
//!    字段就是它，页面据此决定检索按钮能不能点。
//!
//! # 接线状态（M317 起）
//!
//! 本文件是**唯一**的 CF → DTO 映射点，调用方三处、都在生产路径上：
//!
//! | 调用方 | 用到的映射 |
//! |---|---|
//! | `host::addon::sources::curseforge_page` | [`hit_of_cf`] + [`cf_page_total`] + [`cf_failure`] |
//! | `host::addon::sources::cf_available` | [`cf_key_available`]（`ContentResults::curseforge` 那个 `bool`） |
//! | `host::addon::sources::detail` | 仍是明确的 `Unsupported`（CF 详情/版本列表未接，见 `sources.rs` 头） |
//!
//! （M316 落这一层时它还没有调用者——「本切片不接线」那段已过时。）
//!
//! # 两个字段的口径
//!
//! - `number`：CF **没有**独立的版本号字段（`displayName` 就是全部）。这里给空串 ⇒ 「站点没给」，
//!   与 [`ContentHit::author`] 缺失时给空串同一惯例；**不**从文件名里猜一个版本号出来。
//! - `published` / `updated`：站点原文是 ISO-8601 含时分秒，DTO 只要日期部分
//!   （[`super::date_part`]，与 Modrinth 那条路同一个实现）。

use unlucky_core::addon::curseforge::{CfError, CfErrorKind, CfKeyState, Mod, ModFile, ModPage};

use super::{
    date_part, ContentFailure, ContentFailureKind, ContentHit, ContentSource, ContentVersionView,
};

/// 一条 CF 检索命中 → [`ContentHit`]。
#[must_use]
pub fn hit_of_cf(hit: &Mod) -> ContentHit {
    ContentHit {
        // CF 的 id 是数字，DTO 的 id 是字符串（两条源同一个字段）：十进制即可。
        id: hit.id.to_string(),
        slug: hit.slug.clone(),
        title: hit.name.clone(),
        summary: hit.summary.clone(),
        author: hit.authors.first().cloned().unwrap_or_default(),
        downloads: hit.download_count,
        updated: date_part(&hit.date_modified),
        source: ContentSource::Curseforge,
        game_versions: hit.game_versions(),
        loaders: hit.loaders(),
        icon: hit.icon_url.clone(),
    }
}

/// 一页 CF 命中 → 命中总数（`u32`）。
///
/// 站点给的是 `u64`，DTO 是 `u32`：溢出时**饱和**到 `u32::MAX` 而不是回 0（回 0 会让页面把
/// 「有几百万条」画成「一条都没有」）。
#[must_use]
pub fn cf_page_total(page: &ModPage) -> u32 {
    u32::try_from(page.total_count).unwrap_or(u32::MAX)
}

/// 一个 CF 文件 → 版本列表的一行。
#[must_use]
pub fn cf_version(file: &ModFile) -> ContentVersionView {
    let label = if file.display_name.trim().is_empty() {
        file.file_name.clone()
    } else {
        file.display_name.clone()
    };
    ContentVersionView {
        id: file.id.to_string(),
        name: label,
        // 见模块文档：CF 没有独立的版本号字段，不猜。
        number: String::new(),
        kind: file.release_type.as_str().to_string(),
        game_versions: file.game_versions.clone(),
        loaders: file.loaders.clone(),
        published: date_part(&file.file_date),
        file_name: Some(file.file_name.clone()).filter(|name| !name.trim().is_empty()),
        file_size: Some(file.file_length).filter(|size| *size > 0),
    }
}

/// 一页 CF 文件 → 版本列表（顺序 = 站点顺序，最新在前）。
#[must_use]
pub fn cf_version_list(files: &[ModFile]) -> Vec<ContentVersionView> {
    files.iter().map(cf_version).collect()
}

/// core 的 CF 错误 → DTO 的失败大类（**一一映射，不重新判**）。
///
/// | `CfErrorKind` | `ContentFailureKind` | 为什么 |
/// |---------------|----------------------|--------|
/// | `NotConfigured` | `Unsupported` | 「缺密钥」这一档的既有归属（该字段的文档原话） |
/// | `InvalidKey` | `Unsupported` | 同上：都是「去改配置」，不是「换个关键词」 |
/// | `RateLimited` | `Server` | 该档文档写明含「限速」：等一会儿再来，不是改条件 |
/// | `Network` | `Offline` | 页面画独立的离线态 |
/// | `NotFound` | `NotFound` | 换关键词 |
/// | `Server` | `Server` | 站点那边的事 |
/// | `Malformed` / `Unsupported` | `Other` / `Unsupported` | 我们自己的问题 / 这条路不通 |
#[must_use]
pub fn cf_failure(error: CfError) -> ContentFailure {
    let kind = match error.kind() {
        CfErrorKind::NotConfigured | CfErrorKind::InvalidKey => ContentFailureKind::Unsupported,
        CfErrorKind::RateLimited { .. } => ContentFailureKind::Server,
        CfErrorKind::Network => ContentFailureKind::Offline,
        CfErrorKind::NotFound => ContentFailureKind::NotFound,
        CfErrorKind::Server => ContentFailureKind::Server,
        CfErrorKind::Malformed => ContentFailureKind::Other,
        CfErrorKind::Unsupported => ContentFailureKind::Unsupported,
    };
    ContentFailure::new(kind, error.message().to_string())
}

/// Key 配好了吗（= `ContentResults::curseforge` 那个 `bool`）。
#[must_use]
pub fn cf_key_available(state: CfKeyState) -> bool {
    state.is_configured()
}

#[cfg(test)]
mod tests {
    use unlucky_core::addon::curseforge::{
        CfError, CfErrorKind, CfKeyState, CfLoader, FileIndex, Mod, ModFile,
    };

    use super::{
        cf_failure, cf_key_available, cf_page_total, cf_version, cf_version_list, hit_of_cf,
    };
    use crate::dto::content::ContentFailureKind;
    use crate::dto::ContentSource;

    fn project() -> Mod {
        Mod {
            id: 394_468,
            name: "Sodium".to_string(),
            slug: "sodium".to_string(),
            summary: "Modern rendering engine".to_string(),
            authors: vec!["jellysquid3".to_string(), "其他".to_string()],
            download_count: 42_123_456,
            date_modified: "2026-02-03T10:11:12Z".to_string(),
            categories: vec!["optimization".to_string()],
            latest_files: vec![
                FileIndex {
                    game_version: Some("1.21.1".to_string()),
                    loader: Some(CfLoader::Fabric),
                    file_id: 1,
                    filename: "sodium-fabric.jar".to_string(),
                    release_type: Default::default(),
                },
                FileIndex {
                    game_version: Some("1.20.1".to_string()),
                    loader: Some(CfLoader::NeoForge),
                    file_id: 2,
                    filename: "sodium-neoforge.jar".to_string(),
                    release_type: Default::default(),
                },
            ],
            icon_url: Some("https://media.forgecdn.net/avatars/sodium.png".to_string()),
        }
    }

    fn file() -> ModFile {
        ModFile {
            id: 5_512_345,
            mod_id: 394_468,
            display_name: "sodium-fabric-0.6.0+mc1.21.1".to_string(),
            file_name: "sodium-fabric-0.6.0+mc1.21.1.jar".to_string(),
            file_date: "2026-02-03T10:11:12Z".to_string(),
            file_length: 1_234_567,
            game_versions: vec!["1.21.1".to_string()],
            loaders: vec!["fabric".to_string()],
            ..Default::default()
        }
    }

    /// 一个 `CfError::from_status` 造出来的错误（CF 的构造点只有这一个真来源）。
    fn error(status: u16, retry_after: Option<u64>) -> CfError {
        CfError::from_status(status, retry_after)
    }

    #[test]
    fn a_cf_hit_maps_onto_the_shared_content_hit_shape() {
        let hit = hit_of_cf(&project());
        assert_eq!(hit.id, "394468", "CF 的数字 id 在 DTO 里是十进制字符串");
        assert_eq!(hit.slug, "sodium");
        assert_eq!(hit.title, "Sodium");
        assert_eq!(hit.summary, "Modern rendering engine");
        assert_eq!(hit.author, "jellysquid3", "作者取第一个，不拼一长串");
        assert_eq!(hit.downloads, 42_123_456);
        assert_eq!(hit.updated, "2026-02-03", "DTO 只留日期部分");
        assert_eq!(hit.source, ContentSource::Curseforge);
        assert_eq!(
            hit.game_versions,
            vec!["1.21.1".to_string(), "1.20.1".to_string()]
        );
        assert_eq!(
            hit.loaders,
            vec!["fabric".to_string(), "neoforge".to_string()]
        );
        assert_eq!(
            hit.icon.as_deref(),
            Some("https://media.forgecdn.net/avatars/sodium.png")
        );
        // 两条源的 DTO 判断题在同一个类型上，所以「装不装得上」不必分源
        assert!(hit.supports_loader(crate::dto::LoaderKind::Fabric));
        assert!(!hit.supports_loader(crate::dto::LoaderKind::Quilt));
        assert!(hit.supports_version("1.20.1"));
    }

    #[test]
    fn a_sparse_cf_hit_maps_to_empty_fields_instead_of_invented_ones() {
        let bare = Mod {
            id: 7,
            ..Default::default()
        };
        let hit = hit_of_cf(&bare);
        assert_eq!(hit.id, "7");
        assert!(hit.author.is_empty());
        assert!(hit.updated.is_empty());
        assert!(hit.icon.is_none());
        assert!(hit.game_versions.is_empty());
        // 空列表 = 站点没给 ⇒ 不拦（与 Modrinth 同一条纪律）
        assert!(hit.supports_loader(crate::dto::LoaderKind::Forge));
        assert!(hit.supports_version("1.20.1"));
    }

    #[test]
    fn a_cf_file_maps_onto_the_version_row_the_page_draws() {
        let version = cf_version(&file());
        assert_eq!(version.id, "5512345");
        assert_eq!(version.name, "sodium-fabric-0.6.0+mc1.21.1");
        assert_eq!(version.number, "", "CF 没有独立版本号：不猜");
        assert_eq!(version.kind, "release");
        assert_eq!(version.game_versions, vec!["1.21.1".to_string()]);
        assert_eq!(version.loaders, vec!["fabric".to_string()]);
        assert_eq!(version.published, "2026-02-03");
        assert_eq!(
            version.file_name.as_deref(),
            Some("sodium-fabric-0.6.0+mc1.21.1.jar")
        );
        assert_eq!(version.file_size, Some(1_234_567));
    }

    #[test]
    fn a_file_without_a_display_name_falls_back_to_its_file_name() {
        let blank = ModFile {
            display_name: String::new(),
            ..file()
        };
        assert_eq!(cf_version(&blank).name, "sodium-fabric-0.6.0+mc1.21.1.jar");
    }

    #[test]
    fn empty_file_metadata_becomes_none_not_zero() {
        let none = ModFile {
            display_name: String::new(),
            file_name: String::new(),
            file_length: 0,
            file_date: String::new(),
            ..file()
        };
        let version = cf_version(&none);
        assert!(
            version.file_name.is_none(),
            "空文件名是 None，不是 Some(\"\")"
        );
        assert!(
            version.file_size.is_none(),
            "0 字节是 None（站点没给），不是 0"
        );
        assert!(version.published.is_empty());
    }

    #[test]
    fn the_version_list_keeps_the_site_order() {
        let files = vec![file(), ModFile { id: 2, ..file() }];
        let versions = cf_version_list(&files);
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].id, "5512345");
        assert_eq!(versions[1].id, "2");
    }

    #[test]
    fn the_page_total_saturates_instead_of_wrapping_to_zero() {
        use unlucky_core::addon::curseforge::ModPage;
        let page = ModPage {
            total_count: u64::from(u32::MAX) + 5,
            ..Default::default()
        };
        assert_eq!(cf_page_total(&page), u32::MAX, "溢出不许变成 0");
        let page = ModPage {
            total_count: 12,
            ..Default::default()
        };
        assert_eq!(cf_page_total(&page), 12);
    }

    #[test]
    fn every_cf_error_kind_has_its_own_dto_class() {
        // 缺 Key 与 Key 无效都归「这条路走不通」（该档文档原话就是「缺密钥」），
        // 但它们的 detail 不同，页面仍能说清是哪一种。
        let missing = cf_failure(CfError::not_configured());
        assert_eq!(missing.kind, ContentFailureKind::Unsupported);
        assert!(missing.detail.contains("API Key"));

        let invalid = cf_failure(error(403, None));
        assert_eq!(invalid.kind, ContentFailureKind::Unsupported);
        assert!(invalid.detail.contains("403"));

        // 限流落在 Server：等一会儿再来，不是改筛选条件
        let limited = cf_failure(error(429, Some(7)));
        assert_eq!(limited.kind, ContentFailureKind::Server);
        assert!(limited.detail.contains("Retry-After=7s"));

        assert_eq!(
            cf_failure(CfError::network("连不上")).kind,
            ContentFailureKind::Offline
        );
        assert_eq!(
            cf_failure(error(404, None)).kind,
            ContentFailureKind::NotFound
        );
        assert_eq!(
            cf_failure(error(500, None)).kind,
            ContentFailureKind::Server
        );
        assert_eq!(
            cf_failure(CfError::malformed("不是 JSON")).kind,
            ContentFailureKind::Other
        );
        assert_eq!(
            cf_failure(CfError::unsupported("没地址")).kind,
            ContentFailureKind::Unsupported
        );
    }

    #[test]
    fn the_key_state_becomes_the_bool_the_page_asks_for() {
        assert!(!cf_key_available(CfKeyState::Missing));
        assert!(cf_key_available(CfKeyState::Configured));
        // 与排序 / 类型枚举无关的一件事：缺 Key 的那一档必须是 Unsupported 而不是 NotFound
        assert_eq!(
            cf_failure(CfError::new(CfErrorKind::NotConfigured, "x")).kind,
            ContentFailureKind::Unsupported
        );
    }
}
