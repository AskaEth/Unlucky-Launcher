//! 检索请求的形状（facet / 排序 / 分页）——站点要什么就写什么，不预先「通用化」。
//!
//! 两个容易踩的点：
//!
//! 1. `facets` 是**二维数组的 JSON 文本**（外层「与」、内层「或」），要再整体百分号编码；
//! 2. `categories` 里混着分类标签（`optimization`），只有 [`KNOWN_LOADERS`] 那几个才是加载器。

/// Modrinth 的 `project_type` 取值（检索 facet 与项目详情的同名字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    /// 整合包
    Modpack,
    /// Mod
    Mod,
    /// 资源包
    Resourcepack,
    /// 光影
    Shader,
    /// 服务端插件
    Plugin,
    /// 数据包
    Datapack,
}

impl ContentType {
    /// 站点原文（`project_type` facet 的值）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Modpack => "modpack",
            Self::Mod => "mod",
            Self::Resourcepack => "resourcepack",
            Self::Shader => "shader",
            Self::Plugin => "plugin",
            Self::Datapack => "datapack",
        }
    }

    /// 从站点原文解析；认不出返回 `None`（调用方决定当「未知」还是报错）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "modpack" => Self::Modpack,
            "mod" => Self::Mod,
            "resourcepack" => Self::Resourcepack,
            "shader" => Self::Shader,
            "plugin" => Self::Plugin,
            "datapack" => Self::Datapack,
            _ => return None,
        })
    }
}

/// `/v2/search` 的 `index` 参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortIndex {
    /// 相关度（站点默认）
    #[default]
    Relevance,
    /// 下载量
    Downloads,
    /// 关注数
    Follows,
    /// 最新发布
    Newest,
    /// 最近更新
    Updated,
}

impl SortIndex {
    /// 站点原文。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Follows => "follows",
            Self::Newest => "newest",
            Self::Updated => "updated",
        }
    }
}

/// 站点认的加载器 facet 值（`categories` 里混着 `optimization` 这种**分类标签**，
/// 直接整份当加载器会画出假加载器）。
///
/// 后两项是**光影**的加载器（Modrinth 把 shader 的 `iris` / `optifine` 也放在 `categories`
/// 里）：不登记它们，光影检索结果的 `loaders` 就会恒为空，页面既画不出「支持 Iris」，
/// 也用不上「只看 Iris」这种判断。
pub const KNOWN_LOADERS: [&str; 8] = [
    "fabric",
    "forge",
    "neoforge",
    "quilt",
    "liteloader",
    "rift",
    "iris",
    "optifine",
];

/// 一次检索请求（分页游标也在里面：换页就是换一份 query）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    /// 关键词（空 = 不带 `query` 参数，即「按排序浏览」）
    pub text: String,
    /// 内容类型过滤
    pub project_type: Option<ContentType>,
    /// 游戏版本过滤（精确值，如 `1.20.1`）
    pub game_version: Option<String>,
    /// 加载器过滤（站点 facet 值，如 `fabric`）
    pub loader: Option<String>,
    /// 排序
    pub index: SortIndex,
    /// 每页条数（站点上限 100，构造时钳住）
    pub limit: u32,
    /// 偏移
    pub offset: u32,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            text: String::new(),
            project_type: None,
            game_version: None,
            loader: None,
            index: SortIndex::Relevance,
            limit: 20,
            offset: 0,
        }
    }
}

impl SearchQuery {
    /// 站点上限（超过会被站点拒，钳住更省一次往返）。
    pub const MAX_LIMIT: u32 = 100;

    /// 钳过界的分页参数。
    #[must_use]
    pub fn normalized(&self) -> Self {
        let mut query = self.clone();
        query.text = query.text.trim().to_string();
        query.limit = query.limit.clamp(1, Self::MAX_LIMIT);
        query
    }

    /// `facets` 参数（站点要的是**二维数组的 JSON**，每一项是一组「或」）。
    ///
    /// 形状：`[["project_type:mod"],["versions:1.20.1"],["categories:fabric"]]`——不同组之间是
    /// 「与」，同组内是「或」。这里每组只有一项。
    #[must_use]
    pub fn facets(&self) -> String {
        let mut groups: Vec<Vec<String>> = Vec::new();
        if let Some(kind) = self.project_type {
            groups.push(vec![format!("project_type:{}", kind.as_str())]);
        }
        if let Some(version) = self.game_version.as_deref().filter(|v| !v.is_empty()) {
            groups.push(vec![format!("versions:{version}")]);
        }
        if let Some(loader) = self.loader.as_deref().filter(|v| !v.is_empty()) {
            groups.push(vec![format!("categories:{loader}")]);
        }
        serde_json::to_string(&groups).unwrap_or_else(|_| "[]".to_string())
    }
}
