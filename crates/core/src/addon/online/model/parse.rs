//! 站点的原始 JSON → 上面那些类型（**唯一**读字段名的地方）。
//!
//! 每个 `#[serde(default)]` 都在说「这个字段站点可能不给」；`alias` 只在有历史原因时用
//! （例如检索命中的日期字段曾按 `updated` 读，站点实际叫 `date_modified`）。

use std::collections::BTreeMap;

use serde::Deserialize;

use super::query::KNOWN_LOADERS;
use super::types::{ProjectDetail, ProjectHit, SearchPage, Version, VersionFile, VersionType};
use crate::addon::online::transport::{OnlineError, OnlineErrorKind};

// --------------------------------------------------------------- 解析

#[derive(Debug, Default, Deserialize)]
struct RawSearch {
    #[serde(default)]
    hits: Vec<RawHit>,
    #[serde(default)]
    total_hits: u64,
    #[serde(default)]
    offset: u32,
    #[serde(default)]
    limit: u32,
}

#[derive(Debug, Default, Deserialize)]
struct RawHit {
    #[serde(default)]
    project_id: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    follows: u64,
    // 站点的字段名是 `date_modified`（旧实现的 `updated` 永远取不到值，别再加回去当主名）。
    #[serde(default, alias = "updated")]
    date_modified: String,
    #[serde(default)]
    versions: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    project_type: String,
    #[serde(default)]
    icon_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawProject {
    #[serde(default)]
    id: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    project_type: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    followers: u64,
    #[serde(default)]
    updated: String,
    #[serde(default)]
    published: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    issues_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawVersion {
    #[serde(default)]
    id: String,
    #[serde(default)]
    project_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version_number: String,
    #[serde(default)]
    version_type: String,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    date_published: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    files: Vec<RawFile>,
}

#[derive(Debug, Default, Deserialize)]
struct RawFile {
    #[serde(default)]
    hashes: BTreeMap<String, String>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    filename: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    primary: bool,
}

/// 解析 `/v2/search`。
///
/// # Errors
/// JSON 不合法、或没有 `hits` 数组时返回 [`OnlineErrorKind::Malformed`]。
pub fn parse_search(body: &str, offset: u32, limit: u32) -> Result<SearchPage, OnlineError> {
    let raw: RawSearch = parse_json(body, "search")?;
    Ok(SearchPage {
        hits: raw.hits.into_iter().map(hit_of).collect(),
        total_hits: raw.total_hits,
        // 站点会回显 offset/limit；缺省用请求里的（分页游标不能靠回显才成立）
        offset: raw.offset.max(offset),
        limit: if raw.limit == 0 { limit } else { raw.limit },
    })
}

/// 解析 `/v2/project/{id}`。
///
/// # Errors
/// JSON 不合法时返回 [`OnlineErrorKind::Malformed`]。
pub fn parse_project(body: &str) -> Result<ProjectDetail, OnlineError> {
    let raw: RawProject = parse_json(body, "project")?;
    Ok(ProjectDetail {
        id: raw.id,
        slug: raw.slug,
        title: raw.title,
        description: raw.description,
        body: raw.body,
        project_type: raw.project_type,
        downloads: raw.downloads,
        follows: raw.followers,
        updated: raw.updated,
        published: raw.published,
        game_versions: raw.game_versions,
        loaders: raw.loaders,
        categories: raw.categories,
        icon_url: non_empty(raw.icon_url),
        source_url: non_empty(raw.source_url),
        issues_url: non_empty(raw.issues_url),
    })
}

/// 解析 `/v2/project/{id}/version`（一个数组）。
///
/// # Errors
/// JSON 不合法或不是数组时返回 [`OnlineErrorKind::Malformed`]。
pub fn parse_versions(body: &str) -> Result<Vec<Version>, OnlineError> {
    let raw: Vec<RawVersion> =
        serde_json::from_str(body).map_err(|error| malformed("versions", error))?;
    Ok(raw.into_iter().map(version_of).collect())
}

/// 解析 `/v2/version_file/{hash}`（单个版本对象）。
///
/// # Errors
/// JSON 不合法时返回 [`OnlineErrorKind::Malformed`]。
pub fn parse_version(body: &str) -> Result<Version, OnlineError> {
    let raw: RawVersion = parse_json(body, "version_file")?;
    Ok(version_of(raw))
}

fn parse_json<T: for<'de> Deserialize<'de>>(body: &str, what: &str) -> Result<T, OnlineError> {
    serde_json::from_str(body).map_err(|error| malformed(what, error))
}

fn malformed(what: &str, error: serde_json::Error) -> OnlineError {
    OnlineError::new(
        OnlineErrorKind::Malformed,
        format!("Modrinth {what} 响应不是期望形状：{error}"),
    )
}

fn hit_of(raw: RawHit) -> ProjectHit {
    ProjectHit {
        project_id: raw.project_id,
        slug: raw.slug,
        title: raw.title,
        description: raw.description,
        author: raw.author,
        downloads: raw.downloads,
        follows: raw.follows,
        updated: raw.date_modified,
        loaders: known_loaders(&raw.categories),
        game_versions: raw.versions,
        project_type: raw.project_type,
        icon_url: non_empty(raw.icon_url),
    }
}

fn version_of(raw: RawVersion) -> Version {
    Version {
        id: raw.id,
        project_id: raw.project_id,
        name: raw.name,
        version_number: raw.version_number,
        version_type: VersionType::parse(&raw.version_type),
        loaders: raw.loaders,
        game_versions: raw.game_versions,
        date_published: raw.date_published,
        downloads: raw.downloads,
        files: raw
            .files
            .into_iter()
            .map(|file| VersionFile {
                hashes: file.hashes,
                url: file.url,
                filename: file.filename,
                size: file.size,
                primary: file.primary,
            })
            .collect(),
    }
}

/// 只留已知加载器（`categories` 里混着分类标签）。
#[must_use]
pub fn known_loaders(categories: &[String]) -> Vec<String> {
    categories
        .iter()
        .filter(|tag| KNOWN_LOADERS.contains(&tag.as_str()))
        .cloned()
        .collect()
}

/// 空串当「没有」（站点用 `""` 表示没图 / 没链接）。
fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|item| !item.trim().is_empty())
}
