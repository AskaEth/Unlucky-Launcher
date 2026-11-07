//! 下载页视图模型：**presenter 快照 → 页面要画的东西**（与 [`super::super::model`] 同一套分层）。
//!
//! ## 为什么这一页也要中间层
//!
//! 页面层不认识 presenter 的 DTO（同实例页的理由）：真页面吃 `AppState` 的快照，
//! 演示数据与单测吃 [`super::demo`]，两边都转成这里的纯展示字段。
//!
//! 文案在这一层就按 [`Catalog`] 求好值（`supporting` / `detail` / `rate` 都是 `String`）：
//! iced 的文字控件**借用**内容，行内文案又是 `format!` 出来的，不先落成字段就没法进控件树
//! （`pages/frame` 的 `text_role_owned` 是同一个理由的单行版）。
//!
//! ## 快照里**没有**的东西（都登记在 `pages/mod.rs` 的偏离表里）
//!
//! | 缺什么 | 后果 | 兜法 |
//! |---|---|---|
//! | 「版本列表已加载完成」标志 | 空列表既可能是「还没拉到」也可能是「真的没有」 | [`Download::loaded`] 按「快照里有没有版本」判：没有 = 加载中 |
//! | 版本列表**失败**事件 | 拉取失败会停在加载态（不静默：页面上有说明与刷新入口） | 错误态借安装阶段的 `Failed`（结构化、可翻译） |
//! | 「当前选中」字段 | 选中态存在模型里，任何快照重建都会回到「正在安装的那个版本」 | 见 `pages/mod.rs` 偏离登记；长期要靠 presenter 加字段 |
//!
//! ## 中文串一个都不许画
//!
//! `InstallProgressView::message` / `InstallStage::label()` 都是**中文业务串**（`i18n::bridge`
//! 的规矩 2：core 的中文串只给日志和 CLI）。所以这里只搬**结构化字段**（阶段枚举、计数、字节），
//! 文案按 `Key` 现求——中文环境自然出中文，英文环境不需要任何中文字体。

use unlucky_presenter::dto::{
    human_bytes, InstallProgressView, InstallStage, VersionKind, VersionSummary,
};
use unlucky_presenter::state::AppState;

use unlucky_i18n::{Catalog, Key, Lang};

use super::options::{InstallDraft, OptionsView};

/// 取消安装之后那一句提示（M105）。
///
/// 为什么文案在页面层而不是 presenter：取消的**信号**由 host 发（`CoreEvent::InstallCancelled`），
/// 但 presenter 不依赖 `unlucky-i18n`，写死的中文会和页面语言打架（复审 P2-2 抓的就是这个：
/// 选项区是英文、取消弹窗是中文）。所以文本按 `Key` 现求，语言跟着页面走。
#[must_use]
pub fn cancelled_toast(lang: Lang) -> String {
    lang.catalog().text(Key::DownloadCancelled).to_owned()
}

mod category;

pub use category::Category;

/// 版本列表的一行（`ListItem` 的两行档：headline = 版本 id，supporting = `正式版 · 发布于 …`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRow {
    /// 版本 id（`1.21.9` / `rd-132211`）——也是「安装」指令的载荷
    pub id: String,
    /// 发布日期（ISO-8601 日期部分，快照原样）
    pub released: String,
    /// 版本类型
    pub kind: VersionKind,
    /// 本地是否已安装（行上给勾 / overline `Installed`）
    pub installed: bool,
    /// 副文案（`正式版 · 发布于 2026-09-20`；一行 = 一个 key，不拼片段）
    pub supporting: String,
}

impl VersionRow {
    pub(super) fn of(version: &VersionSummary, catalog: Catalog) -> Self {
        Self {
            id: version.id.clone(),
            released: version.released.clone(),
            kind: version.kind,
            installed: version.installed,
            supporting: catalog.render(
                Key::DownloadRowSupporting,
                &[
                    ("kind", catalog.text(kind_label_key(version.kind))),
                    ("date", version.released.as_str()),
                ],
            ),
        }
    }

    /// 这个版本属于哪个主版本桶（规则见 [`super::group`]）。
    pub fn major(&self) -> Option<super::group::MajorKey> {
        super::group::key(&self.id, self.kind)
    }
}

/// 版本类型 → 文案 key（远古版不分子类：chip 只有一档，行上再分子类等于两套语言）。
pub(super) const fn kind_label_key(kind: VersionKind) -> Key {
    match kind {
        VersionKind::Release => Key::DownloadCategoryRelease,
        VersionKind::Snapshot => Key::DownloadCategorySnapshot,
        VersionKind::Ancient(_) => Key::DownloadCategoryAncient,
    }
}

/// 一次安装的展示态（结构化字段 + 已求好的文案）。
#[derive(Debug, Clone, PartialEq)]
pub struct InstallView {
    /// 目标版本（= 选中的那一行）
    pub version: String,
    /// 阶段
    pub stage: InstallStage,
    /// 阶段词（`下载文件`）
    pub stage_label: String,
    /// 计数行（`128/512 文件`；未知时是「等第一个文件…」）
    pub detail: String,
    /// 速率 + 剩余时间（`1.2 MB/s · 剩 1m15s`；未知时 `None`）
    pub rate: Option<String>,
    /// 整体进度 0.0~1.0
    pub ratio: f32,
    /// 进度是否已知（`false` = 进度条画不确定态）
    pub known: bool,
}

impl InstallView {
    /// 快照 → 展示态。
    pub(super) fn of(progress: &InstallProgressView, catalog: Catalog) -> Self {
        let known = progress.is_known();
        let detail = if known {
            catalog.render(
                Key::DownloadProgress,
                &[
                    ("done", &progress.files_done.to_string()),
                    ("total", &progress.files_total.to_string()),
                ],
            )
        } else {
            catalog.text(Key::DownloadProgressUnknown).to_owned()
        };
        let rate = if progress.speed_bps == 0 || progress.eta_secs == 0 {
            None
        } else {
            let eta = if progress.eta_secs >= 60 {
                catalog.render(
                    Key::DownloadEtaMins,
                    &[
                        ("mins", &(progress.eta_secs / 60).to_string()),
                        ("secs", &format!("{:02}", progress.eta_secs % 60)),
                    ],
                )
            } else {
                catalog.render(
                    Key::DownloadEtaSecs,
                    &[("secs", &progress.eta_secs.to_string())],
                )
            };
            Some(catalog.render(
                Key::DownloadRate,
                &[
                    ("speed", &format!("{}/s", human_bytes(progress.speed_bps))),
                    ("eta", eta.as_str()),
                ],
            ))
        };

        Self {
            version: progress.version.clone(),
            stage: progress.stage,
            stage_label: catalog.text(stage_key(progress.stage)).to_owned(),
            detail,
            rate,
            ratio: progress.ratio(),
            known,
        }
    }

    /// 失败了（错误态要出可读文案 + 出口，不许静默）。
    pub const fn failed(&self) -> bool {
        matches!(self.stage, InstallStage::Failed)
    }

    /// 收尾了（`Done` / `Failed` / `Cancelled`）。
    pub fn terminal(&self) -> bool {
        self.stage.is_terminal()
    }

    /// **结束且没什么可说**的收尾：成功（`Done`）与取消（`Cancelled`）。
    ///
    /// 这两种终态页面没有动作可给，留着只会让安装面板停在一个点不动的状态上（复审 p2 的根因）。
    /// 失败**不在其中**：它还要给原因出口。
    pub fn finished(&self) -> bool {
        matches!(self.stage, InstallStage::Done | InstallStage::Cancelled)
    }

    /// 还在跑（「取消安装」只在这时给）。
    pub fn running(&self) -> bool {
        !self.stage.is_terminal()
    }
}

/// 阶段 → 文案 key（**不**用 `InstallStage::label()`：那是中文业务串）。
const fn stage_key(stage: InstallStage) -> Key {
    match stage {
        InstallStage::Planning => Key::DownloadStagePlanning,
        InstallStage::Downloading => Key::DownloadStageDownloading,
        InstallStage::Installing => Key::DownloadStageInstalling,
        InstallStage::Registering => Key::DownloadStageRegistering,
        InstallStage::Done => Key::DownloadStageDone,
        InstallStage::Failed => Key::DownloadStageFailed,
        InstallStage::Cancelled => Key::DownloadStageCancelled,
    }
}

/// 下载页要画的东西（**全部字段都能从快照推出来**，页面自己不留状态）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Download {
    /// 版本列表（顺序 = 快照顺序）
    pub rows: Vec<VersionRow>,
    /// 当前分类（从快照反推）
    pub category: Category,
    /// 选中的版本 id（`None` = 没选中 → 面板只给一句提示）
    pub selected: Option<String>,
    /// 安装选项区（M105）：选中了版本、又没在装的时候才有
    pub options: Option<OptionsView>,
    /// 正在跑（或刚跑完）的安装
    pub install: Option<InstallView>,
    /// 快照里有没有版本列表（`false` = 还没拉到 → 加载态）
    pub loaded: bool,
    /// 搜索词（presenter 的 `download_query` 原样；空串 = 不过滤）
    pub query: String,
    /// **跨分类**命中数：搜索时侧栏显示它，所以它不随当前分类变化
    pub hits: usize,
    /// 选中的主版本桶（`None` = 全部；第二级侧栏的选中态，纯视图态）
    pub selected_major: Option<String>,
    /// 类型栏被用户手动折叠（`true` = 64 图标条；宽度档自己也会折叠，两者取更窄的）
    pub type_rail_folded: bool,
    /// 主版本桶（第二级侧栏的数据源；由 [`Self::rows`] 聚合，所以只含**当前分类**的桶）
    pub buckets: Vec<super::group::Bucket>,
}

impl Download {
    /// 空模型 = 加载态（首帧，以及 `pages::Model::new` 的默认值）。
    pub fn loading() -> Self {
        Self::default()
    }

    /// presenter 快照 → 下载页模型（**唯一的真数据入口**）。
    ///
    /// 三个过滤条件都是「过滤」，不是「乐观回显」：
    /// - `installed_only` / `download_query` 是 presenter 自己的状态，页面不该另存一份；
    /// - `category` 是**本页的过滤条件**（参数传进来，调用方持有它）：真 host 的
    ///   `AppCommand::ListVersions(_)` 会丢掉分类参数、回全量清单（见 `host/dispatch.rs`），
    ///   所以分类**必须在页面这一侧过滤**，否则 chip 只是点亮一下、列表照旧是全量（复审 P2-1）。
    ///   过滤谓词与 chip 的选中判断是同一个 [`Category::matches`]，于是「chip 说的」与「列表画的」
    ///   永远是同一份真相，事件回来重建也不会跳回「全部」。
    ///
    /// 过滤后为空就是「空态」（`loaded` 仍为 `true`）。
    ///
    /// `draft` 是**用户还没确认的安装输入**（选中版本 + 加载器 + 加载器版本 + 实例名）。
    /// 它住在 `App`（照分类筛选的既有做法），因为 `refresh_view` 每次事件都会把整棵模型重建，
    /// 页面自己存不住任何东西。选中项还要**在当前列表里**才算数：切分类后那一行不在了，
    /// 就不该还留着一个点不动的「安装」。
    ///
    /// M107：`query` / `hits` / `buckets` 也走这条路——查询词来自快照的 `download_query`，
    /// 桶由过滤后的行聚合（第二级侧栏的数据源）。
    pub fn from_snapshot(
        state: &AppState,
        category: Category,
        draft: &InstallDraft,
        catalog: Catalog,
    ) -> Self {
        let query = state.download_query.trim().to_ascii_lowercase();
        // 过滤谓词写一次、两处用：`hits` 是**跨分类**命中数（搜索框旁边的「N 个结果」不能
        // 随当前分类变，否则用户在「正式版」里搜一个只有快照才有的串会看到 0）。
        let matches = |version: &VersionSummary| {
            (!state.installed_only || version.installed)
                && (query.is_empty() || version.id.to_ascii_lowercase().contains(&query))
        };

        let hits = state
            .versions
            .iter()
            .filter(|version| matches(version))
            .count();
        let rows = state
            .versions
            .iter()
            .filter(|version| matches(version) && category.matches(version.kind))
            .map(|version| VersionRow::of(version, catalog))
            .collect::<Vec<_>>();

        let install = state
            .install
            .as_ref()
            .map(|view| InstallView::of(view, catalog))
            // 安装**已经成功结束/被取消**就不再占用面板（复审 p2，本轮的修复点）：
            // presenter 只在 `InstallProgress(None)`（取消指令）时清 `AppState.install`，`Done`
            // 会一直留在快照里 ⇒ 面板被一个**没有动作**的终态钉死，用户装完第一个版本就再也发不出
            // 第二次安装（只能重启进程）。界面这里把「结束」当结束：面板让回列表。
            //
            // 为什么不连 `Failed` 一起清：失败态带着用户要的东西（停在哪个文件 + 去日志页），
            // 那正是 P2-4 要它一眼可辨的理由。
            .filter(|view| !view.finished());
        // 选中项：安装中就是正在装的那个；否则是用户点的那个（草稿），且必须还在当前列表里。
        let selected = install
            .as_ref()
            .map(|view| view.version.clone())
            .or_else(|| draft.selected.clone())
            .filter(|id| rows.iter().any(|row| &row.id == id));
        // 选项区与安装面板**互斥**：在装的时候面板讲进度，讲不了选项。
        let options = match &install {
            Some(_) => None,
            None => selected.as_ref().and_then(|id| {
                rows.iter()
                    .find(|row| &row.id == id)
                    .map(|row| OptionsView::of(row, state, draft, catalog))
            }),
        };

        Self {
            // 桶由**过滤后的行**聚合：第二级侧栏只列当前分类里真实出现的版本族。
            buckets: super::group::buckets(&rows),
            rows,
            // 分类不再从列表反推（那正是「chip 跳回全部」的来源）：谁选的就是谁。
            // [`Category::of`] 只留给「不经过用户选择」的入口（演示数据的首帧）。
            category,
            selected,
            options,
            install,
            loaded: !state.versions.is_empty(),
            // 搜索词与命中数都来自快照：页面不持有查询（见模块文档「为什么搜索也走快照」）。
            query: state.download_query.trim().to_owned(),
            hits,
            // 主版本桶的选中是**纯视图态**（同 `selected`）：快照重建会回到「全部」。
            selected_major: None,
            // 折叠也是纯视图态；搜索重建时由 `rebuild_after_query` 前后各搬一次。
            type_rail_folded: false,
        }
    }

    /// 选中的那一行（安装面板上的版本名）。
    pub fn selected_row(&self) -> Option<&VersionRow> {
        let id = self.selected.as_deref()?;
        self.rows.iter().find(|row| row.id == id)
    }

    /// 加载态：快照里还没有版本列表。
    pub const fn is_loading(&self) -> bool {
        !self.loaded
    }

    /// 空态：列表拉到了，但过滤之后一行都不剩。
    pub fn is_empty(&self) -> bool {
        self.loaded && self.rows.is_empty()
    }

    /// 有搜索词（空串不算：用户在框里敲了空格不该触发「没有匹配」）。
    pub fn has_query(&self) -> bool {
        !self.query.is_empty()
    }

    /// **整份清单**里一个都不匹配（跨分类也没有）⇒ 画「没有匹配」而不是白屏。
    pub fn no_match(&self) -> bool {
        self.loaded && self.has_query() && self.hits == 0
    }

    /// 有命中、但**不在当前分类**里 ⇒ 画「N 个版本在别的类型下」，并给一个「看全部」的出口。
    ///
    /// 这条与 [`Self::no_match`] 分开，是因为两者该说的话完全不同：一个是「真的没有」，
    /// 一个是「有，但你站错了分类」——后者让用户多点一次 tab 就白跑一趟。
    pub fn only_elsewhere(&self) -> bool {
        self.loaded && self.rows.is_empty() && self.hits > 0
    }
}

#[cfg(test)]
mod tests;
