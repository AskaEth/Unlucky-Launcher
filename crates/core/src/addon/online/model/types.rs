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

/// 前置依赖的类型（`/version` 的 `dependencies[].dependency_type`）。
///
/// # 为什么要留 `Unknown` 而不是「认不出就当可选」
///
/// 站点以后加新取值时，**只有 `Required` 才该触发递归**。认不出的值落进 `Unknown`，
/// 于是「站点改名」的下场是「少装一个前置」（可观察、可补），而不是「把所有建议项都当必备项
/// 递归拉一遍」（一次装模组变成几十个请求，还装了一堆不兼容的东西）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DependencyType {
    /// 必须装（BFS **只跟这一档**）
    Required,
    /// 可选（装不装都能跑）
    #[default]
    Optional,
    /// 明确不兼容（**不要**和它一起装）
    Incompatible,
    /// 已内嵌在被依赖的文件里（不用另外装）
    Embedded,
    /// 站点给了个我们不认识的取值
    Unknown,
}

impl DependencyType {
    /// 站点原文。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Optional => "optional",
            Self::Incompatible => "incompatible",
            Self::Embedded => "embedded",
            Self::Unknown => "unknown",
        }
    }

    /// 从站点原文解析；认不出（含空串）返回 [`Self::Unknown`]。
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "required" => Self::Required,
            "optional" => Self::Optional,
            "incompatible" => Self::Incompatible,
            "embedded" => Self::Embedded,
            _ => Self::Unknown,
        }
    }
}

/// 版本声明的一条前置（`dependencies[]`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Dependency {
    /// 依赖的**项目** id（BFS 的地址；有了它就不用像参考实现那样再查一次 slug）
    pub project_id: Option<String>,
    /// 依赖的版本 id（站点基本只对 `embedded` 给）
    pub version_id: Option<String>,
    /// 站点给的文件名（只对 `embedded` / `incompatible` 有意义）
    pub file_name: Option<String>,
    /// 依赖类型
    pub dependency_type: DependencyType,
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
    /// 前置声明（站点原文顺序；空 = 站点没说 / 没有）
    pub dependencies: Vec<Dependency>,
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

    /// 要跟下去的**前置项目 id**：只取 `required`，去掉空值，**排序去重**。
    ///
    /// 排序在这里做（不是让调用方排）：BFS 的展开顺序必须是数据本身决定的，
    /// 否则站点把 `dependencies` 换个顺序、或者用 `HashMap` 收一遍，测试就会随机红。
    #[must_use]
    pub fn required_project_ids(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self
            .dependencies
            .iter()
            .filter(|dependency| dependency.dependency_type == DependencyType::Required)
            .filter_map(|dependency| dependency.project_id.as_deref())
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// 声明了 `required`、却**没有项目 id** 的前置（站点只给了版本 id / 文件名）。
    ///
    /// 这些条目 BFS 跟不下去（取版本要项目 id），但也不能当成「没有这条依赖」——
    /// 上层据此说「有 N 条前置没法解析」，见 `DependencyPlan::unresolved_required`。
    #[must_use]
    pub fn unresolved_required(&self) -> Vec<&Dependency> {
        self.dependencies
            .iter()
            .filter(|dependency| dependency.dependency_type == DependencyType::Required)
            .filter(|dependency| {
                dependency
                    .project_id
                    .as_deref()
                    .map(str::trim)
                    .map(str::is_empty)
                    .unwrap_or(true)
            })
            .collect()
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
