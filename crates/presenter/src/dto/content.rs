//! 内容库 DTO（M109 `CONTENTDL`）：Modrinth / CurseForge 上「整合包 / Mod / 资源包 / 光影」的
//! **检索**与**装入指定实例**。
//!
//! ## 为什么这些类型在 presenter 而不是 core
//!
//! D3 / 01c §5：UI 只经 `crate::dto` 消费后端，跨边界类型必须可序列化。而**检索本身**
//! （HTTP + 两个站点的 JSON 方言）今天落在 `crate::host::addon`：`crates/core` 的 `addon` 模块
//! 只有加载器安装与整合包**格式**解析，没有 CF / Modrinth 的 API 客户端（M111 才做导入管线）。
//! 所以这是「宿主层临时落点」的契约面——长期应下沉到 `core::addon`，届时这里只留薄 DTO。
//!
//! 两个形状是**故意**的：
//!
//! - [`ContentHit::game_versions`] / [`ContentHit::loaders`] 来自检索结果的 facets ⇒ 页面在用户
//!   **点之前**就能判「装不装得上」（任务书第 3 条），不用点完才报错；
//! - [`ContentKind::dir`] 的 `None`（整合包）是「按钮禁用」的**唯一**判据，不是一个文案开关。
//!
//! ## 密钥纪律（任务书第 5 条）
//!
//! CurseForge 的 `X-API-KEY` 从环境变量 `ULL_CF_API_KEY` 读，**不进仓库、不进 DTO、不进日志**。
//! 这里只有 [`ContentResults::curseforge`] 一个 `bool`：源可用与否。
//! 于是「密钥泄漏」在类型层就不可能——本 crate 没有任何字段能装下它。

use serde::{Deserialize, Serialize};

use crate::dto::{InstanceId, LoaderKind};

/// 内容类型（内容库页左侧那四条一级侧栏）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContentKind {
    /// 整合包（Modpack）
    Modpack,
    /// Mod
    Mod,
    /// 资源包（Resource pack）
    Resourcepack,
    /// 光影（Shader pack）
    Shader,
}

impl ContentKind {
    /// 四条侧栏的声明顺序（= 画出来的顺序）。
    pub const ALL: [ContentKind; 4] = [
        ContentKind::Modpack,
        ContentKind::Mod,
        ContentKind::Resourcepack,
        ContentKind::Shader,
    ];

    /// 稳定 slug（日志 / 文件名 / 命令行参数用，**不要改名**）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Modpack => "modpack",
            Self::Mod => "mod",
            Self::Resourcepack => "resourcepack",
            Self::Shader => "shader",
        }
    }

    /// Modrinth 的 `project_type` facet 值。
    #[must_use]
    pub const fn modrinth_type(self) -> &'static str {
        match self {
            Self::Modpack => "modpack",
            Self::Mod => "mod",
            Self::Resourcepack => "resourcepack",
            Self::Shader => "shader",
        }
    }

    /// 装入实例时的落盘目录（相对运行目录）；`None` = 不是「一个文件进一个目录」这种语义。
    ///
    /// 整合包没有目录：它要走 core 的导入管线（解压 + 补全 + 登记实例），那是 M111 的活，
    /// 所以这里是 `None`——**按钮禁用由这个 `None` 驱动**，不是靠一句文案。
    #[must_use]
    pub const fn dir(self) -> Option<&'static str> {
        match self {
            Self::Modpack => None,
            Self::Mod => Some("mods"),
            Self::Resourcepack => Some("resourcepacks"),
            Self::Shader => Some("shaderpacks"),
        }
    }

    /// CurseForge 的 `classId`（CF API 的类目字段；数值是官方文档里的固定值）。
    #[must_use]
    pub const fn curseforge_class(self) -> u32 {
        match self {
            Self::Modpack => 4471,
            Self::Mod => 6,
            Self::Resourcepack => 12,
            Self::Shader => 6552,
        }
    }
}

/// 内容来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContentSource {
    /// Modrinth（无需 API key，默认源）
    Modrinth,
    /// CurseForge（需要 `ULL_CF_API_KEY`）
    Curseforge,
}

impl ContentSource {
    /// 两个来源的声明顺序（= 筛选条顺序）。
    pub const ALL: [ContentSource; 2] = [ContentSource::Modrinth, ContentSource::Curseforge];

    /// 稳定 slug。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Modrinth => "modrinth",
            Self::Curseforge => "curseforge",
        }
    }
}

/// 排序方式（两个站点的官方排序项，口径对齐后再映射到各自参数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContentSort {
    /// 相关度（Modrinth `relevance`；CF 没有对应项，退回热度）
    Relevance,
    /// 下载量
    Downloads,
    /// 最新发布
    Newest,
    /// 最近更新
    Updated,
}

impl ContentSort {
    /// 四种排序的声明顺序。
    pub const ALL: [ContentSort; 4] = [
        ContentSort::Relevance,
        ContentSort::Downloads,
        ContentSort::Newest,
        ContentSort::Updated,
    ];

    /// Modrinth `/v2/search` 的 `index` 参数值。
    #[must_use]
    pub const fn modrinth_index(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Newest => "newest",
            Self::Updated => "updated",
        }
    }

    /// CurseForge 的 `sortField`（1 = 热度，2 = 更新，3 = 发布，4 = 名称）。
    #[must_use]
    pub const fn curseforge_field(self) -> u32 {
        match self {
            Self::Relevance | Self::Downloads => 1,
            Self::Newest => 3,
            Self::Updated => 2,
        }
    }
}

/// 加载器 → Modrinth 的 loader facet 值；`None` = 该加载器没有对应 facet（原版等）。
#[must_use]
pub const fn loader_facet(loader: LoaderKind) -> Option<&'static str> {
    match loader {
        LoaderKind::Fabric => Some("fabric"),
        LoaderKind::Quilt => Some("quilt"),
        LoaderKind::Forge => Some("forge"),
        LoaderKind::NeoForge => Some("neoforge"),
        LoaderKind::Vanilla | LoaderKind::OptiFine | LoaderKind::LiteLoader => None,
    }
}

/// 一次检索请求（**页面把它整份发出去**，不做本地过滤之外的乐观处理）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentQuery {
    /// 内容类型
    pub kind: ContentKind,
    /// 来源
    pub source: ContentSource,
    /// 关键词（空串 = 不带 `query` 参数，即「按热度浏览」）
    pub text: String,
    /// 游戏版本过滤（`None` = 不过滤）
    pub game_version: Option<String>,
    /// 加载器过滤（`None` = 不过滤；对资源包 / 光影也照发——静默丢掉会让筛选条撒谎）
    pub loader: Option<LoaderKind>,
    /// 排序
    pub sort: ContentSort,
    /// 偏移（分页游标）
    pub offset: u32,
    /// 每页条数
    pub limit: u32,
}

impl ContentQuery {
    /// 每页条数（20 = Modrinth 默认上限的一半，翻页手感与请求量折中）。
    pub const PAGE: u32 = 20;

    /// 某个类型的首页（来源 Modrinth、无关键词、无过滤、按相关度）。
    #[must_use]
    pub fn first(kind: ContentKind) -> Self {
        Self {
            kind,
            source: ContentSource::Modrinth,
            text: String::new(),
            game_version: None,
            loader: None,
            sort: ContentSort::Relevance,
            offset: 0,
            limit: Self::PAGE,
        }
    }

    /// 换一批（保留全部筛选条件，只把游标往后挪一页）。
    #[must_use]
    pub fn next_page(&self) -> Self {
        Self {
            offset: self.offset.saturating_add(self.limit),
            ..self.clone()
        }
    }

    /// 筛选条件是否与另一份相同（**不含游标**：判「结果是不是同一批条件拉回来的」用它）。
    #[must_use]
    pub fn same_filters(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.source == other.source
            && self.text == other.text
            && self.game_version == other.game_version
            && self.loader == other.loader
            && self.sort == other.sort
    }
}

/// 一条检索结果（一个项目，不是文件）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentHit {
    /// 项目 id（Modrinth 的 base62 id / CF 的 modId；装入指令的载荷）
    pub id: String,
    /// 人读 slug（`sodium`；链接与日志里更可读）
    pub slug: String,
    /// 标题
    pub title: String,
    /// 一句话说明（站点原文，不翻译：它是用户内容，不是界面文案）
    pub summary: String,
    /// 作者（Modrinth 只有团队 id 时给空串）
    pub author: String,
    /// 下载量
    pub downloads: u64,
    /// 最近更新时间（ISO-8601 的日期部分；未知给空串）
    pub updated: String,
    /// 来源
    pub source: ContentSource,
    /// 支持的游戏版本（升序由站点给；页面只做「包不包含」判断）
    pub game_versions: Vec<String>,
    /// 支持的加载器（`fabric` / `forge` / `neoforge` / `quilt`）
    pub loaders: Vec<String>,
    /// 图标 URL（`None` = 没有；页面不拉图，只把主机名画成占位标记）
    pub icon: Option<String>,
}

impl ContentHit {
    /// 这个项目支持某个游戏版本吗（空列表 = 站点没给，按「未知」处理 ⇒ 不拦）。
    #[must_use]
    pub fn supports_version(&self, version: &str) -> bool {
        self.game_versions.is_empty() || self.game_versions.iter().any(|item| item == version)
    }

    /// 这个项目支持某个加载器吗（空列表 = 站点没给，按「未知」处理 ⇒ 不拦）。
    #[must_use]
    pub fn supports_loader(&self, loader: LoaderKind) -> bool {
        let Some(facet) = loader_facet(loader) else {
            return true;
        };
        self.loaders.is_empty() || self.loaders.iter().any(|item| item == facet)
    }
}

/// 一批检索结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentResults {
    /// 这份结果对应的请求（页面据此判断「这批还算不算当前条件」）
    pub query: ContentQuery,
    /// 命中项（顺序 = 站点顺序）
    pub hits: Vec<ContentHit>,
    /// 命中总数（站点给的分页总数；`0` = 站点没给）
    pub total: u32,
    /// CurseForge 源此刻是否可用（`false` = 没有 `ULL_CF_API_KEY`）
    pub curseforge: bool,
}

impl ContentResults {
    /// 还有下一页吗。
    ///
    /// 判据是「本页装满了」而不是 `offset + len < total`：站点给的 `total` 有时是估算值，
    /// 用「装满」判定最多多请求一次空页，不会出现「明明还有却翻不到」。
    #[must_use]
    pub fn has_more(&self) -> bool {
        self.hits.len() as u32 >= self.query.limit
    }
}

/// 「装入指定实例」的请求（UI 侧只发意图；选哪个文件由 host 按版本 + 加载器选）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentInstallRequest {
    /// 项目 id
    pub project: String,
    /// 项目标题（进度面板上显示的那一行；**不是** i18n 文案）
    pub title: String,
    /// 内容类型（决定落盘目录与「能不能装」）
    pub kind: ContentKind,
    /// 来源
    pub source: ContentSource,
    /// 目标实例
    pub instance: InstanceId,
    /// 目标实例的游戏版本（选文件用）
    pub game_version: String,
    /// 目标实例的加载器（选文件用）
    pub loader: LoaderKind,
}

/// 内容库页的状态（M109）：**检索条件 + 最近一批结果 + 请求态**。
///
/// 条件为什么也存在这里：页面层不持有业务状态是老纪律（同 `download_query`）；换页只挪游标、
/// 条件只有一份，才不会出现「筛选条说的是 A、列表画的是 B」。
#[derive(Debug, Clone, PartialEq)]
pub struct ContentState {
    /// 当前检索条件（游标也在里面：换页就是换一份 `query`）
    pub query: ContentQuery,
    /// 最近一批结果（`None` = 还没拉过）
    pub results: Option<ContentResults>,
    /// 有请求在飞（页面画加载态的依据；命中项只由事件写进来，不做乐观回显）
    pub pending: bool,
    /// 最近一次失败的原文（`None` = 没失败）
    pub failed: Option<String>,
    /// **最近一页**是否装满（`hits.len() >= limit`）。
    ///
    /// 为什么不是「累计条数够一页」：分页是**累加**到 `results.hits` 里的，累计值在第 2 页之后
    /// 必然 ≥ limit ⇒ 用累计判会得到「has_more 恒真」：加载更多永不消失、「没有更多了」在真实
    /// 翻页路径上不可达，而且每点一次白发一次网络请求（复审 P1-2）。所以这一位单独记。
    pub last_page_filled: bool,
}

impl Default for ContentState {
    fn default() -> Self {
        Self {
            query: ContentQuery::first(ContentKind::Mod),
            results: None,
            pending: false,
            failed: None,
            last_page_filled: false,
        }
    }
}

impl ContentState {
    /// 这批结果还算当前条件吗（`false` = 迟到的事件，丢掉，不许覆盖新的筛选结果）。
    #[must_use]
    pub fn accepts(&self, results: &ContentResults) -> bool {
        results.query.same_filters(&self.query)
    }
}
#[cfg(test)]
mod tests;
