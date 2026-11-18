//! 站点的原始 JSON → 上面那些类型（**唯一**读字段名的地方）。
//!
//! # 两层容错
//!
//! - **形状**（外层信封）：`{"data": ...}` 里的 `data` 必须存在且类型对。缺失时是
//!   [`CfErrorKind::Malformed`]，**不是**空页——「站点说没有」和「我们读错了信封」是两件事，
//!   把后者画成「没有匹配的内容」就是撒谎；
//! - **字段**：每一条 mod / file 里的字段一律 `#[serde(default)]`。站点会加字段、也会整段不给
//!   （没图标、没作者、没摘要），缺字段不该让整页解析失败。
//!
//! 唯一的例外是 `/download-url`：`{"data": null}` 在站点语义里是「这个文件没有可用下载地址」
//! （作者关闭第三方分发），所以它落 [`CfErrorKind::Unsupported`] 而不是 `Malformed`。
//!
//! # `gameVersions` 是混装的
//!
//! 一个文件的 `gameVersions` 长这样：`["1.20.1", "1.20.2", "Forge", "Client", "Server"]`。
//! 直接把整份当「支持的游戏版本」，页面上就会出现「支持的版本：Forge / Client」这种谎话，
//! 「装到 1.20.1 Forge 上」的判断也会失真。所以这里按**首字符是不是数字**分流：
//! 版本号进 `game_versions`，加载器词（`Forge` / `NeoForge` / `Fabric` …）进 `loaders`。
//! 环境标记（`Client` / `Server`）两边都不进。

use std::collections::BTreeMap;

use serde::Deserialize;

use super::query::{CfLoader, CfReleaseType};
use super::types::{push_unique, FileIndex, Mod, ModFile, ModPage};
use crate::addon::curseforge::error::{CfError, CfErrorKind};

/// 站点 `hashes[].algo` 的数字 → 算法名。
///
/// 站点只认这两个（1 = SHA-1、2 = MD5），且 CF 的 `manifest.json` 用 SHA-1 做指纹，
/// 所以摘要表只保留这两个键——收集 `algo` 数字本身对调用方没有意义。
fn hash_algo(code: u64) -> Option<&'static str> {
    match code {
        1 => Some("sha1"),
        2 => Some("md5"),
        _ => None,
    }
}

// --------------------------------------------------------------- 原始形状

#[derive(Debug, Default, Deserialize)]
struct RawSearch {
    data: Option<Vec<RawMod>>,
    pagination: Option<RawPagination>,
}

#[derive(Debug, Default, Deserialize)]
struct RawPagination {
    #[serde(default)]
    index: u32,
    #[serde(default, rename = "pageSize")]
    page_size: u32,
    #[serde(default, rename = "totalCount")]
    total_count: u64,
}

#[derive(Debug, Default, Deserialize)]
struct RawSingle<T> {
    data: Option<T>,
}

#[derive(Debug, Default, Deserialize)]
struct RawDownloadUrl {
    data: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawMod {
    #[serde(default)]
    id: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    authors: Vec<RawAuthor>,
    #[serde(default, rename = "downloadCount")]
    download_count: u64,
    #[serde(default, rename = "dateModified")]
    date_modified: String,
    #[serde(default)]
    categories: Vec<RawCategory>,
    #[serde(default, rename = "latestFilesIndexes")]
    latest_files: Vec<RawFileIndex>,
    #[serde(default)]
    logo: Option<RawLogo>,
    // `classId` 只在 mod 上有（文件端点不给）：导入侧靠它决定落盘目录，见 [`parse_class_id`]。
    #[serde(default, rename = "classId")]
    class_id: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawAuthor {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawCategory {
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawLogo {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawFileIndex {
    #[serde(default, rename = "gameVersion")]
    game_version: Option<String>,
    // `modLoader` 是数字枚举；认不出的取值（站点以后加新加载器）落 `None`，不让整页报错。
    #[serde(default, rename = "modLoader")]
    mod_loader: Option<u32>,
    #[serde(default, rename = "fileId")]
    file_id: u32,
    #[serde(default)]
    filename: String,
    #[serde(default, rename = "releaseType")]
    release_type: Option<u8>,
}

#[derive(Debug, Default, Deserialize)]
struct RawFile {
    #[serde(default)]
    id: u32,
    #[serde(default, rename = "modId")]
    mod_id: u32,
    #[serde(default, rename = "displayName")]
    display_name: String,
    #[serde(default, rename = "fileName")]
    file_name: String,
    #[serde(default, rename = "releaseType")]
    release_type: Option<u8>,
    #[serde(default, rename = "fileDate")]
    file_date: String,
    #[serde(default, rename = "fileLength")]
    file_length: u64,
    #[serde(default, rename = "downloadCount")]
    download_count: u64,
    #[serde(default, rename = "downloadUrl")]
    download_url: Option<String>,
    #[serde(default)]
    hashes: Vec<RawHash>,
    #[serde(default, rename = "gameVersions")]
    game_versions: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawHash {
    #[serde(default)]
    value: String,
    #[serde(default)]
    algo: u64,
}

// --------------------------------------------------------------- 解析入口

/// 解析 `/mods/search`（`data[]` + `pagination`）。
///
/// # Errors
/// JSON 不合法、或外层没有 `data` 数组时返回 [`CfErrorKind::Malformed`]。
pub fn parse_page(body: &str) -> Result<ModPage, CfError> {
    let raw: RawSearch = decode(body, "mods/search")?;
    let data = raw.data.ok_or_else(|| missing("mods/search", "data"))?;
    let pagination = raw.pagination.unwrap_or_default();
    Ok(ModPage {
        hits: data.into_iter().map(mod_of).collect(),
        total_count: pagination.total_count,
        index: pagination.index,
        page_size: pagination.page_size,
    })
}

/// 解析 `/mods/{id}`（单个 mod）。
///
/// # Errors
/// JSON 不合法、或外层没有 `data` 对象时返回 [`CfErrorKind::Malformed`]。
pub fn parse_mod(body: &str) -> Result<Mod, CfError> {
    let raw: RawSingle<RawMod> = decode(body, "mods/{id}")?;
    let data = raw.data.ok_or_else(|| missing("mods/{id}", "data"))?;
    Ok(mod_of(data))
}

/// 解析 `/mods/{id}/files`（文件数组）。
///
/// # Errors
/// JSON 不合法、或外层没有 `data` 数组时返回 [`CfErrorKind::Malformed`]。
pub fn parse_files(body: &str) -> Result<Vec<ModFile>, CfError> {
    let raw: RawSingle<Vec<RawFile>> = decode(body, "mods/{id}/files")?;
    let data = raw.data.ok_or_else(|| missing("mods/{id}/files", "data"))?;
    Ok(data.into_iter().map(file_of).collect())
}

/// 解析 `/mods/{id}/files/{fileId}`（单个文件）与 `/download-url` 之外的同一形状。
///
/// # Errors
/// JSON 不合法、或外层没有 `data` 对象时返回 [`CfErrorKind::Malformed`]。
pub fn parse_file(body: &str) -> Result<ModFile, CfError> {
    let raw: RawSingle<RawFile> = decode(body, "mods/{id}/files/{fileId}")?;
    let data = raw
        .data
        .ok_or_else(|| missing("mods/{id}/files/{fileId}", "data"))?;
    Ok(file_of(data))
}

/// 解析 `/mods/{id}/files/{fileId}/download-url`（`data` 是一个字符串）。
///
/// # Errors
/// JSON 不合法 ⇒ `Malformed`；`data` 缺失 / `null` / 空串 ⇒ [`CfErrorKind::Unsupported`]
/// （站点语义是「这个文件没有可用下载地址」，作者关闭了第三方分发）。
pub fn parse_download_url(body: &str) -> Result<String, CfError> {
    let raw: RawDownloadUrl = decode(body, "download-url")?;
    let url = raw
        .data
        .ok_or_else(|| {
            CfError::unsupported("CurseForge 没有给这个文件下载地址（作者可能关闭了第三方分发）")
        })?
        .trim()
        .to_string();
    if url.is_empty() {
        return Err(CfError::unsupported(
            "CurseForge 返回了空的下载地址（作者可能关闭了第三方分发）",
        ));
    }
    Ok(url)
}

// --------------------------------------------------------------- 逐条转换

fn mod_of(raw: RawMod) -> Mod {
    Mod {
        id: raw.id,
        name: raw.name,
        slug: raw.slug,
        summary: raw.summary,
        authors: raw
            .authors
            .into_iter()
            .map(|author| author.name)
            .filter(|name| !name.trim().is_empty())
            .collect(),
        download_count: raw.download_count,
        date_modified: raw.date_modified,
        categories: raw
            .categories
            .into_iter()
            .map(|category| {
                if category.slug.trim().is_empty() {
                    category.name
                } else {
                    category.slug
                }
            })
            .filter(|tag| !tag.trim().is_empty())
            .collect(),
        latest_files: raw.latest_files.into_iter().map(index_of).collect(),
        icon_url: raw
            .logo
            .and_then(|logo| logo.url)
            .filter(|url| !url.trim().is_empty()),
    }
}

/// 解析 `/mods/{id}` 响应里的项目类别（`classId`：6 Mods / 4471 Modpacks / 12 ResourcePacks /
/// 6552 Shaders）。
///
/// # 为什么单开一个入口，而不是给 [`Mod`] 加字段
///
/// `classId` 只有**一个**消费者：导入侧拿它决定落盘目录（`import::plan::curse_class_dir`，
/// 认不出就不猜）。给 [`Mod`] 加字段是改所有检索/详情消费者共用的形状——每个 `Mod { .. }`
/// 字面量都要跟着改，代价与收益不成比例。所以本函数只读这一个字段，其余字段照旧由
/// [`parse_mod`] 给出。
///
/// # Errors
/// JSON 不合法、或外层没有 `data` 对象时返回 [`CfErrorKind::Malformed`]；
/// 站点没给 `classId`（或给了 0）⇒ `Ok(None)`（**不猜**类别）。
pub fn parse_class_id(body: &str) -> Result<Option<u32>, CfError> {
    let raw: RawSingle<RawMod> = decode(body, "mods/{id}")?;
    let data = raw.data.ok_or_else(|| missing("mods/{id}", "data"))?;
    Ok(data.class_id.filter(|class| *class > 0))
}

fn index_of(raw: RawFileIndex) -> FileIndex {
    FileIndex {
        game_version: raw
            .game_version
            .filter(|version| !version.trim().is_empty()),
        loader: raw.mod_loader.and_then(CfLoader::parse),
        file_id: raw.file_id,
        filename: raw.filename,
        release_type: raw
            .release_type
            .map_or(CfReleaseType::Release, CfReleaseType::parse),
    }
}

fn file_of(raw: RawFile) -> ModFile {
    let hashes = raw
        .hashes
        .into_iter()
        .filter_map(|hash| {
            let algo = hash_algo(hash.algo)?;
            let value = hash.value.trim().to_ascii_lowercase();
            if value.is_empty() {
                return None;
            }
            Some((algo.to_string(), value))
        })
        .collect::<BTreeMap<String, String>>();
    let (game_versions, loaders) = split_game_versions(raw.game_versions);
    ModFile {
        id: raw.id,
        mod_id: raw.mod_id,
        display_name: raw.display_name,
        file_name: raw.file_name,
        release_type: raw
            .release_type
            .map_or(CfReleaseType::Release, CfReleaseType::parse),
        file_date: raw.file_date,
        file_length: raw.file_length,
        download_count: raw.download_count,
        // 空串与 `null` 同义：站点两种都表示「没给」
        download_url: raw.download_url.filter(|url| !url.trim().is_empty()),
        hashes,
        game_versions,
        loaders,
    }
}

/// `gameVersions` 分流：版本号 / 加载器词 / 两边都不进（`Client`、`Server`）。
fn split_game_versions(raw: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut versions: Vec<String> = Vec::new();
    let mut loaders: Vec<String> = Vec::new();
    for entry in raw {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        if let Some(facet) = CfLoader::from_word(entry).and_then(CfLoader::facet) {
            push_unique(&mut loaders, facet);
            continue;
        }
        // 版本号：首字符是数字。`Client` / `Server` 这类环境标记两边都不进。
        if entry.starts_with(|ch: char| ch.is_ascii_digit()) {
            push_unique(&mut versions, entry);
        }
    }
    (versions, loaders)
}

// --------------------------------------------------------------- 信封

fn decode<T: for<'de> Deserialize<'de>>(body: &str, what: &str) -> Result<T, CfError> {
    serde_json::from_str(body).map_err(|error| {
        CfError::new(
            CfErrorKind::Malformed,
            format!("CurseForge {what} 响应不是期望形状：{error}"),
        )
    })
}

fn missing(what: &str, field: &str) -> CfError {
    CfError::new(
        CfErrorKind::Malformed,
        format!("CurseForge {what} 响应里没有 {field}（站点方言变了？）"),
    )
}
