//! 两个站点的检索方言（M109）：把各自的 JSON 翻成 [`ContentHit`]。
//!
//! 两个站点在**同一件事上用的字段名完全不同**，所以这里每个站点一段、各自带注释：
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
//! 直接整份当加载器用，画面上就会多出「optimization」这种假加载器（`sources/tests` 钉住了这一条）。

use serde_json::Value;

use crate::dto::{
    loader_facet, ContentHit, ContentKind, ContentQuery, ContentResults, ContentSource,
};

use super::super::env::HostEnv;
use super::curseforge_ready;
use super::http::{encode, get};

/// Modrinth v2 的根（无需 key）。
pub(super) const MODRINTH: &str = "https://api.modrinth.com/v2";
/// CurseForge v1 的根（需要 `X-API-KEY`）。
pub(super) const CURSEFORGE: &str = "https://api.curseforge.com/v1";
/// CurseForge 的「Minecraft」gameId（官方固定值）。
const CF_MINECRAFT: u32 = 432;

/// 检索分派。
pub(super) fn search(env: &HostEnv, query: &ContentQuery) -> Result<ContentResults, String> {
    match query.source {
        ContentSource::Modrinth => modrinth(env, query),
        ContentSource::Curseforge => curseforge(env, query),
    }
}

// --------------------------------------------------------------- Modrinth

fn modrinth(env: &HostEnv, query: &ContentQuery) -> Result<ContentResults, String> {
    let mut facets = vec![format!("[\"project_type:{}\"]", query.kind.modrinth_type())];
    if let Some(version) = &query.game_version {
        facets.push(format!("[\"versions:{version}\"]"));
    }
    if let Some(facet) = query.loader.and_then(loader_facet) {
        facets.push(format!("[\"categories:{facet}\"]"));
    }

    let mut url = format!(
        "{MODRINTH}/search?limit={}&offset={}&index={}&facets={}",
        query.limit.min(100),
        query.offset,
        query.sort.modrinth_index(),
        encode(&format!("[{}]", facets.join(",")))
    );
    if !query.text.trim().is_empty() {
        url.push_str(&format!("&query={}", encode(query.text.trim())));
    }

    let body = get(env, &url, &[])?;
    let json: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Modrinth 返回的不是合法 JSON：{error}"))?;
    let hits = json
        .get("hits")
        .and_then(Value::as_array)
        .ok_or_else(|| "Modrinth 返回里没有 hits 字段".to_string())?
        .iter()
        .map(modrinth_hit)
        .collect::<Vec<_>>();

    Ok(ContentResults {
        query: query.clone(),
        hits,
        total: u32_of(&json, "total_hits"),
        curseforge: curseforge_ready(),
    })
}

/// 一条 Modrinth 命中项 → DTO（缺字段一律给空/零，**不**编造）。
fn modrinth_hit(value: &Value) -> ContentHit {
    let loaders = strings(value, "categories")
        .into_iter()
        // `categories` 混着加载器与分类标签：只留已知的四个加载器
        .filter(|tag| matches!(tag.as_str(), "fabric" | "forge" | "neoforge" | "quilt"))
        .collect();

    ContentHit {
        id: text(value, "project_id"),
        slug: text(value, "slug"),
        title: text(value, "title"),
        summary: text(value, "description"),
        author: text(value, "author"),
        downloads: u64_of(value, "downloads"),
        updated: date_part(&text(value, "updated")),
        source: ContentSource::Modrinth,
        game_versions: strings(value, "versions"),
        loaders,
        icon: optional_text(value, "icon_url"),
    }
}

// --------------------------------------------------------------- CurseForge

fn curseforge(env: &HostEnv, query: &ContentQuery) -> Result<ContentResults, String> {
    let Some(key) = super::curseforge_key() else {
        return Err("CurseForge 需要 API key（ULL_CF_API_KEY 未设置）".to_string());
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

    let body = get(env, &url, &[("x-api-key", key.as_str())])?;
    let json: Value = serde_json::from_str(&body)
        .map_err(|error| format!("CurseForge 返回的不是合法 JSON：{error}"))?;
    let hits = json
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "CurseForge 返回里没有 data 字段".to_string())?
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

/// 取可选字符串字段（空串也当没有——站点会给 `""` 表示没图）。
fn optional_text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

/// 取字符串数组（缺 / 类型不对 ⇒ 空 vec）。
fn strings(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn u64_of(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn u32_of(value: &Value, key: &str) -> u32 {
    u32::try_from(u64_of(value, key)).unwrap_or(0)
}

/// ISO-8601 → 日期部分（`2024-05-01T…` → `2024-05-01`；短串原样返回）。
pub(super) fn date_part(raw: &str) -> String {
    raw.get(..10).unwrap_or(raw).to_string()
}

/// 类型名只用来钉住「四条侧栏都在」的常量没被改乱（检索本身按 `kind` 走 facet）。
#[must_use]
pub(super) fn all_kinds() -> [ContentKind; 4] {
    ContentKind::ALL
}
