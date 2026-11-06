//! 安装选项的**决策**（无 iced）：选哪个加载器、它配不配这个原版版本、确认键能不能按。
//!
//! ## 为什么选项要有自己的模型层
//!
//! M105 之前「点安装」直接开工，于是用户装不了带加载器的版本（core 早就会装四家加载器，
//! 只是 UI 没接）。补上这一步之后，页面要回答的问题从一个变成四个：
//!
//! | 问题 | 判据 |
//! |---|---|
//! | 这个加载器配这个原版版本吗 | [`Loader::supports`]（静态规则，**不联网**） |
//! | 加载器版本有哪些 | presenter 的 `LoaderVersions` 快照（`state.loader_versions_for`） |
//! | 用哪个加载器版本 | 用户选的；没选就取列表里最新的一条（core 的 `resolve(kind, mc, None)` 也是这个口径） |
//! | 现在能不能点「安装」 | [`OptionsView::can_confirm`]（不兼容 / 列表没到位 / 已装 都按不下去） |
//!
//! ## 兼容性为什么是静态规则而不是问网络
//!
//! 「选完才失败」是用户最讨厌的一种交互。凡是**官方根本没有**的组合（NeoForge × 1.12.2、
//! 远古版 × 任何加载器），必须在选中那一刻就说清，不能等点了安装再回一个 404。
//! 反过来说：**能**装但列表拉不到（网络问题）属于「取不到」，由 [`LoaderVersionsView::Failed`]
//! 显示具名原因并禁掉确认——两条都不会让用户「选完才失败」。
//!
//! 规则来源（不猜：都是 core 安装管线自己的判据）：
//! - Fabric/Quilt 的 loader 列表从 `1.14` 起才有（core 走 fabric meta，1.13 及以前没有）；
//! - NeoForge 的第一个版本是 `1.20.1`（`maven::NEOFORGE_FORGE_LAYOUT_MC`）；
//! - Forge 从 `1.1` 起有正式构建；远古版（`rd-*` / `a*` / `b*`）与预览版在 maven 表里没有条目。

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{LoaderKind, VersionKind};
use unlucky_presenter::state::AppState;

use super::super::model::VersionRow;

/// 加载器版本下拉此刻的内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoaderVersionsView {
    /// 还没查过 / 正在查（页面显示「正在获取…」，确认键按下不去）
    Loading,
    /// 查到了（**最新在前**，与 core 的 `loader_versions` 同序）
    Ready(Vec<String>),
    /// 查不到：人读原因（页面显示它并禁掉确认，不静默显示成空列表）
    Failed(String),
}

/// 页面侧的加载器词表：只列 GUI 真能装的五家（与 `dto::LoaderKind::wizard_choices` 同集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Loader {
    /// 原版（不装加载器）
    #[default]
    Vanilla,
    /// Forge
    Forge,
    /// NeoForge
    NeoForge,
    /// Fabric
    Fabric,
    /// Quilt
    Quilt,
}

impl Loader {
    /// 展示顺序（任务书的口径：原版 / Forge / NeoForge / Fabric / Quilt）。
    pub const ALL: [Loader; 5] = [
        Loader::Vanilla,
        Loader::Forge,
        Loader::NeoForge,
        Loader::Fabric,
        Loader::Quilt,
    ];

    /// 对应的 presenter 词表（派 `AppCommand` 时用）。
    pub const fn kind(self) -> LoaderKind {
        match self {
            Loader::Vanilla => LoaderKind::Vanilla,
            Loader::Forge => LoaderKind::Forge,
            Loader::NeoForge => LoaderKind::NeoForge,
            Loader::Fabric => LoaderKind::Fabric,
            Loader::Quilt => LoaderKind::Quilt,
        }
    }

    /// 从 presenter 词表回来（`OptiFine` / `LiteLoader` 不是本页选项）。
    pub const fn of(kind: LoaderKind) -> Option<Self> {
        match kind {
            LoaderKind::Vanilla => Some(Loader::Vanilla),
            LoaderKind::Forge => Some(Loader::Forge),
            LoaderKind::NeoForge => Some(Loader::NeoForge),
            LoaderKind::Fabric => Some(Loader::Fabric),
            LoaderKind::Quilt => Some(Loader::Quilt),
            LoaderKind::OptiFine | LoaderKind::LiteLoader => None,
        }
    }

    /// chip 文案。
    ///
    /// 四家加载器的名字是**专有名词**（Fabric/Forge/NeoForge/Quilt 官方各语言都写原样），
    /// 所以只有「原版」走 i18n；硬给品牌名建一条 en==zh 的 key 会被
    /// `catalog::tests::chinese_catalog_is_actually_chinese` 判为「未翻译」。
    pub fn label(self, catalog: Catalog) -> &'static str {
        match self {
            Loader::Vanilla => catalog.text(Key::LoaderVanilla),
            Loader::Forge => "Forge",
            Loader::NeoForge => "NeoForge",
            Loader::Fabric => "Fabric",
            Loader::Quilt => "Quilt",
        }
    }

    /// 这个加载器配不配这个原版版本（静态规则，见模块文档的判据表）。
    ///
    /// 参数是 `(版本 id, 版本类型)` 而不是一整行：app 层在**派查询之前**也要问同一个问题
    /// （静态就不支持的组合不该去联网问，否则会白白换来一条「取不到」的 Toast）。
    pub fn supports(self, id: &str, kind: VersionKind) -> Result<(), Incompat> {
        if self == Loader::Vanilla {
            return Ok(());
        }
        // 远古版：四家都没有构建（maven 表 / fabric meta 都没有这些版本）。
        if kind.is_ancient() {
            return Err(Incompat::Unsupported);
        }
        let rank = release_rank(id);
        match self {
            Loader::Vanilla => Ok(()),
            // 预览版：Fabric 系的确会为快照发 loader，交给列表回答（拉不到就显示原因）；
            // maven 系（Forge/NeoForge）没有快照条目，直接拒。
            Loader::Fabric | Loader::Quilt => match rank {
                Some(version) if version >= (1, 14, 0) => Ok(()),
                Some(_) => Err(Incompat::NeedsAtLeast("1.14")),
                None if kind == VersionKind::Snapshot => Ok(()),
                None => Err(Incompat::Unsupported),
            },
            Loader::Forge => match rank {
                Some(_) => Ok(()),
                None => Err(Incompat::Unsupported),
            },
            Loader::NeoForge => match rank {
                Some(version) if version >= (1, 20, 1) => Ok(()),
                Some(_) => Err(Incompat::NeedsAtLeast("1.20.1")),
                None => Err(Incompat::Unsupported),
            },
        }
    }
}

/// 不支持的原因（两种，页面各一句文案）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incompat {
    /// 官方没有这个组合的构建
    Unsupported,
    /// 需要某个最低原版版本
    NeedsAtLeast(&'static str),
}

/// `1.20.1` → `(1, 20, 1)`；预览版 / 远古版 / 乱七八糟的串 → `None`。
///
/// 只要有一段落不是纯数字就给 `None`：宁可保守地拒绝一个自定义版本号，也不把
/// `1.20.1-pre3` 当成 `1.20.1` 放过去（那会装出加载器找不到父版本）。
pub fn release_rank(id: &str) -> Option<(u64, u64, u64)> {
    let mut pieces = id.trim().split('.');
    let major: u64 = pieces.next()?.parse().ok()?;
    let minor: u64 = pieces.next()?.parse().ok()?;
    let patch: u64 = match pieces.next() {
        Some(patch) => patch.parse().ok()?,
        None => 0,
    };
    if pieces.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// 用户还没确认的安装输入（**app 持有**，不是 presenter 快照的一部分）。
///
/// 为什么住在 app 而不是 presenter：这是「用户正在填、还没提交」的草稿，
/// 和下载页的分类筛选（`App::view.download.category` 的来源）是同一层的东西；
/// 塞进 presenter 快照会让后端视图模型混进 UI 草稿。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstallDraft {
    /// 选中的版本 id（点行得到；点别的版本会清掉加载器版本，见 `app/update/download.rs`）
    pub selected: Option<String>,
    /// 选中的加载器
    pub loader: Loader,
    /// 用户指定的加载器版本（空 = 用列表里最新的那条）
    pub loader_version: String,
    /// 实例名（空 = 用版本 id）
    pub instance_name: String,
}

/// 选项区要画的东西（文案都已按 `Catalog` 求好值）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsView {
    /// 目标版本 id
    pub version: String,
    /// 当前选中的加载器
    pub loader: Loader,
    /// 加载器版本下拉的内容（原版恒为 [`LoaderVersionsView::Ready`] 空表）
    pub versions: LoaderVersionsView,
    /// **实际会用**的加载器版本（空 = 原版）
    pub loader_version: String,
    /// 实例名输入框的现值
    pub instance_name: String,
    /// 不兼容 / 取不到的原因（`Some` ⇒ 确认键按下不去）
    pub problem: Option<String>,
    /// 本地已装（确认键给「已安装」禁用态）
    pub installed: bool,
    /// 现在能不能点「安装」
    pub can_confirm: bool,
}

impl OptionsView {
    /// 选中行 + 快照 + 草稿 → 选项区的样子。
    pub fn of(row: &VersionRow, state: &AppState, draft: &InstallDraft, catalog: Catalog) -> Self {
        let loader = draft.loader;
        let incompat = loader.supports(&row.id, row.kind);
        // 静态就不支持的组合**不去查列表**（查了也只会换来一条「取不到」的假原因）：
        // 空表 + 上面那条「没有适配构建」就是说清这件事的那一句话。
        let versions = if loader == Loader::Vanilla || incompat.is_err() {
            LoaderVersionsView::Ready(Vec::new())
        } else {
            match state.loader_versions_for(loader.kind(), &row.id) {
                None => LoaderVersionsView::Loading,
                Some(Ok(items)) => LoaderVersionsView::Ready(items.clone()),
                Some(Err(reason)) => LoaderVersionsView::Failed(reason.clone()),
            }
        };
        // 用户选过就用它；没选过取列表里最新的一条（core 的默认口径也是「最新」）。
        let loader_version = if loader == Loader::Vanilla {
            String::new()
        } else if !draft.loader_version.trim().is_empty() {
            draft.loader_version.clone()
        } else {
            match &versions {
                LoaderVersionsView::Ready(items) => items.first().cloned().unwrap_or_default(),
                _ => String::new(),
            }
        };

        let problem = match (incompat, &versions) {
            (Err(Incompat::Unsupported), _) => Some(catalog.render(
                Key::DownloadOptionsIncompatible,
                &[
                    ("loader", loader.label(catalog)),
                    ("version", row.id.as_str()),
                ],
            )),
            (Err(Incompat::NeedsAtLeast(min)), _) => Some(catalog.render(
                Key::DownloadOptionsNeedsAtLeast,
                &[("loader", loader.label(catalog)), ("min", min)],
            )),
            (Ok(()), LoaderVersionsView::Failed(reason)) => Some(catalog.render(
                Key::DownloadOptionsLoaderVersionsFailed,
                &[("reason", reason.as_str())],
            )),
            (Ok(()), LoaderVersionsView::Ready(items))
                if loader != Loader::Vanilla && items.is_empty() =>
            {
                Some(catalog.render(
                    Key::DownloadOptionsLoaderVersionsEmpty,
                    &[("version", row.id.as_str())],
                ))
            }
            (Ok(()), _) => None,
        };

        // 四条同时成立才按得下去：不兼容没有、列表真的就位（原版不需要列表）、没装过。
        // 「列表没到位」既包括 `Loading` 也包括 `Failed` 与空表——它们都已经在 `problem` 里说了原因。
        let ready = loader == Loader::Vanilla
            || matches!(&versions, LoaderVersionsView::Ready(items) if !items.is_empty());
        let can_confirm = problem.is_none() && ready && !row.installed;

        Self {
            version: row.id.clone(),
            loader,
            versions,
            loader_version,
            instance_name: draft.instance_name.clone(),
            problem,
            installed: row.installed,
            can_confirm,
        }
    }

    /// 确认安装要提交的加载器版本（`None` = 让 core 用官方默认，只在原版路径出现）。
    pub fn loader_version_arg(&self) -> Option<&str> {
        (!self.loader_version.is_empty()).then_some(self.loader_version.as_str())
    }
}
