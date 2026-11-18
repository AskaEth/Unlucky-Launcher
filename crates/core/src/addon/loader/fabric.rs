//! Fabric 系（Fabric / Quilt）：官方 meta → loader 版本 JSON。
//!
//! 依据 `DevDocs/analyse/05-modloader-modpack.md` §1.4：这两个加载器**没有 installer**，
//! `meta.fabricmc.net` / `meta.quiltmc.org` 直接给 launcher 可用的 profile JSON，
//! 所以安装 = "把 profile 收成我们的版本 JSON + 补库"，没有任何子进程。
//!
//! 本文件是**纯函数**（不进网络）：URL 拼装与 JSON 归一化分开写，测试直接喂离线夹具。
//!
//! # 归一化做了三件容易被上游坑的事
//!
//! 1. **`jar` 钉成原版 id**：profile 通常不写 `jar`，而 `game::inherit::resolve` 在缺 `jar`
//!    时会把 `jar` 兜底成**自身 id**——那会让启动去找 `versions/<loader>/<loader>.jar`
//!    （不存在）。这里显式写 `jar = <原版 id>`，让继承链正确地回到原版 jar。
//! 2. **`mainClass` 断言**：缺主类就不是一份能启动的清单，宁可在安装期报 metadata，
//!    也不要装出一个起不来的实例（HMCL 同款防御，analyse/05 §1.4）。
//! 3. **扁平 `sha1`/`size` 提升成 `downloads.artifact`**：Fabric profile 的库把摘要写在
//!    库对象顶层，M27 的库展开只认 `downloads.artifact`。不提上来的后果是"没有校验值 →
//!    存在即信"，重装时无法识别被截断的库文件。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::plan::{LoaderRelease, LoaderSupport};
use super::{LoaderError, LoaderKind};
use crate::game::library::{Library, LibraryDownloads};
use crate::game::manifest::{DownloadInfo, VersionJson};

/// Fabric 官方 meta 根（BMCLAPI 有对应镜像规则 `bmclapi-fabric-meta`）。
pub const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
/// Quilt 官方 meta 根（同上，`bmclapi-quilt-meta`）。
pub const QUILT_META: &str = "https://meta.quiltmc.org/v3";

/// meta 根地址（按加载器；非 fabric 系给空串，调用方已按 [`LoaderKind::is_fabric_family`] 分流）。
#[must_use]
pub fn meta_base(kind: LoaderKind) -> &'static str {
    match kind {
        LoaderKind::Fabric => FABRIC_META,
        LoaderKind::Quilt => QUILT_META,
        // Forge / NeoForge 走 maven，LiteLoader 走官方版本表——都不能回落到 fabric meta。
        LoaderKind::NeoForge | LoaderKind::Forge | LoaderKind::LiteLoader => "",
    }
}

/// 该原版版本可用的加载器版本列表地址。
#[must_use]
pub fn loader_list_url(kind: LoaderKind, mc_version: &str) -> String {
    format!("{}/versions/loader/{mc_version}", meta_base(kind))
}

/// 指定加载器版本的 launcher profile 地址（官方直出可用的版本 JSON）。
#[must_use]
pub fn profile_url(kind: LoaderKind, mc_version: &str, loader_version: &str) -> String {
    format!(
        "{}/versions/loader/{mc_version}/{loader_version}/profile/json",
        meta_base(kind)
    )
}

/// 列表里的一个加载器版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaderEntry {
    /// 版本号（`0.15.11`）。
    #[serde(default)]
    pub version: String,
    /// 是否稳定版（Quilt 的列表不带这个键，缺省按稳定处理）。
    #[serde(default = "default_true")]
    pub stable: bool,
    /// Maven 坐标（`net.fabricmc:fabric-loader:0.15.11`）。
    #[serde(default)]
    pub maven: String,
}

const fn default_true() -> bool {
    true
}

/// 列表响应的一行（`[{ "loader": {…}, "intermediary": {…}, … }]`）。
#[derive(Debug, Deserialize)]
struct ListRow {
    #[serde(default)]
    loader: Option<LoaderEntry>,
}

/// 解析版本列表（最新的排在前，与官方顺序一致——**不重排**）。
///
/// # Errors
///
/// JSON 非法、或列表形状完全不符（顶层不是数组 / 一条都认不出）时返回 metadata 错误。
pub fn parse_loader_list(kind: LoaderKind, json: &str) -> Result<Vec<LoaderEntry>, LoaderError> {
    let rows: Vec<ListRow> = serde_json::from_str(json).map_err(|error| {
        LoaderError::metadata(format!("{} 版本列表解析失败：{error}", kind.label()))
    })?;
    let entries: Vec<LoaderEntry> = rows
        .into_iter()
        .filter_map(|row| row.loader)
        .filter(|entry| !entry.version.is_empty())
        .collect();
    if entries.is_empty() {
        return Err(LoaderError::not_found(format!(
            "{} 版本列表为空（该原版版本可能还没有对应加载器）",
            kind.label()
        )));
    }
    Ok(entries)
}

/// 从列表里挑一个默认版本：优先第一个稳定版，否则取第一个（列表最新在前）。
#[must_use]
pub fn pick_default(entries: &[LoaderEntry]) -> Option<&LoaderEntry> {
    entries
        .iter()
        .find(|entry| entry.stable)
        .or_else(|| entries.first())
}

/// 把官方 profile JSON 归一化成 `<mc>-<kind>-<loader>` 的 loader 版本 JSON。
///
/// # Errors
///
/// profile 不是合法版本 JSON、缺主类时返回 metadata 错误。
pub fn build_release(
    kind: LoaderKind,
    mc_version: &str,
    loader_version: &str,
    profile_json: &str,
) -> Result<LoaderRelease, LoaderError> {
    let mut version_json = VersionJson::from_json_str(profile_json).map_err(|error| {
        LoaderError::metadata(format!(
            "{} {loader_version} 的 profile JSON 解析失败：{error}",
            kind.label()
        ))
    })?;
    let mut warnings = Vec::new();

    let version_id = kind.version_id(mc_version, loader_version);
    version_json.id = Some(version_id.clone());
    version_json.name = Some(format!("{mc_version} · {} {loader_version}", kind.label()));
    version_json.inherits_from = Some(mc_version.to_string());
    // 启动用原版 jar（见模块文档第 1 条）。
    version_json.jar = Some(mc_version.to_string());

    if version_json
        .main_class
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err(LoaderError::metadata(format!(
            "{} {loader_version} 的 profile 没有 mainClass：这份清单装上去也起不来",
            kind.label()
        ))
        .with_hint("换一个加载器版本，或用官方启动器核对"));
    }

    let libraries = version_json.libraries.get_or_insert_with(Vec::new);
    for library in libraries.iter_mut() {
        lift_flat_digest(library);
    }
    ensure_loader_library(kind, loader_version, libraries, &mut warnings);

    Ok(LoaderRelease {
        kind,
        mc_version: mc_version.to_string(),
        parent_version_id: mc_version.to_string(),
        loader_version: loader_version.to_string(),
        version_id,
        version_json,
        support: LoaderSupport::JsonRoute,
        installer: None,
        install_profile: None,
        warnings,
    })
}

/// 把库对象顶层的 `sha1`/`size` 提升成 `downloads.artifact`（Fabric profile 的写法）。
fn lift_flat_digest(library: &mut Library) {
    if library.downloads.is_some() {
        return;
    }
    let Some(base) = library
        .url
        .as_deref()
        .filter(|url| !url.is_empty())
        .map(str::to_string)
    else {
        return;
    };
    let Ok(relative) = library.relative_path() else {
        return;
    };
    let sha1 = library
        .extra
        .get("sha1")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let size = library
        .extra
        .get("size")
        .and_then(serde_json::Value::as_u64);
    let path = relative.to_string_lossy().replace('\\', "/");
    let url = if base.ends_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    };
    library.downloads = Some(LibraryDownloads {
        artifact: Some(DownloadInfo {
            id: None,
            sha1,
            size,
            url: Some(url),
            path: Some(path),
        }),
        classifiers: BTreeMap::new(),
    });
}

/// loader 本体库必须在列表里：缺了就补一条（并用告警说明），别装出个"没有 loader"的实例。
///
/// 只有 fabric 系会走到这里（调用方 [`build_release`] 的入口已按
/// [`LoaderKind::is_fabric_family`] 分流）；这里仍然**穷尽 match**，因为过去那对 `_ =>`
/// 会在新加一支加载器时把它静默当成 `fabric-loader` 补进库表。
fn ensure_loader_library(
    kind: LoaderKind,
    loader_version: &str,
    libraries: &mut Vec<Library>,
    warnings: &mut Vec<String>,
) {
    let (group, artifact, maven_base) = match kind {
        LoaderKind::Fabric => (
            "net.fabricmc",
            "fabric-loader",
            "https://maven.fabricmc.net/",
        ),
        LoaderKind::Quilt => (
            "org.quiltmc",
            "quilt-loader",
            "https://maven.quiltmc.org/repository/release/",
        ),
        LoaderKind::NeoForge | LoaderKind::Forge | LoaderKind::LiteLoader => return,
    };
    let present = libraries.iter().any(|library| {
        library
            .coordinate()
            .is_ok_and(|coordinate| coordinate.group == group && coordinate.artifact == artifact)
    });
    if present {
        return;
    }
    libraries.push(Library {
        name: format!("{group}:{artifact}:{loader_version}"),
        url: Some(maven_base.to_string()),
        ..Library::default()
    });
    warnings.push(format!(
        "{} {loader_version} 的 profile 没带 {group}:{artifact}：已按 maven 坐标补一条（缺库会让游戏起不来）",
        kind.label()
    ));
}
