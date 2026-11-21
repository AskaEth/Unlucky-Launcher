//! 响应形状（`/mods/search`、`/mods/{id}`、`/mods/{id}/files`、`.../download-url`）。
//!
//! 字段名按站点原文（`downloadCount` / `dateModified` / `latestFilesIndexes`），翻译成 Rust 的
//! 蛇形只发生在 `parse` 那一层——本文件是**下游可以直接消费**的形状。

use std::collections::BTreeMap;

use super::query::{CfLoader, CfReleaseType};

/// 一页 mod 检索结果（`GET /mods/search` 的 `data` + `pagination`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModPage {
    /// 命中项（顺序 = 站点顺序）
    pub hits: Vec<Mod>,
    /// 站点给的总数（`pagination.totalCount`；`0` = 站点没给）
    pub total_count: u64,
    /// 站点回显的偏移
    pub index: u32,
    /// 站点回显的每页条数
    pub page_size: u32,
}

impl ModPage {
    /// 还有下一页吗。
    ///
    /// 判据是「本页装满了」，不是 `index + len < total_count`：两条源（Modrinth / CF）用同一个判据，
    /// 页面就不必为两家写两套翻页状态；`totalCount` 在关键词过滤下也未必是精确值。
    #[must_use]
    pub fn has_more(&self) -> bool {
        self.page_size > 0 && self.hits.len() as u32 >= self.page_size
    }
}

/// 站点给的一个「最新文件」索引（`latestFilesIndexes[]`）。
///
/// 检索结果里**没有**文件列表，只有这张表：它回答「这个 mod 最近给哪个游戏版本 / 哪个加载器
/// 发过文件」，也就是「装不装得上」的判据。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileIndex {
    /// 游戏版本（站点偶尔不给）
    pub game_version: Option<String>,
    /// 加载器（认不出的数字 ⇒ `None`）
    pub loader: Option<CfLoader>,
    /// 文件 id
    pub file_id: u32,
    /// 文件名
    pub filename: String,
    /// 版本类型
    pub release_type: CfReleaseType,
}

/// 一个 mod（项目，不是文件）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mod {
    /// 站点的数字 id（文件端点与装入指令的地址）
    pub id: u32,
    /// 标题（`name`）
    pub name: String,
    /// 人读 slug
    pub slug: String,
    /// 简介（`summary`）
    pub summary: String,
    /// 作者名（站点给的是团队/作者列表，这里按站点顺序取名字）
    pub authors: Vec<String>,
    /// 下载量
    pub download_count: u64,
    /// 最近更新（站点原文 ISO-8601）
    pub date_modified: String,
    /// 分类标签（站点的 `categories[].slug`）
    pub categories: Vec<String>,
    /// 最新文件索引
    pub latest_files: Vec<FileIndex>,
    /// 图标 URL（`None` = 没有）
    pub icon_url: Option<String>,
}

impl Mod {
    /// 支持的游戏版本（来自 `latestFilesIndexes`，去重、保持站点顺序）。
    #[must_use]
    pub fn game_versions(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for entry in &self.latest_files {
            let Some(version) = entry.game_version.as_deref().filter(|v| !v.is_empty()) else {
                continue;
            };
            push_unique(&mut out, version);
        }
        out
    }

    /// 支持的加载器（与 Modrinth 同口径的 facet 名；Cauldron / LiteLoader 不计入）。
    ///
    /// 空 = 站点没给索引（**不是**「什么加载器都不支持」）：上层按「未知 ⇒ 不拦」处理，
    /// 与 `online::ProjectHit::supports_loader` 同一条纪律。
    #[must_use]
    pub fn loaders(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for entry in &self.latest_files {
            let Some(facet) = entry.loader.and_then(CfLoader::facet) else {
                continue;
            };
            push_unique(&mut out, facet);
        }
        out
    }

    /// 支持某个加载器吗（空列表 = 站点没给 ⇒ 不拦）。
    #[must_use]
    pub fn supports_loader(&self, facet: &str) -> bool {
        let loaders = self.loaders();
        loaders.is_empty() || loaders.iter().any(|known| known == facet)
    }

    /// 支持某个游戏版本吗（空列表 = 站点没给 ⇒ 不拦）。
    #[must_use]
    pub fn supports_version(&self, version: &str) -> bool {
        let versions = self.game_versions();
        versions.is_empty() || versions.iter().any(|known| known == version)
    }
}

/// 一个 mod 文件（`GET /mods/{id}/files` 的 `data[]`，以及 `.../files/{fileId}`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModFile {
    /// 文件 id
    pub id: u32,
    /// 所属 mod id
    pub mod_id: u32,
    /// 展示名（`displayName`，作者写的人读版本名）
    pub display_name: String,
    /// 磁盘文件名（`fileName`）
    pub file_name: String,
    /// 版本类型
    pub release_type: CfReleaseType,
    /// 上传时间（站点原文 ISO-8601）
    pub file_date: String,
    /// 字节数
    pub file_length: u64,
    /// 该文件下载量
    pub download_count: u64,
    /// 站点给的下载地址（**可能为 `None`**：作者关闭第三方分发时站点不给）
    pub download_url: Option<String>,
    /// 摘要表（键小写：`sha1` / `md5`；站点只给 `algo` 数字，见 `parse::hash_algo`）
    pub hashes: BTreeMap<String, String>,
    /// 支持的游戏版本（**只有版本号**：站点把加载器名与 `Client`/`Server` 混在同一个
    /// `gameVersions` 数组里，解析时已分流，见 [`Self::loaders`]）
    pub game_versions: Vec<String>,
    /// 这个文件对应的加载器（从 `gameVersions` 里的加载器词推出来）
    pub loaders: Vec<String>,
}

impl ModFile {
    /// SHA-1（CF 的 `hashes[].algo == 1`）。
    #[must_use]
    pub fn sha1(&self) -> Option<&str> {
        self.hashes.get("sha1").map(String::as_str)
    }

    /// 站点给下载地址了吗（`None` / 空串都算没给）。
    #[must_use]
    pub fn has_download_url(&self) -> bool {
        self.download_url
            .as_deref()
            .is_some_and(|url| !url.trim().is_empty())
    }

    /// 这个文件能装到某个游戏版本 + 加载器上吗（空列表 = 站点没给 ⇒ 不拦，与 [`Mod`] 同纪律）。
    #[must_use]
    pub fn fits(&self, game_version: &str, loader: Option<&str>) -> bool {
        let version_ok = self.game_versions.is_empty()
            || self.game_versions.iter().any(|known| known == game_version);
        let loader_ok = match loader {
            None => true,
            Some(facet) => {
                self.loaders.is_empty() || self.loaders.iter().any(|known| known == facet)
            }
        };
        version_ok && loader_ok
    }
}

/// 保持顺序的去重追加（站点会把同一个值重复给多次：`latestFilesIndexes` 里一个加载器常出现
/// 好几次，一次一个游戏版本）。
pub(super) fn push_unique(target: &mut Vec<String>, value: &str) {
    if !target.iter().any(|known| known == value) {
        target.push(value.to_string());
    }
}
