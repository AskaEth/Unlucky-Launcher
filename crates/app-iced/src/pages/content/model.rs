//! 内容库页的视图模型：**presenter 快照 + 页面选中项 → 要画的东西**。
//!
//! ## 分层（与实例页 / 下载页同一条）
//!
//! | 数据源 | 谁转 | 用途 |
//! |---|---|---|
//! | `AppState::content`（检索条件 + 结果 + 请求态） | [`Content::from_snapshot`] | 产品运行时的真页面 |
//! | [`super::demo`] 的固定夹具 | 同上 | `pages_demo` 出图与单测 |
//!
//! ## 「点之前就知道装不装得上」（任务书第 3 条）
//!
//! 兼容性判定在 [`Content::blockers`]：**纯函数**，输入是「选中的项目 + 目标实例」，
//! 输出是 [`Blocker`] 列表。视图拿它决定按钮 `disabled` 与旁边那几句原因——于是「点完才报错」
//! 在结构上不可能发生（按钮根本发不出消息）。
//!
//! 两条判据各自独立、**都能同时成立**（版本对不上 + 加载器也不对）：把两条合成一条会在
//! 「都不兼容」时只说一半原因。
//!
//! ## 为什么要 `PageState`（页面自留的那点状态）
//!
//! `kind` / `selected` / `target` / `draft` 都是**页面内**的选择：检索条件与结果在 presenter
//! （`AppState::content`），这四样是「用户在这一屏上点了什么」，重建快照不该把它们冲掉。
//! 它们的所有者是 `app/state.rs` 的 `App::content`，本文件只借用。

use unlucky_presenter::dto::{
    loader_facet, ContentKind, ContentSort, ContentSource, ContentState, LoaderKind,
};
use unlucky_presenter::state::AppState;

use unlucky_i18n::{Catalog, Key};

use crate::pages::download::Download;

mod items;

pub(crate) use items::HitFacts;
pub(crate) use items::{capitalize, thousands};
pub use items::{loader_name, Blocker, Hit, PageState, Status, Target, VersionRow};

/// 内容库页要画的东西。
#[derive(Debug, Clone, PartialEq)]
pub struct Content {
    /// 当前内容类型（左栏四条侧栏的选中项）
    pub kind: ContentKind,
    /// 当前检索条件（筛选条 + 分页游标）
    pub query: ContentQueryView,
    /// 命中项
    pub hits: Vec<Hit>,
    /// 选中项
    pub selected: Option<String>,
    /// 当前状态
    pub status: Status,
    /// 命中总数（`0` = 站点没给）
    pub total: u32,
    /// 还有下一页
    pub has_more: bool,
    /// CurseForge 源可用（缺 key 时筛选条里那条禁用并说明）
    pub curseforge: bool,
    /// 目标实例候选
    pub targets: Vec<Target>,
    /// 选中的目标实例
    pub target: Option<String>,
    /// 选中目标实例的可读标签（详情面板「装入到 …」）
    pub target_label: Option<String>,
    /// 装入不上的原因（空 = 可以装）
    pub blockers: Vec<Blocker>,
    /// 安装面板的视图模型（**复用下载页那块**：`pages::download::install_panel`）
    ///
    /// 存整个 `Download` 而不是只存一帧视图：面板的入参就是 `&Download`，而 iced 的控件树
    /// 借用它 ⇒ 被借的东西必须活得比这一帧长（局部临时值不行）。
    pub panel: Option<Download>,
    /// 游戏版本筛选的候选（来自实例；去重、保持实例顺序）
    pub versions: Vec<String>,
    /// 关键词草稿
    pub draft: String,
    /// 导入/导出区（M113）：入口 + 进度 + 结果 + 导出预检
    pub pack: super::packui::Pack,
    /// 选中项目的可用版本（M162：详情通道；空 = 还没拉到 / 站点没给）
    pub available_versions: Vec<VersionRow>,
    /// 版本区的一句话说明（正在取 / 取不到：原因；`None` = 不必说）
    pub versions_note: Option<String>,
}

/// 筛选条的展示态（数字/枚举都在这里，页面不再从 `ContentQuery` 里挖字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentQueryView {
    /// 来源
    pub source: ContentSource,
    /// 排序
    pub sort: ContentSort,
    /// 游戏版本过滤
    pub version: Option<String>,
    /// 加载器过滤
    pub loader: Option<LoaderKind>,
    /// 已提交的关键词（输入框显示的是 `draft`）
    pub text: String,
}

impl Content {
    /// presenter 快照 + 页面选中项 → 视图模型（**唯一的真数据入口**）。
    pub fn from_snapshot(state: &AppState, page: &PageState, catalog: Catalog) -> Self {
        let content: &ContentState = &state.content;
        let hits: Vec<Hit> = content
            .results
            .as_ref()
            .map(|results| {
                results
                    .hits
                    .iter()
                    .map(|hit| Hit::of(hit, catalog))
                    .collect()
            })
            .unwrap_or_default();

        let status = status_of(content, &hits);
        let targets: Vec<Target> = state.instances.iter().map(Target::of).collect();
        let target = page
            .target
            .clone()
            .filter(|id| targets.iter().any(|target| &target.id == id))
            .or_else(|| targets.first().map(|target| target.id.clone()));
        let target_label = target
            .as_deref()
            .and_then(|id| targets.iter().find(|item| item.id == id))
            .map(Target::label);

        let mut versions: Vec<String> = Vec::new();
        for instance in &targets {
            if !versions.iter().any(|item| item == &instance.mc_version) {
                versions.push(instance.mc_version.clone());
            }
        }

        let selected_hit = page.selected.as_deref().and_then(|id| {
            content
                .results
                .as_ref()?
                .hits
                .iter()
                .find(|hit| hit.id == id)
        });
        let blockers = blockers_of(
            selected_hit.map(|hit| HitFacts {
                versions: hit.game_versions.as_slice(),
                loaders: hit.loaders.as_slice(),
                source: hit.source,
            }),
            page.kind,
            &targets,
            target.as_deref(),
        );

        // M113: import/export —— 进度/报告/预检在 presenter（`AppState.pack`），
        // 页内选择在 `page.pack`。`targets` 下面会被移动进结构体，所以这里先借用它建视图模型。
        let pack = super::packui::Pack::of(
            &state.pack.import,
            &state.pack.export,
            &page.pack,
            &targets,
            catalog,
        );

        let (available_versions, versions_note) =
            detail_view(content, page.selected.as_deref(), catalog);

        Self {
            kind: page.kind,
            query: ContentQueryView {
                source: content.query.source,
                sort: content.query.sort,
                version: content.query.game_version.clone(),
                loader: content.query.loader,
                text: content.query.text.clone(),
            },
            hits,
            selected: page.selected.clone(),
            status,
            total: content.results.as_ref().map_or(0, |results| results.total),
            // 读「最近一页是否装满」（不是累计条数）：见 `ContentState::last_page_filled` 的文档
            has_more: content.last_page_filled && !content.pending,
            curseforge: content
                .results
                .as_ref()
                .is_some_and(|results| results.curseforge),
            targets,
            target: target.clone(),
            target_label,
            blockers,
            panel: content_install(state, catalog),
            versions,
            draft: page.draft.clone(),
            pack,
            available_versions,
            versions_note,
        }
    }

    /// 选中项在命中列表里的那一行（详情面板读它）。
    pub fn selected_hit(&self) -> Option<&Hit> {
        let id = self.selected.as_deref()?;
        self.hits.iter().find(|hit| hit.id == id)
    }

    /// 「装入」按钮此刻能不能按（`false` 时按钮禁用，`blockers()` 给原因）。
    pub fn can_install(&self) -> bool {
        self.selected_hit().is_some() && self.blockers.is_empty()
    }

    /// 原因的人读文案（顺序 = 判定顺序；空 = 没有原因）。
    pub fn blocker_reasons(&self, catalog: Catalog) -> Vec<String> {
        self.blockers
            .iter()
            .map(|blocker| blocker.reason(catalog))
            .collect()
    }

    /// 加载中（首屏 / 换条件）。
    pub const fn is_loading(&self) -> bool {
        matches!(self.status, Status::Loading)
    }
}

/// 快照 → 四态。判据只有三个字段（请求在飞 / 失败原因 / 命中项），没有第四份真相。
fn status_of(content: &ContentState, hits: &[Hit]) -> Status {
    if let Some(failure) = &content.failed {
        // M162：没联网单独一态（空列表不能冒充「没有结果」）
        return if failure.is_offline() {
            Status::Offline(failure.detail.clone())
        } else {
            Status::Failed(failure.detail.clone())
        };
    }
    if content.pending || content.results.is_none() {
        return Status::Loading;
    }
    if hits.is_empty() {
        Status::Empty
    } else {
        Status::Ready
    }
}

/// 安装面板（**只借用下载页那块面板**，不另写一套画法）。
///
/// 转换与「成功/取消这类终态不占面板」的判断都在 `pages::download::install_panel_model` 里
/// （M105 起 `InstallView` 不再从那个模块导出）：内容库只拿 `Option<Download>`。
fn content_install(state: &AppState, catalog: Catalog) -> Option<Download> {
    state
        .install
        .as_ref()
        .and_then(|progress| crate::pages::download::install_panel_model(progress, catalog))
}

/// 选中项目的版本列表 + 一句说明（详情通道；迟到结果按项目 id 丢）。
///
/// 三条优先级：**有结果就画结果** → 在飞就画「正在取」 → 失败才画原因。选中项变了而结果是
/// 上一个项目的，一律当「没有」（`detail.project != selected`）——否则会画着 A 的版本列表
/// 却高亮着 B。
fn detail_view(
    content: &ContentState,
    selected: Option<&str>,
    catalog: Catalog,
) -> (Vec<VersionRow>, Option<String>) {
    let Some(selected) = selected else {
        return (Vec::new(), None);
    };
    if let Some(detail) = content.detail.as_ref().filter(|d| d.project == selected) {
        let rows = detail
            .versions
            .iter()
            .map(|version| VersionRow {
                number: version.number.clone(),
                kind: version.kind.clone(),
                file_name: version.file_name.clone(),
                line: catalog.render(
                    Key::ContentDetailVersionLine,
                    &[
                        ("number", version.number.as_str()),
                        ("kind", capitalize(&version.kind).as_str()),
                        ("date", version.published.as_str()),
                    ],
                ),
            })
            .collect();
        return (rows, None);
    }
    if content.detail_pending {
        return (
            Vec::new(),
            Some(catalog.text(Key::ContentDetailFetching).to_string()),
        );
    }
    match &content.detail_failed {
        Some(failure) => (
            Vec::new(),
            Some(catalog.render(
                Key::ContentDetailFetchFailed,
                &[("reason", failure.detail.as_str())],
            )),
        ),
        None => (Vec::new(), None),
    }
}

/// 兼容性判定（**纯函数**，单测直接钉它；见模块文档第 2 节）。
///
/// `hit` 是「选中项的两个支持列表」：`None` = 没选中，`Some((版本, 加载器))` = 选中了。
/// **空列表 ≠ 不兼容**：站点没给数据时按「未知」处理，不许把「不知道」画成「不支持」。
pub(super) fn blockers_of(
    hit: Option<HitFacts<'_>>,
    kind: ContentKind,
    targets: &[Target],
    target: Option<&str>,
) -> Vec<Blocker> {
    if targets.is_empty() {
        return vec![Blocker::NoTarget];
    }
    let Some(target) = target.and_then(|id| targets.iter().find(|item| item.id == id)) else {
        return vec![Blocker::NoTarget];
    };
    let mut blockers = Vec::new();
    // 整合包：不是「兼容不兼容」的问题，是这条路根本没有（导入管线未落地）
    if kind.dir().is_none() {
        blockers.push(Blocker::NeedsPipeline);
    }
    let Some(facts) = hit else {
        return blockers;
    };
    let (versions, loaders) = (facts.versions, facts.loaders);
    // 来源的文件端点没通：**必须在点之前**说清楚。放行等于「点完必然失败」（复审 P1-1）。
    if facts.source == ContentSource::Curseforge {
        blockers.push(Blocker::SourceNotWired);
    }
    if !versions.is_empty() && !versions.iter().any(|item| item == &target.mc_version) {
        blockers.push(Blocker::Version {
            needs: join_or_unknown(versions),
            actual: target.mc_version.clone(),
        });
    }
    let facet = loader_facet(target.loader);
    if let Some(facet) = facet {
        if !loaders.is_empty() && !loaders.iter().any(|item| item == facet) {
            blockers.push(Blocker::Loader {
                loader: target.loader,
            });
        }
    }
    blockers
}

/// 版本列表 → 一行人读串（最多三个，其余用 `…`；空 = 「未知」）。
fn join_or_unknown(versions: &[String]) -> String {
    if versions.is_empty() {
        return "?".to_string();
    }
    let shown = versions.iter().take(3).cloned().collect::<Vec<_>>();
    if versions.len() > shown.len() {
        format!("{}…", shown.join(", "))
    } else {
        shown.join(", ")
    }
}

#[cfg(test)]
#[path = "model/tests.rs"]
mod tests;
