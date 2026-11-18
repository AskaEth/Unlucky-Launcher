//! 官方版本表（`dl.liteloader.com/versions/versions.json`）的解析：原版版本 → 条目列表。
//!
//! 上游形状的四条硬事实写在父模块的文档里（[`super`]）；这里只放解析与 URL 拼装。
//! 单独一个文件的理由是行数：父模块要装「解析」与「合成清单」两件事，一起写会破 400 行硬线。

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::{LiteLoaderEntry, LoaderError, CENTRAL_REPO, DEFAULT_REPO, SNAPSHOT_REPO};
use crate::game::library::Library;

#[derive(Debug, Deserialize)]
struct Root {
    #[serde(default)]
    versions: BTreeMap<String, GameVersions>,
}

#[derive(Debug, Deserialize)]
struct GameVersions {
    #[serde(default)]
    repo: Option<Repo>,
    #[serde(default)]
    artefacts: Option<Branch>,
    #[serde(default)]
    snapshots: Option<Branch>,
}

#[derive(Debug, Deserialize)]
struct Repo {
    #[serde(default)]
    url: String,
}

#[derive(Debug, Deserialize)]
struct Branch {
    #[serde(rename = "com.mumfrey:liteloader", default)]
    liteloader: BTreeMap<String, Record>,
}

#[derive(Debug, Deserialize)]
struct Record {
    #[serde(default, rename = "tweakClass")]
    tweak_class: String,
    #[serde(default)]
    libraries: Vec<Library>,
    #[serde(default)]
    file: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    stream: String,
}

/// 解析官方版本表：原版版本 → 该原版下的 LiteLoader 版本（**新在前**）。
///
/// # Errors
///
/// 顶层不是 `{"versions": {…}}`、或一个版本都认不出时返回 metadata 错误。
pub fn parse_versions(json: &str) -> Result<BTreeMap<String, Vec<LiteLoaderEntry>>, LoaderError> {
    let root: Root = serde_json::from_str(json)
        .map_err(|error| LoaderError::metadata(format!("LiteLoader 版本表解析失败：{error}")))?;
    let mut out: BTreeMap<String, Vec<LiteLoaderEntry>> = BTreeMap::new();
    for (mc_version, game) in &root.versions {
        let repo = game
            .repo
            .as_ref()
            .map(|repo| https_repo(&repo.url))
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| DEFAULT_REPO.to_string());
        let mut entries: Vec<LiteLoaderEntry> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        if let Some(branch) = &game.artefacts {
            collect(branch, mc_version, &repo, false, &mut entries, &mut seen);
        }
        if let Some(branch) = &game.snapshots {
            collect(branch, mc_version, &repo, true, &mut entries, &mut seen);
        }
        if entries.is_empty() {
            continue;
        }
        entries.sort_by(|left, right| cmp_versions(&right.version, &left.version));
        out.insert(mc_version.clone(), entries);
    }
    if out.is_empty() {
        return Err(LoaderError::not_found(
            "LiteLoader 版本表里没有任何可用版本（数据源形状变了？）",
        ));
    }
    Ok(out)
}

/// 收一个分支的记录（`latest` 别名与重复版本跳过）。
fn collect(
    branch: &Branch,
    mc_version: &str,
    repo: &str,
    snapshot_branch: bool,
    out: &mut Vec<LiteLoaderEntry>,
    seen: &mut BTreeSet<String>,
) {
    for (key, record) in &branch.liteloader {
        if key.eq_ignore_ascii_case("latest") || record.version.is_empty() {
            continue;
        }
        if !seen.insert(record.version.clone()) {
            continue;
        }
        // 记录自己标 SNAPSHOT 的也走快照路线（release 分支里出现过混编）。
        let snapshot = snapshot_branch || record.stream.eq_ignore_ascii_case("SNAPSHOT");
        let dir = format!("{repo}com/mumfrey/liteloader/{mc_version}/");
        let (url, snapshot_dir) = if snapshot {
            // 快照仓是**固定**的（见 `SNAPSHOT_REPO` 的注释），不跟 `repo.url` 走。
            let snap_dir = format!("{SNAPSHOT_REPO}com/mumfrey/liteloader/{mc_version}-SNAPSHOT/");
            (None, Some(snap_dir))
        } else {
            (
                (!record.file.is_empty()).then(|| format!("{dir}{}", record.file)),
                None,
            )
        };
        out.push(LiteLoaderEntry {
            version: record.version.clone(),
            mc_version: mc_version.to_string(),
            snapshot,
            tweak_class: record.tweak_class.clone(),
            libraries: record.libraries.iter().map(rebased).collect(),
            url,
            snapshot_dir,
        });
    }
}

/// `http://…` → `https://…`（上游表里混着 http；HMCL 同样改写）。
fn https_repo(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("http://") {
        return format!("https://{rest}");
    }
    url.to_string()
}

/// 记录级库：保留上游字段（`http` → `https`），只给「没写 url」的库补基址（见模块文档的 group 分流）。
fn rebased(library: &Library) -> Library {
    let mut out = library.clone();
    match out
        .url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        Some(url) => out.url = Some(https_repo(url)),
        None => {
            let group = out.name.split(':').next().unwrap_or_default();
            if group != "net.minecraft" {
                out.url = Some(CENTRAL_REPO.to_string());
            }
        }
    }
    out
}

/// 版本号比较：按 `.`/`-`/`_` 切段，纯数字段按数值比，其余按字面量比。
fn cmp_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let lhs: Vec<&str> = left.split(['.', '-', '_']).collect();
    let rhs: Vec<&str> = right.split(['.', '-', '_']).collect();
    for (a, b) in lhs.iter().zip(rhs.iter()) {
        let order = match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(a), Ok(b)) => a.cmp(&b),
            _ => a.cmp(b),
        };
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    lhs.len().cmp(&rhs.len())
}

/// 从 `maven-metadata.xml` 拼出快照产物的直链（`<dir>liteloader-<mc>-<ts>-<build>-release.jar`）。
///
/// # Errors
///
/// 缺 `<timestamp>`/`<buildNumber>` 时返回 metadata 错误（宁可报错，也不猜一个 404 直链）。
pub fn snapshot_artifact_url(
    entry: &LiteLoaderEntry,
    metadata_xml: &str,
) -> Result<String, LoaderError> {
    let dir = entry.snapshot_dir.as_deref().ok_or_else(|| {
        LoaderError::metadata(format!(
            "LiteLoader {} 不是快照版本，没有 maven 目录",
            entry.version
        ))
    })?;
    let timestamp = tag_value(metadata_xml, "timestamp").ok_or_else(|| {
        LoaderError::metadata(format!(
            "{} 的 maven-metadata.xml 里没有 <timestamp>：拿不到快照直链",
            entry.version
        ))
    })?;
    let build = tag_value(metadata_xml, "buildNumber").ok_or_else(|| {
        LoaderError::metadata(format!(
            "{} 的 maven-metadata.xml 里没有 <buildNumber>",
            entry.version
        ))
    })?;
    Ok(format!(
        "{dir}liteloader-{}-{timestamp}-{build}-release.jar",
        entry.mc_version
    ))
}

/// 取 `<tag>…</tag>` 里的文本（maven 元数据只有一层，不引 XML 库，与 [`super::maven`] 同口径）。
fn tag_value(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let value = xml[start..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}
