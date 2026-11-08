//! 两个站点的检索方言（M109；M162 起 Modrinth 走 `core::addon::online`）+ **详情/版本列表**。
//!
//! # M162 之后的分工
//!
//! | 站点 | 检索 | 详情 / 版本列表 | 端点拼装与 JSON 解析在哪 |
//! |------|------|------------------|--------------------------|
//! | Modrinth | `core::addon::online` | `core::addon::online` | core（`online::modrinth` / `online::model`） |
//! | CurseForge | 本文件（要 `X-API-KEY`） | **未接通**（明确报错） | 本文件 |
//!
//! 为什么 Modrinth 搬走：「在线内容源」是 core 的能力（导出的 `RemoteCatalog`、导入的
//! `PackFetcher`、UI 的检索要共用同一份解析与同一套错误分类）。presenter 只留 DTO 映射与
//! 失败分类——它不再自己拼 URL、不再自己读 JSON 字段名。
//!
//! # 两个站点的字段名完全不同（CF 这一半仍是本文件的活）
//!
//! | 语义 | Modrinth `/v2/search` | CurseForge `/v1/mods/search` |
//! |---|---|---|
//! | 项目 id | `project_id`（base62） | `id`（数字） |
//! | 标题 / 简介 | `title` / `description` | `name` / `summary` |
//! | 下载量 | `downloads` | `downloadCount` |
//! | 游戏版本 | `versions` | `latestFilesIndexes[].gameVersion` |
//! | 加载器 | `categories` 里混着的 `fabric`/`forge`/… | `latestFilesIndexes[].modLoader`（数字枚举） |
//!
//! 加载器那一行是最容易抄错的地方：Modrinth 的 `categories` 同时装着 `optimization` 这种**分类标签**，
//! 直接整份当加载器用，画面上就会多出「optimization」这种假加载器（core 那一侧的
//! `known_loaders` 已经过滤，`sources/tests` 钉住了这一条）。

use serde_json::Value;

use unlucky_core::addon::online::{
    ContentType, Modrinth, OnlineError, OnlineErrorKind, SearchQuery, SortIndex, VersionQuery,
};

use crate::dto::{
    loader_facet, ContentDetail, ContentDetailRequest, ContentFailure, ContentFailureKind,
    ContentHit, ContentQuery, ContentResults, ContentSource, ContentVersionView,
};

use super::super::engine;
use super::super::env::HostEnv;
use super::curseforge_ready;
use super::http::{encode, get};
use unlucky_core::addon::online::EngineTransport;
use unlucky_core::download::blocking::block_on;

// Modrinth 的根不再在本文件：URL 拼装与解析都在 `core::addon::online`，这里只留 CF 的
// （它要 `X-API-KEY`，是 core 那一侧**故意**没接的那半）。
/// CurseForge v1 的根（需要 `X-API-KEY`）。
pub(super) const CURSEFORGE: &str = "https://api.curseforge.com/v1";
/// CurseForge 的「Minecraft」gameId（官方固定值）。
const CF_MINECRAFT: u32 = 432;

/// 检索分派。
pub(super) fn search(
    env: &HostEnv,
    query: &ContentQuery,
) -> Result<ContentResults, ContentFailure> {
    match query.source {
        ContentSource::Modrinth => modrinth(env, query),
        ContentSource::Curseforge => curseforge(env, query),
    }
}

/// 详情 + 版本列表分派（M162）。
pub(super) fn detail(
    env: &HostEnv,
    request: &ContentDetailRequest,
) -> Result<ContentDetail, ContentFailure> {
    match request.source {
        ContentSource::Modrinth => modrinth_detail(env, request),
        ContentSource::Curseforge => Err(ContentFailure::unsupported(
            "CurseForge 的详情/版本列表未接通（要 X-API-KEY 的文件端点）；先切到 Modrinth 源",
        )),
    }
}

// --------------------------------------------------------------- Modrinth

fn modrinth(env: &HostEnv, query: &ContentQuery) -> Result<ContentResults, ContentFailure> {
    let core_engine = engine::engine(env);
    let transport = EngineTransport::new(&core_engine);
    let client = Modrinth::new(&transport);
    let page = block_on(client.search(&search_query(query))).map_err(failure_of)?;
    Ok(ContentResults {
        query: query.clone(),
        hits: page.hits.iter().map(hit_of).collect(),
        total: u32::try_from(page.total_hits).unwrap_or(0),
        curseforge: curseforge_ready(),
    })
}

fn modrinth_detail(
    env: &HostEnv,
    request: &ContentDetailRequest,
) -> Result<ContentDetail, ContentFailure> {
    let core_engine = engine::engine(env);
    let transport = EngineTransport::new(&core_engine);
    let client = Modrinth::new(&transport);
    let project = block_on(client.project(&request.project)).map_err(failure_of)?;
    let versions = block_on(client.versions(&VersionQuery {
        project: request.project.clone(),
        game_version: request.game_version.clone(),
        loader: request.loader.and_then(loader_facet).map(str::to_string),
    }))
    .map_err(failure_of)?;
    Ok(ContentDetail {
        project: if project.id.is_empty() {
            request.project.clone()
        } else {
            project.id
        },
        title: project.title,
        description: project.description,
        body: project.body,
        versions: versions.iter().map(version_of).collect(),
    })
}

/// DTO 检索条件 → core 的检索请求（**唯一**一处字段搬运）。
fn search_query(query: &ContentQuery) -> SearchQuery {
    SearchQuery {
        text: query.text.clone(),
        project_type: ContentType::parse(query.kind.modrinth_type()),
        game_version: query.game_version.clone(),
        loader: query.loader.and_then(loader_facet).map(str::to_string),
        index: match query.sort {
            crate::dto::ContentSort::Relevance => SortIndex::Relevance,
            crate::dto::ContentSort::Downloads => SortIndex::Downloads,
            crate::dto::ContentSort::Newest => SortIndex::Newest,
            crate::dto::ContentSort::Updated => SortIndex::Updated,
        },
        limit: query.limit,
        offset: query.offset,
    }
}

/// core 的命中项 → DTO（缺字段一律给空/零，**不**编造）。
fn hit_of(hit: &unlucky_core::addon::online::ProjectHit) -> ContentHit {
    ContentHit {
        id: hit.project_id.clone(),
        slug: hit.slug.clone(),
        title: hit.title.clone(),
        summary: hit.description.clone(),
        author: hit.author.clone(),
        downloads: hit.downloads,
        updated: date_part(&hit.updated),
        source: ContentSource::Modrinth,
        game_versions: hit.game_versions.clone(),
        loaders: hit.loaders.clone(),
        icon: hit.icon_url.clone(),
    }
}

/// core 的版本 → DTO 的版本行（文件名/大小取 `primary` 那个）。
fn version_of(version: &unlucky_core::addon::online::Version) -> ContentVersionView {
    let file = version.primary_file();
    ContentVersionView {
        id: version.id.clone(),
        name: version.name.clone(),
        number: version.version_number.clone(),
        kind: version.version_type.as_str().to_string(),
        game_versions: version.game_versions.clone(),
        loaders: version.loaders.clone(),
        published: date_part(&version.date_published),
        file_name: file
            .map(|file| file.filename.clone())
            .filter(|name| !name.trim().is_empty()),
        file_size: file.map(|file| file.size).filter(|size| *size > 0),
    }
}

/// core 的错误 → DTO 的失败大类（**一一映射，不重新判**）。
pub(super) fn failure_of(error: OnlineError) -> ContentFailure {
    let kind = match error.kind() {
        OnlineErrorKind::NotConnected => ContentFailureKind::Offline,
        OnlineErrorKind::NotFound => ContentFailureKind::NotFound,
        OnlineErrorKind::Server => ContentFailureKind::Server,
        OnlineErrorKind::Unsupported => ContentFailureKind::Unsupported,
        OnlineErrorKind::Malformed | OnlineErrorKind::Io | OnlineErrorKind::Integrity => {
            ContentFailureKind::Other
        }
    };
    ContentFailure::new(kind, error.message().to_string())
}

// --------------------------------------------------------------- CurseForge

fn curseforge(env: &HostEnv, query: &ContentQuery) -> Result<ContentResults, ContentFailure> {
    let Some(key) = super::curseforge_key() else {
        return Err(ContentFailure::unsupported(
            "CurseForge 需要 API key（ULL_CF_API_KEY 未设置）",
        ));
    };
    let mut url = format!(
        "{CURSEFORGE}/mods/search?gameId={CF_MINECRAFT}&classId={}&index={}&pageSize={}&sortField={}&sortOrder=desc",
        query.kind.curseforge_class(),
        query.offset,
        query.limit.min(50),
        query.sort.curseforge_field()
    );
    if !query.text.trim().is_empty() {
        url.push_str(&format!("&searchFilter={}", encode(query.text.trim())));
    }
    if let Some(version) = &query.game_version {
        url.push_str(&format!("&gameVersion={}", encode(version)));
    }

    let body = get(env, &url, &[("x-api-key", key.as_str())]).map_err(cf_failure)?;
    let json: Value = serde_json::from_str(&body).map_err(|error| {
        ContentFailure::new(
            ContentFailureKind::Other,
            format!("CurseForge 返回的不是合法 JSON：{error}"),
        )
    })?;
    let hits = json
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ContentFailure::new(ContentFailureKind::Other, "CurseForge 返回里没有 data 字段")
        })?
        .iter()
        .map(curseforge_hit)
        .collect::<Vec<_>>();

    Ok(ContentResults {
        query: query.clone(),
        hits,
        total: json
            .get("pagination")
            .map(|pagination| u64_of(pagination, "totalCount"))
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0),
        curseforge: true,
    })
}

/// CurseForge 的字符串错误 → 失败大类：只有「缺密钥」是 Unsupported，其余按站点/其它。
fn cf_failure(reason: String) -> ContentFailure {
    let kind = if reason.contains("ULL_CF_API_KEY") {
        ContentFailureKind::Unsupported
    } else if reason.contains("HTTP 503")
        || reason.contains("HTTP 502")
        || reason.contains("HTTP 500")
    {
        ContentFailureKind::Server
    } else {
        ContentFailureKind::Other
    };
    ContentFailure::new(kind, reason)
}

/// 一条 CF 命中项 → DTO。加载器只能从 `latestFilesIndexes[].modLoader` 的数字枚举反推。
fn curseforge_hit(value: &Value) -> ContentHit {
    let mut game_versions: Vec<String> = Vec::new();
    let mut loaders: Vec<String> = Vec::new();
    if let Some(indexes) = value.get("latestFilesIndexes").and_then(Value::as_array) {
        for entry in indexes {
            if let Some(version) = entry.get("gameVersion").and_then(Value::as_str) {
                if !game_versions.iter().any(|item| item == version) {
                    game_versions.push(version.to_string());
                }
            }
            if let Some(loader) = entry
                .get("modLoader")
                .and_then(Value::as_u64)
                .and_then(curseforge_loader)
            {
                if !loaders.iter().any(|item| item == loader) {
                    loaders.push(loader.to_string());
                }
            }
        }
    }
    let author = value
        .get("authors")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .map(|entry| text(entry, "name"))
        .unwrap_or_default();

    ContentHit {
        id: value
            .get("id")
            .and_then(Value::as_u64)
            .map(|id| id.to_string())
            .unwrap_or_default(),
        slug: text(value, "slug"),
        title: text(value, "name"),
        summary: text(value, "summary"),
        author,
        downloads: u64_of(value, "downloadCount"),
        updated: date_part(&text(value, "dateModified")),
        source: ContentSource::Curseforge,
        game_versions,
        loaders,
        icon: value
            .get("logo")
            .and_then(|logo| logo.get("url"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
    }
}

/// CF 的 `modLoader` 枚举 → Modrinth 口径的 loader 名（1 Forge / 4 Fabric / 5 Quilt / 6 NeoForge）。
pub(super) fn curseforge_loader(raw: u64) -> Option<&'static str> {
    match raw {
        1 => Some("forge"),
        4 => Some("fabric"),
        5 => Some("quilt"),
        6 => Some("neoforge"),
        _ => None,
    }
}

// --------------------------------------------------------------- 小工具

/// 取字符串字段（缺 / 类型不对 ⇒ 空串）。
fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn u64_of(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// 四条侧栏的常量表没被改乱（检索本身按 `kind` 走 facet）。
#[must_use]
pub(super) fn all_kinds() -> [crate::dto::ContentKind; 4] {
    crate::dto::ContentKind::ALL
}

/// ISO-8601 → 日期部分（`2024-05-01T…` → `2024-05-01`；短串原样返回）。
pub(super) fn date_part(raw: &str) -> String {
    raw.get(..10).unwrap_or(raw).to_string()
}
