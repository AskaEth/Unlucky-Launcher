//! 「只重试这些」面板的视图模型（M191）。
//!
//! ## 它画的是什么
//!
//! 安装失败之后，界面要说清「哪几项没装上、分别是哪种没装上」：
//!
//! ```text
//! 安装未完成 —— 3 项没装上
//! 不兼容所选版本/加载器
//!   P7dR8mSH                       ← 拿不到名字时只画 id（合理兜底）
//! 取不到（网络 / 项目不存在）
//!   Sodium (AANobbMI)              ← 根项目：名字来自请求标题
//!   fabric-api-0.92.2.jar (1eAoo2KR) ← 下载失败：名字来自文件名
//! [只重试这 3 个]  [关闭]
//! ```
//!
//! 每行是 `InstallFailureView::line()` 的结果（名字优先、id 次要）。名字是**语言中立**的站点
//! 标题或文件名，不是 core 的中文散文——那条线（i18n R2）踩了就回不去。
//!
//! ## 三条判据（都落在 [`RetryPanel::of`]，纯函数，用例直接钉）
//!
//! 1. **全成功不渲染**：`retry` 名单空 ⇒ `None` ⇒ 视图里连按钮都不存在（不是置灰）；
//! 2. **两档不混**：分组按 [`InstallFailureKind`] 分开，两档各有自己的标题 key；
//! 3. **只列名单里的**：条目原样来自 DTO（`presenter` 从 `DependencyPlan::retry_request()`
//!    的同一份失败名单产出），页面不自己挑、也不补「顺便重装根」。
//!
//! ## 为什么这里不拼文案
//!
//! 分组标题是 [`Key`]（i18n），条目是项目键（站点数据，不是文案）。拼一句带量的中文句子
//! 会把语序写死——英文与中文的量词位置不同（`Retry just these 3` / `只重试这 3 个`）。

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{
    ContentInstallRequest, ContentSource, InstallFailureKind, InstallFailureView,
    InstallProgressView, InstallStage, LoaderKind,
};

/// 面板上的一个档位分组。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryGroup {
    /// 哪一档（顺序固定，见 [`InstallFailureKind::ALL`]）
    pub kind: InstallFailureKind,
    /// 这一档的标题 key（不硬编码中文）
    pub title: Key,
    /// 这一档要画的每一行（`名字 (id)`；拿不到名字时就只有 id，顺序 = 计划里的顺序）
    pub items: Vec<String>,
}

/// 「只重试这些」面板（`None` = 不渲染：没有失败项、或已经全成功）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPanel {
    /// 失败的那次装入的目标实例（重试只对这个实例生效）
    pub instance: String,
    /// 标题（已按词表渲染：带条数）
    pub title: String,
    /// 主按钮文字（已按词表渲染：`只重试这 N 个`）
    ///
    /// 存在这里而不是画的时候现拼：`Button::new` 要的是 `&'a str`，而词表渲染出来的是
    /// `String`——存进模型（与 `Hit` / `VersionRow.line` 同一手法）就不必为了一个标签
    /// 去改组件库的入参类型。
    pub action: String,
    /// 按档分组（空组不出现）
    pub groups: Vec<RetryGroup>,
    /// 失败项总数（标题与按钮都用这个数）
    pub total: usize,
}

impl RetryPanel {
    /// 进度快照 → 面板（**唯一**判据；`None` = 视图里不该出现这块）。
    ///
    /// 只认 `Failed` 这一态：`Done` 没有可重试的东西，`Cancelled` 是用户自己停的
    /// （那条路上根本没有失败名单），`Planning`/`Downloading` 还在跑。
    #[must_use]
    pub fn of(progress: &InstallProgressView, catalog: Catalog) -> Option<Self> {
        if progress.stage != InstallStage::Failed || !progress.has_retry() {
            return None;
        }
        let total = progress.retry.len();
        let count = total.to_string();
        let groups = InstallFailureKind::ALL
            .into_iter()
            .map(|kind| RetryGroup {
                kind,
                title: title_of(kind),
                items: progress
                    .retry_of(kind)
                    .map(InstallFailureView::line)
                    .collect(),
            })
            .filter(|group| !group.items.is_empty())
            .collect();
        Some(Self {
            instance: progress.instance.clone(),
            title: catalog.render(Key::ContentInstallFailedTitle, &[("count", count.as_str())]),
            action: catalog.render(Key::ContentInstallRetryOnly, &[("count", count.as_str())]),
            groups,
            total,
        })
    }

    /// 两档都在吗（「分组」这件事本身的判据：都出现时界面上是两段，不是一段）。
    #[must_use]
    pub fn has_both_tiers(&self) -> bool {
        self.groups.len() == 2
    }

    /// 某一档的条目（用例断言用）。
    #[must_use]
    pub fn items_of(&self, kind: InstallFailureKind) -> Option<&[String]> {
        self.groups
            .iter()
            .find(|group| group.kind == kind)
            .map(|group| group.items.as_slice())
    }
}

/// 档位 → 标题 key（**穷尽 match**：合并两档会少一条文案，用例会红）。
#[must_use]
pub const fn title_of(kind: InstallFailureKind) -> Key {
    match kind {
        InstallFailureKind::Incompatible => Key::ContentInstallRetryIncompatible,
        InstallFailureKind::Unreachable => Key::ContentInstallRetryUnreachable,
    }
}

impl super::Content {
    /// 「只重试这些」那一次请求（M191；`None` = 没有可重试的面板 ⇒ 这个动作不存在）。
    ///
    /// 载荷里 `retry: true` 是**唯一**的语义开关：host 收到它就不解根项目，而是拿上一轮的计划
    /// 调 `DependencyPlan::retry_request()`（只请求失败的那几个项目）。其余字段照旧填，
    /// 是因为 `ContentInstallRequest` 的判据仍要能读（`project` / `title` 也用于日志）；
    /// 这些字段在重试路径上**不被 host 读取**——过滤条件与落盘目录都来自那份计划。
    ///
    /// 实例取**失败那次的**实例，不是页面上此刻选中的那个：用户可能已经换了目标实例。
    pub fn retry_install_request(&self) -> Option<ContentInstallRequest> {
        let retry = self.retry.as_ref()?;
        let hit = self.selected_hit();
        let target = self
            .targets
            .iter()
            .find(|target| target.id == retry.instance);
        Some(ContentInstallRequest {
            project: hit.map(|hit| hit.id.clone()).unwrap_or_default(),
            title: hit.map(|hit| hit.title.clone()).unwrap_or_default(),
            kind: self.kind,
            source: hit.map_or(ContentSource::Modrinth, |hit| hit.source),
            instance: retry.instance.clone(),
            game_version: target.map_or_else(String::new, |target| target.mc_version.clone()),
            loader: target.map_or(LoaderKind::Vanilla, |target| target.loader),
            retry: true,
        })
    }
}
