//! 两个站点的检索方言（M109；M162 起 Modrinth 走 `core::addon::online`，M317 起 CurseForge
//! 走 `core::addon::curseforge`）+ **详情/版本列表**。
//!
//! # M317 之后的分工
//!
//! | 站点 | 检索 | 详情 / 版本列表 | 端点拼装、请求头与 JSON 解析在哪 |
//! |------|------|------------------|--------------------------|
//! | Modrinth | `core::addon::online` | `core::addon::online` | core（`online::modrinth` / `online::model`） |
//! | CurseForge | `core::addon::curseforge`（M317 接线） | core 侧已落地；presenter 的 `detail` 仍是明确的 `Unsupported`（未接线） | core（`curseforge::model`）；`x-api-key` 只在 `CurseForge::get` 里拼 |
//!
//! 为什么两条都搬走：「在线内容源」是 core 的能力（导出的 `RemoteCatalog`、导入的
//! `PackFetcher`、UI 的检索要共用同一份解析与同一套错误分类）。presenter 只留 DTO 映射与
//! 失败分类——它不再自己拼 URL、不再自己读 JSON 字段名、**也不再见得到 Key 的形状**
//! （Key 变成 `addon::curseforge::ApiKey`，本文件没有任何位置能装下它的明文）。
//!
//! # 两个站点的字段名完全不同（方言差异在 core 那一半）
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
//! `known_loaders` 已经过滤，`sources/tests` 钉住了这一条）。CF 那一半的数字枚举
//! （1 Forge / 4 Fabric / 5 Quilt / 6 NeoForge）现在只在 core 的 `CfLoader` 里出现一次。

use unlucky_core::addon::curseforge::auth::{resolve_key, API_KEY_ENV};
use unlucky_core::addon::curseforge::{ApiKey, CfClass, CfKeyState, CfSort, CurseForge};

use unlucky_core::addon::online::{
    ContentType, Modrinth, OnlineError, OnlineErrorKind, SearchQuery, SortIndex, VersionQuery,
};

use crate::dto::{
    loader_facet, ContentDetail, ContentDetailRequest, ContentFailure, ContentFailureKind,
    ContentHit, ContentQuery, ContentResults, ContentSource, ContentVersionView,
};

use super::super::engine;
use super::super::env::HostEnv;
use crate::dto::{cf_failure, cf_key_available, cf_page_total, hit_of_cf};
use unlucky_core::addon::curseforge::{CfTransport, ModQuery};
use unlucky_core::addon::online::EngineTransport;
use unlucky_core::download::blocking::block_on;

// Modrinth 的根不再在本文件：URL 拼装与解析都在 `core::addon::online`。CurseForge 也一样
// （M317 接线 + 退役）：根、请求头与 JSON 方言都在 `core::addon::curseforge`。

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
        curseforge: cf_available(env),
    })
}

fn modrinth_detail(
    env: &HostEnv,
    request: &ContentDetailRequest,
) -> Result<ContentDetail, ContentFailure> {
    let core_engine = engine::engine(env);
    let transport = EngineTransport::new(&core_engine);
    let client = Modrinth::new(&transport);
    let query = VersionQuery {
        project: request.project.clone(),
        game_version: request.game_version.clone(),
        loader: request.loader.and_then(loader_facet).map(str::to_string),
    };
    // 项目详情 + 版本列表是**两次串行 GET**，包在**一次** `block_on` 里（宿主侧少一层阻塞边界，
    // 也就少一次运行时进出——M165 登记的「单请求两次 block_on」在这里收掉）。
    //
    // 这不是嵌套 `block_on`：两段 `await` 依次跑在同一个 future 里，落在同一个常驻运行时
    // （`core::download::blocking`）与同一个共享引擎上，连接池/限流照常复用。
    // 失败语义不变：哪一步先失败就先返回，与拆成两次调用时的先后顺序一致，分类仍走 `failure_of`。
    let (project, versions) = block_on(async {
        let project = client.project(&request.project).await?;
        let versions = client.versions(&query).await?;
        Ok::<_, OnlineError>((project, versions))
    })
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

/// 设置里那个键（键名常量只在 core 的 schema 登记处写一次）。
const CF_KEY: &str = unlucky_core::setting::keys::online::CURSEFORGE_API_KEY;

/// CurseForge 的 Key：**设置**（`online.curseforge.api_key`）优先，环境变量 `ULL_CF_API_KEY` 兜底。
///
/// **判据只在 core**（[`resolve_key`]）：本函数只做「从自己的 store 取裸值 + 读一次环境变量」，
/// 不自己比较先后、也不自己滤空。GUI 的另外几条路（`host/addon.rs` 的 `curseforge_ready` /
/// `log_curseforge_state`、导入侧的 `pack::import::fetcher_for`）与 CLI（`commands::pack`）
/// 都收敛到同一个函数，于是「设置里填了 Key，但启动日志说没配」这种两种结论不可能再出现。
///
/// Key 的值**不进日志、不进错误串、不进 DTO**：这里只把它变成一个
/// [`ApiKey`]（没有公开取值方法、`Debug` 恒打 `<redacted>`），明文到此为止。
pub(super) fn api_key(env: &HostEnv) -> Option<ApiKey> {
    let store = env.store();
    let doc = store.doc();
    resolve_key(
        doc.effective_str(CF_KEY),
        std::env::var(API_KEY_ENV).ok().as_deref(),
    )
}

/// 「CF 源此刻可用吗」（= 有没有配 Key，与检索、导入用的是**同一个**判据 [`api_key`]）。
///
/// 这个 `bool` 直接驱动 UI 的过滤条；它以前自己算一套（只看环境变量），于是「在设置里填了 Key」
/// 的用户会发现 CF 那一栏永远是灰的。现在调用方按顺序是同一个函数，三处说法不可能分叉。
pub(super) fn cf_available(env: &HostEnv) -> bool {
    let state = if api_key(env).is_some() {
        CfKeyState::Configured
    } else {
        CfKeyState::Missing
    };
    cf_key_available(state)
}

fn curseforge(env: &HostEnv, query: &ContentQuery) -> Result<ContentResults, ContentFailure> {
    let core_engine = engine::engine(env);
    // 传输层是引擎那个 `reqwest::Client`（同一条连接池 / 代理 / UA），本文件**不新发请求**。
    let transport = unlucky_core::addon::curseforge::EngineTransport::new(&core_engine);
    let key = api_key(env);
    let (hits, total) = curseforge_page(&transport, key.as_ref(), query)?;
    Ok(ContentResults {
        query: query.clone(),
        hits,
        total,
        // 可用性取自**同一个** Key 判据（`cf_available`），不另算一套。
        curseforge: cf_available(env),
    })
}

/// 检索一页并翻成 DTO（命中项 + 站点给的总数）。
///
/// 传输层是**参数**：用例喂一个离线 `CfTransport`，一个字节都不出网。DTO 那一半（命中项逐字段、
/// 总数饱和、失败一一映射）全在 [`crate::dto::content::curseforge`]——本文件只搬调用点。
pub(super) fn curseforge_page<T: CfTransport + ?Sized>(
    transport: &T,
    key: Option<&ApiKey>,
    query: &ContentQuery,
) -> Result<(Vec<ContentHit>, u32), ContentFailure> {
    let client = match key {
        Some(key) => CurseForge::with_key(transport, key),
        None => CurseForge::new(transport),
    };
    let page = block_on(client.search(&cf_query(query))).map_err(cf_failure)?;
    Ok((
        page.hits.iter().map(hit_of_cf).collect(),
        cf_page_total(&page),
    ))
}

/// DTO 检索条件 → CF 客户端的请求（**唯一**一处字段搬运）。
///
/// 加载器过滤刻意留空（`loader: None`）：退役前那条路径只发 `gameVersion`，本刀只换传输与
/// 解析，**不外扩检索行为**——多加一个过滤会把「旧实现搜得到的」变成搜不到。
fn cf_query(query: &ContentQuery) -> ModQuery {
    ModQuery {
        text: query.text.clone(),
        class: CfClass::parse(query.kind.curseforge_class()).unwrap_or(CfClass::Mods),
        game_version: query.game_version.clone(),
        loader: None,
        sort: match query.sort {
            crate::dto::ContentSort::Relevance | crate::dto::ContentSort::Downloads => {
                CfSort::Featured
            }
            crate::dto::ContentSort::Newest => CfSort::LastUpdated,
            crate::dto::ContentSort::Updated => CfSort::Popularity,
        },
        index: query.offset,
        page_size: query
            .limit
            .min(unlucky_core::addon::curseforge::MAX_PAGE_SIZE),
    }
}

/// 四条侧栏的常量表没被改乱（检索本身按 `kind` 走 facet）。
#[must_use]
pub(super) fn all_kinds() -> [crate::dto::ContentKind; 4] {
    crate::dto::ContentKind::ALL
}

/// ISO-8601 → 日期部分（措辞与实现都在 `dto::content::date_part`：两条源共用一份）。
pub(super) use crate::dto::date_part;
