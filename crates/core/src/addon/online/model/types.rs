//! 响应形状（`/v2/search`、`/v2/project`、`/v2/project/{id}/version`、`/v2/version_file`）。

use std::collections::BTreeMap;

/// 一页检索结果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchPage {
    /// 命中项（顺序 = 站点顺序）
    pub hits: Vec<ProjectHit>,
    /// 站点给的总数（**估算值**：站点文档不保证精确）
    pub total_hits: u64,
    /// 请求的偏移
    pub offset: u32,
    /// 请求的每页条数
    pub limit: u32,
}

impl SearchPage {
    /// 还有下一页吗。
    ///
    /// 判据是「本页装满了」，不是 `offset + len < total`：`total_hits` 是估算值，
    /// 用「装满」最多多请求一次空页，不会出现「明明还有却翻不到」。
    #[must_use]
    pub fn has_more(&self) -> bool {
        self.limit > 0 && self.hits.len() as u32 >= self.limit
    }
}

/// 一条检索命中（`/v2/search` 的 `hits[]`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectHit {
    /// 项目 id（base62）
    pub project_id: String,
    /// 人读 slug
    pub slug: String,
    /// 标题
    pub title: String,
    /// 简介
    pub description: String,
    /// 作者（站点给的是团队名；没有时为空）
    pub author: String,
    /// 下载量
    pub downloads: u64,
    /// 关注数
    pub follows: u64,
    /// 最近更新（站点原文 ISO-8601）
    pub updated: String,
    /// 支持的加载器（已按 [`KNOWN_LOADERS`] 过滤）
    pub loaders: Vec<String>,
    /// 支持的游戏版本
    pub game_versions: Vec<String>,
    /// 项目类型原文
    pub project_type: String,
    /// 图标 URL（`None` = 没有）
    pub icon_url: Option<String>,
}

impl ProjectHit {
    /// 支持某个加载器吗（空列表 = 站点没给，按「未知」处理 ⇒ 不拦）。
    #[must_use]
    pub fn supports_loader(&self, loader: &str) -> bool {
        self.loaders.is_empty() || self.loaders.iter().any(|item| item == loader)
    }

    /// 支持某个游戏版本吗（空列表 = 未知 ⇒ 不拦）。
    #[must_use]
    pub fn supports_version(&self, version: &str) -> bool {
        self.game_versions.is_empty() || self.game_versions.iter().any(|item| item == version)
    }
}

/// 项目详情（`/v2/project/{id or slug}`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectDetail {
    /// 项目 id
    pub id: String,
    /// slug
    pub slug: String,
    /// 标题
    pub title: String,
    /// 简介
    pub description: String,
    /// 长文（markdown 原文；站点可能给 `""`）
    pub body: String,
    /// 项目类型原文
    pub project_type: String,
    /// 下载量
    pub downloads: u64,
    /// 关注数
    pub follows: u64,
    /// 最近更新
    pub updated: String,
    /// 首次发布
    pub published: String,
    /// 游戏版本
    pub game_versions: Vec<String>,
    /// 加载器
    pub loaders: Vec<String>,
    /// 分类标签
    pub categories: Vec<String>,
    /// 图标
    pub icon_url: Option<String>,
    /// 项目主页
    pub source_url: Option<String>,
    /// 问题追踪
    pub issues_url: Option<String>,
}

/// 版本类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VersionType {
    /// 正式版
    #[default]
    Release,
    /// 测试版
    Beta,
    /// 早期预览
    Alpha,
}

impl VersionType {
    /// 站点原文。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Beta => "beta",
            Self::Alpha => "alpha",
        }
    }

    /// 从站点原文解析；认不出按正式版（**不**拿它做安装判据）。
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw {
            "beta" => Self::Beta,
            "alpha" => Self::Alpha,
            _ => Self::Release,
        }
    }
}

/// 一个版本（`/v2/project/{id}/version[]` 与 `/v2/version_file/{hash}` 同一形状）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Version {
    /// 版本 id
    pub id: String,
    /// 所属项目 id
    pub project_id: String,
    /// 版本名
    pub name: String,
    /// 版本号（`1.2.3+fabric`）
    pub version_number: String,
    /// 版本类型
    pub version_type: VersionType,
    /// 支持的加载器
    pub loaders: Vec<String>,
    /// 支持的游戏版本
    pub game_versions: Vec<String>,
    /// 发布日期
    pub date_published: String,
    /// 该版本总下载量
    pub downloads: u64,
    /// 文件列表
    pub files: Vec<VersionFile>,
}

impl Version {
    /// 拿这个文件：优先 `primary: true`（Modrinth 官方口径），否则第一个。
    ///
    /// 不能无脑取第一个：很多项目同时发 `sources.jar` / `-dev.jar`，取第一个会装错东西。
    #[must_use]
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files
            .iter()
            .find(|file| file.primary)
            .or_else(|| self.files.first())
    }
}

/// 版本里的一个文件。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersionFile {
    /// 摘要表（键名小写：`sha1` / `sha512` / `md5`；站点给几个就有几个）
    pub hashes: BTreeMap<String, String>,
    /// 下载地址
    pub url: String,
    /// 文件名
    pub filename: String,
    /// 字节数
    pub size: u64,
    /// 是否主文件
    pub primary: bool,
}

impl VersionFile {
    /// SHA-1。
    #[must_use]
    pub fn sha1(&self) -> Option<&str> {
        self.hashes.get("sha1").map(String::as_str)
    }

    /// SHA-512。
    #[must_use]
    pub fn sha512(&self) -> Option<&str> {
        self.hashes.get("sha512").map(String::as_str)
    }
}

/// 摘要表（给需要「按算法取」的调用方用；与 [`VersionFile::hashes`] 同源）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileHashes(pub BTreeMap<String, String>);

impl FileHashes {
    /// 按算法取。
    #[must_use]
    pub fn get(&self, algorithm: &str) -> Option<&str> {
        self.0.get(algorithm).map(String::as_str)
    }
}

/// 版本列表请求（`/v2/project/{id}/version` 的过滤条件）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersionQuery {
    /// 项目 id 或 slug
    pub project: String,
    /// 游戏版本过滤（单个精确值）
    pub game_version: Option<String>,
    /// 加载器过滤（站点 facet 值）
    pub loader: Option<String>,
}
