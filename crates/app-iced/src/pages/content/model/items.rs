//! 内容库页的类型：页面自留状态 / 四态 / 展示行 / 目标实例 / 装不上的原因。
//!
//! 从 `pages/content/model.rs` 拆出（那个文件已贴 400 行硬上限）。纯数据类型与纯函数，
//! 不认识 iced：`model/tests.rs` 直接钉它们的判定。

use unlucky_presenter::dto::{ContentHit, ContentKind, ContentSource, InstanceSummary, LoaderKind};

use unlucky_i18n::{Catalog, Key};

/// 页面自留的选择（检索条件与结果在 presenter 的 `AppState::content`）。
#[derive(Debug, Clone, PartialEq)]
pub struct PageState {
    /// 当前内容类型
    pub kind: ContentKind,
    /// 选中的项目 id（`None` = 没选 → 详情面板空态）
    pub selected: Option<String>,
    /// 目标实例 id（`None` = 没选 → 装入按钮禁用并说明）
    pub target: Option<String>,
    /// 关键词输入框里的草稿（**未提交**；提交后才进 `AppState::content.query`）
    pub draft: String,
    /// 导入/导出区的页内选择（M113：路径草稿 / 目标实例 / 格式 / 展开的那条冲突）
    pub pack: super::super::packui::PackPageState,
}

impl Default for PageState {
    fn default() -> Self {
        Self {
            kind: ContentKind::Mod,
            selected: None,
            target: None,
            draft: String::new(),
            pack: super::super::packui::PackPageState::default(),
        }
    }
}

/// 页面此刻画哪一态（四态 + 分页尾）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// 加载中（首屏 / 换条件 / 换页）
    Loading,
    /// 有命中项
    Ready,
    /// 拉到了但一条都没有
    Empty,
    /// 失败（带一句人读原因）
    Failed(String),
    /// 未联网（M162）：**不是**「没有匹配的内容」，也不是普通失败——单独一屏，
    /// 说清「要么联网要么没结果」这件事不可能靠空列表表达。
    Offline(String),
}

impl Status {
    /// 失败了吗（视图用它决定画错误面板还是列表）。
    pub const fn failed(&self) -> bool {
        matches!(self, Self::Failed(_) | Self::Offline(_))
    }

    /// 未联网吗（视图据此画离线态而不是通用失败态）。
    pub const fn offline(&self) -> bool {
        matches!(self, Self::Offline(_))
    }
}

/// 版本列表的一行（M162：详情通道拉回来的 `ContentVersionView`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRow {
    /// 版本号（`mc1.20.1-0.5.11`）
    pub number: String,
    /// 版本类型（`release` / `beta` / `alpha`，站点原文）
    pub kind: String,
    /// 文件名（`None` = 站点没给）
    pub file_name: Option<String>,
    /// 已经求好文案的一行（`版本号 · 类型 · 日期`）
    pub line: String,
}

/// 一条命中项的展示态（文案在这一层求好：iced 的文本控件借用内容）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// 项目 id（选中 / 装入指令的载荷）
    pub id: String,
    /// 标题
    pub title: String,
    /// 一行说明
    pub summary: String,
    /// 来源 + 作者（`Modrinth · jelly`）
    pub supporting: String,
    /// 下载量（已加千分位）
    pub downloads: String,
    /// 更新日期的人读串
    pub updated: String,
    /// 支持的游戏版本（详情面板用；空 = 站点没给）
    pub versions: Vec<String>,
    /// 支持的加载器（**站点原样的小写 facet**：兼容性判定要用它跟 `loader_facet` 比）
    pub loaders: Vec<String>,
    /// 来源（行上标来源徽标）
    pub source: ContentSource,
}

impl Hit {
    /// 命中项 DTO → 展示态（**演示数据也走这条**，于是 fixture 与产品路径逐字段同源）。
    pub(crate) fn of(hit: &ContentHit, catalog: Catalog) -> Self {
        let source = match hit.source {
            ContentSource::Modrinth => catalog.text(Key::ContentSourceModrinth),
            ContentSource::Curseforge => catalog.text(Key::ContentSourceCurseforge),
        };
        let supporting = if hit.author.is_empty() {
            source.to_string()
        } else {
            format!("{source} · {}", hit.author)
        };
        Self {
            id: hit.id.clone(),
            title: hit.title.clone(),
            summary: hit.summary.clone(),
            supporting,
            downloads: catalog.render(
                Key::ContentDetailDownloads,
                &[("count", &thousands(hit.downloads))],
            ),
            updated: catalog.render(Key::ContentDetailUpdated, &[("date", &hit.updated)]),
            versions: hit.game_versions.clone(),
            loaders: hit.loaders.clone(),
            source: hit.source,
        }
    }
}

/// 目标实例（一键装入的对象）的展示态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// 实例 id
    pub id: String,
    /// 显示名
    pub name: String,
    /// 游戏版本
    pub mc_version: String,
    /// 加载器
    pub loader: LoaderKind,
}

impl Target {
    pub(super) fn of(instance: &InstanceSummary) -> Self {
        Self {
            id: instance.id.clone(),
            name: instance.name.clone(),
            mc_version: instance.mc_version.clone(),
            loader: instance.loader.kind,
        }
    }

    /// 详情里那一行（`Survival · 1.20.4 · Fabric`）。
    pub(crate) fn label(&self) -> String {
        format!(
            "{} · {} · {}",
            self.name,
            self.mc_version,
            loader_name(self.loader)
        )
    }
}

/// 下载量加千分位（`1200000` → `1,200,000`）：raw 数字在详情面板上读不出量级。
pub(crate) fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// 首字母大写（`fabric` → `Fabric`；空串原样）。
///
/// **只在画的时候用**：`Hit::loaders` 与兼容性判定都保持站点原样的小写 facet，
/// 否则「Fabric」与 facet「fabric」一比就成了不兼容（本轮踩过）。
pub(crate) fn capitalize(raw: &str) -> String {
    let mut chars = raw.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// 加载器的**界面名**（ASCII）。
///
/// 不复用 `LoaderKind::label()`：它对原版返回「原版」——中文业务串，而本仓的界面文案在 i18n
/// 之前是英文基线（干净 CI 上没有中文字体，见 `pages/instances/data.rs` 的同一条理由）。
pub const fn loader_name(loader: LoaderKind) -> &'static str {
    match loader {
        LoaderKind::Vanilla => "Vanilla",
        LoaderKind::Fabric => "Fabric",
        LoaderKind::Quilt => "Quilt",
        LoaderKind::Forge => "Forge",
        LoaderKind::NeoForge => "NeoForge",
        LoaderKind::OptiFine => "OptiFine",
        LoaderKind::LiteLoader => "LiteLoader",
    }
}

/// 选中项的**判定事实**（版本 / 加载器 / 来源）。
///
/// 为什么另起一个结构：兼容性判据随着「宿主接通了什么」在长（M109 加了来源），
/// 每次加一条都改一次 `blockers_of` 的形参列表，而调用点有两处（产品路径与演示数据）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct HitFacts<'a> {
    /// 支持的游戏版本（空 = 站点没给）
    pub versions: &'a [String],
    /// 支持的加载器（空 = 站点没给；**站点原样的小写 facet**）
    pub loaders: &'a [String],
    /// 来源（决定「文件端点通没通」）
    pub source: ContentSource,
}

/// 装不上的原因（**一条一个原因**，可以有多条同时成立）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocker {
    /// 整包类内容不走「单文件进目录」这条路（导入管线还没落地）
    NeedsPipeline,
    /// 这个来源的**文件端点**还没接通（检索通了、拿到下载地址那一跳没通）
    SourceNotWired,
    /// 项目不支持目标实例的游戏版本
    Version {
        /// 项目支持的游戏版本（人读，最多取三个）
        needs: String,
        /// 目标实例的版本
        actual: String,
    },
    /// 项目不支持目标实例的加载器
    Loader {
        /// 目标实例的加载器
        loader: LoaderKind,
    },
    /// 没有目标实例
    NoTarget,
}

impl Blocker {
    /// 一句话原因（i18n）。
    pub(super) fn reason(&self, catalog: Catalog) -> String {
        match self {
            Self::NeedsPipeline => catalog.text(Key::ContentModpackNotReady).to_string(),
            Self::Version { needs, actual } => catalog.render(
                Key::ContentIncompatibleVersion,
                &[("version", needs), ("actual", actual)],
            ),
            Self::Loader { loader } => catalog.render(
                Key::ContentIncompatibleLoader,
                &[("loader", loader_name(*loader))],
            ),
            Self::SourceNotWired => catalog.text(Key::ContentSourceNotWired).to_string(),
            Self::NoTarget => catalog.text(Key::ContentNoInstances).to_string(),
        }
    }
}
