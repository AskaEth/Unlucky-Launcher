//! 实例详情页的 **MODS 区**（M224 · MODTOGGLE）：本地 mod 列表 + 启停开关。
//!
//! # 位置与形状
//!
//! 挂在详情列里、元信息网格之后、启动参数卡**之前**（`pages/instances/detail.rs` 的 `detail_column`），
//! 动作行仍在最后。与启动参数区同一套卡片口径：`frame::card` + 一个 `LabelMedium` 小标 + 内容。
//!
//! **为什么在启动参数之前**：详情列整列是一个滚动区，而启动参数卡本身就有近一屏高。MODS 排在它
//! 后面时，1180×1000 下 MODS 区整块落在折叠线以下、根本看不见（M224 出图实测踩到过）。
//!
//! # 为什么不自己再套一层滚动区
//!
//! 详情列**整列**已经是滚动区（M157 r2 修的「Max memory 以下够不着」）。这里再套一个滚动条
//! 会出现「滚动条里的滚动条」——1180×1000 下两个条都认得出来，用户不知道该拖哪一个。
//! 列表多长就排多长，交给外层那一条。
//!
//! # 开关三档
//!
//! | 状态 | 开关 | 为什么 |
//! |---|---|---|
//! | 正常 | 可点（发 `PageMessage::SetModEnabled`） | — |
//! | 游戏在跑 | 禁用（不带消息） | Windows 上进程正持有 jar，rename 必然失败（host 侧也挡了一层） |
//! | 请求在飞 | 禁用 | 同一文件连点两次会发出两条 rename，第二条必然报「文件不在」 |
//!
//! **不做乐观回显**：开关画的是快照里的 `enabled`，点完等 host 回一份新快照才翻面。
//! 与启动/停止同一条纪律（`app/update/instances.rs` 的 Save 分支）。
//!
//! # 重复与前置（M312 · MODDUP）
//!
//! 快照里多了一段 `ModProblemView`（重复 / 必需缺失 / 无法判定 / 可选提示），本模块把它变成
//! **几句已经拼好的话**（[`Problems`]），画法在 `mods::rows`：
//!
//! | 报告 | 画成 | 可点的东西 |
//! |---|---|---|
//! | 同一 modid 装了多份 | 一族一条：`Sodium · 建议保留 x.jar（0.5.11）` + `另有 N 份` | 列表里那一行的**启停开关**（M224 那个，不新增入口）；重复行的副文案挂「重复 · 建议保留 / 建议禁用」 |
//! | 必需前置没装 | 一条一句：id + `由 x.jar 声明` | 「去模组库找」→ `Route::Content`（内容库页就是装 mod 的地方） |
//! | 装了但版本判不了 | 一句计数（**不是问题**） | 无 |
//! | 可选前置没装 | 一句计数（作者说"推荐"） | 无 |
//! | 元数据读不出的文件 | 一句计数（这份报告少看了几个文件） | 无 |
//!
//! **不删任何文件**：报告只给建议，禁用是用户点开关之后的事（删用户的文件不可逆）。
//! **不阻塞启动**：这些是诊断信息，启动按钮与它无关（缺前置也能进游戏看）。
//!
//! ## 为什么新 key 直接读 `Catalog` 而不是走 `Texts`
//!
//! `Texts` 的取词方法住在 `pages/instances/text.rs`（不在 M312 的改动范围）。而死 key 门禁
//! （`tests/i18n.rs`）扫的是**目录派生的本页那一族**（`pages/instances.rs` + `pages/instances/**`
//! 的非测试文件，见 `tests/i18n/sources.rs`；M338 之前那张手写清单已经删了），本文件就在族里、自动参加。
//! 门禁要求 `Key::InstancesMods*` 字面出现在这些文件里，所以 M312 这一族 key 在本文件用
//! `catalog.text(Key::…)` / `catalog.render(Key::…)` 直接取——引用集中在这里，门禁照旧生效。
//! **以后新加页面文件不用回来这里登记**：目录里出现就参加（空集与夹具存在性由 `scan` 的下界守着）。

mod rows;

#[cfg(test)]
mod tests;

pub(in crate::pages::instances) use rows::section;

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{human_bytes, ModFailureView, ModProblemView};
use unlucky_presenter::state::ModsState;

use super::Texts;

/// 一条 mod（纯展示字段，来源是 `presenter::dto::ModEntryView`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// 磁盘文件名（开关消息原样带回去）。
    pub file: String,
    /// 展示名（去掉 `.disabled` / `.old`）。
    pub name: String,
    /// 是否启用。
    pub enabled: bool,
    /// 右侧副文案（大小 + 「旧版本残留」/「重复」标记，已由 app 层拼好）。
    pub detail: String,
    /// 这个文件的启停请求在飞（开关先禁用，不做乐观翻面）。
    pub pending: bool,
}

/// 问题区里的一条（**已经是成品句子**，页面只借 `&str`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Problem {
    /// 主句（重复族的主语 / 缺失前置的 id，后者是运行时数据、不翻译）。
    pub text: String,
    /// 副句（`由 x.jar 声明` / `另有 N 份`）；空串 = 不画。
    pub detail: String,
}

/// 问题区（M312）：两段问题 + 几段提示整句。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Problems {
    /// 重复那一段的小标。
    pub duplicate_heading: String,
    /// 缺失那一段的小标。
    pub missing_heading: String,
    /// 同一个模组装了多份（**问题**）。
    pub duplicates: Vec<Problem>,
    /// 必需前置没装（**问题**）。
    pub missing: Vec<Problem>,
    /// 提示类整句（"无法判定" / "可选没装" / "几个文件读不出"）；空 = 不画。
    pub notes: Vec<String>,
    /// 问题条数（重复组数 + 必需缺失项数）。
    pub count: usize,
}

impl Problems {
    /// 有没有要画的东西（一条都没有 ⇒ 整个问题区不出现）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.duplicates.is_empty() && self.missing.is_empty() && self.notes.is_empty()
    }
}

/// MODS 区的展示模型（`Model::mods`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mods {
    /// 这份数据属于哪个实例（`None` = 不在详情页 / 还没请求过）。
    pub bound: Option<String>,
    /// 正在读盘（详情页首帧：请求已发出、快照还没回来）。
    pub loading: bool,
    /// 游戏在该实例上运行（开关只读）。
    pub running: bool,
    /// 实例有 `mods/` 目录吗（`false` 且没有错误 ⇒ 还没装过模组）。
    pub exists: bool,
    /// 读目录失败的人读原因（app 层按 i18n 拼好）。
    pub error: Option<String>,
    /// 列表。
    pub items: Vec<Item>,
    /// 跳过条目数（符号链接 / 目录 / 读不出）。
    pub skipped: usize,
    /// 计数行（`12 启用 · 3 禁用`）：**在这一层算好**，页面只借 `&str`。
    ///
    /// 为什么不留到页面里 `format!`：iced 的 `Element` 只借这一帧的数据，页面函数返回的是
    /// **借用局部变量的树**，局部 `String` 一出栈就悬空（`E0515`）。所以"要显示的那句话"
    /// 一律在视图模型里备好（与 `Detail::status` / `Row::supporting` 同一条做法）。
    pub count: String,
    /// 跳过提示（`跳过 3 项`）。
    pub skipped_note: String,
    /// 重复 / 前置（M312）。
    pub problems: Problems,
    /// 小标上的问题徽标（`2 个问题`）；没有问题时为 `None`。
    pub problem_badge: Option<String>,
    /// 「去模组库找」那枚按钮的字。
    pub missing_cta: String,
}

impl Mods {
    /// 启用中的条数。
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.items.iter().filter(|item| item.enabled).count()
    }

    /// 禁用中的条数。
    #[must_use]
    pub fn disabled_count(&self) -> usize {
        self.items.len() - self.enabled_count()
    }

    /// 有没有拿到一份快照（成功或失败都算；`loading` 中不算）。
    #[must_use]
    pub fn loaded(&self) -> bool {
        self.bound.is_some() && !self.loading
    }

    /// 把某个文件的开关标成「请求在飞」（不做乐观翻面：`enabled` 不动）。
    pub fn mark_pending(&mut self, file: &str) {
        if let Some(item) = self.items.iter_mut().find(|item| item.file == file) {
            item.pending = true;
        }
    }
}

/// presenter 的视图态 → 页面要画的东西（**唯一**的转换点，`app/view/instances.rs` 调）。
///
/// 两条口径：
///
/// 1. **失败不清列表**：list 失败时保留上一份 `items`（用户还能看见列表，只是多一条红字），
///    只有"从来没有过快照"时才是一条纯错误态；
/// 2. **不做乐观翻面**：`enabled` 只来自快照，`pending` 只用来把开关置灰。
#[must_use]
pub fn from_state(state: &ModsState, catalog: Catalog) -> Mods {
    let texts = Texts::of(catalog);
    let mut out = Mods {
        bound: state.bound.clone(),
        loading: state.bound.is_some() && state.view.is_none() && state.failed.is_none(),
        missing_cta: catalog.text(Key::InstancesModsMissingCta).to_string(),
        ..Mods::default()
    };
    if let Some(view) = &state.view {
        out.running = view.running;
        out.exists = view.exists;
        out.skipped = view.skipped.len();
        // **先装列表再算计数**：`enabled_count()` 读的是 `out.items`（顺序反了会画出 `0 启用 · 0 禁用`
        // ——M224 出图实测抓到过一次）。
        out.items = view
            .entries
            .iter()
            .map(|entry| Item {
                file: entry.file.clone(),
                name: entry.name.clone(),
                enabled: entry.enabled,
                detail: item_detail(
                    entry.bytes,
                    entry.old,
                    duplicate_badge(&view.problems, &entry.file, entry.enabled, catalog),
                    texts,
                ),
                pending: state.is_pending(&entry.file),
            })
            .collect();
        out.count = texts.mods_count(out.enabled_count(), out.disabled_count());
        out.skipped_note = texts.mods_skipped(out.skipped);
        out.problems = problems(&view.problems, catalog);
        if out.problems.count > 0 {
            out.problem_badge = Some(catalog.render(
                Key::InstancesModsProblemCount,
                &[("count", &out.problems.count.to_string())],
            ));
        }
    }
    if let Some(failure) = &state.failed {
        out.loading = false;
        out.error = Some(failure_line(failure, texts));
    }
    out
}

/// 启停失败/读目录失败的一句话（**唯一产地**：Toast 与页面内红字共用）。
#[must_use]
pub fn failure_line(failure: &ModFailureView, texts: Texts) -> String {
    let reason = texts.mods_reason(failure.kind);
    let sentence = if failure.file.is_empty() {
        texts.mods_failed(reason)
    } else {
        texts.mods_toggle_failed(&failure.file, reason)
    };
    if failure.detail.is_empty() {
        return sentence;
    }
    format!("{sentence}（{}）", failure.detail)
}

/// 报告 → 问题区的几段话。
fn problems(view: &ModProblemView, catalog: Catalog) -> Problems {
    let duplicates = view
        .duplicates
        .iter()
        .map(|group| Problem {
            text: match group.keep_version.as_deref() {
                Some(version) => format!(
                    "{} · {}",
                    group.display,
                    catalog.render(
                        Key::InstancesModsDuplicateKeep,
                        &[("file", group.keep.as_str()), ("version", version)]
                    )
                ),
                None => format!(
                    "{} · {}",
                    group.display,
                    catalog.render(
                        Key::InstancesModsDuplicateKeepAny,
                        &[("file", group.keep.as_str())]
                    )
                ),
            },
            detail: catalog.render(
                Key::InstancesModsDuplicateMore,
                &[("count", &group.copies.len().to_string())],
            ),
        })
        .collect();
    let missing = view
        .missing
        .iter()
        .map(|item| Problem {
            text: item.id.clone(),
            detail: catalog.render(
                Key::InstancesModsMissingDeclaredBy,
                &[("file", &item.required_by.join(" · "))],
            ),
        })
        .collect();

    let mut notes = Vec::new();
    if !view.unverifiable.is_empty() {
        notes.push(catalog.render(
            Key::InstancesModsUnverifiable,
            &[("count", &view.unverifiable.len().to_string())],
        ));
    }
    if !view.optional.is_empty() {
        notes.push(catalog.render(
            Key::InstancesModsOptional,
            &[("count", &view.optional.len().to_string())],
        ));
    }
    if view.opaque > 0 {
        notes.push(catalog.render(
            Key::InstancesModsOpaque,
            &[("count", &view.opaque.to_string())],
        ));
    }

    Problems {
        duplicate_heading: catalog
            .text(Key::InstancesModsProblemsDuplicates)
            .to_string(),
        missing_heading: catalog.text(Key::InstancesModsProblemsMissing).to_string(),
        duplicates,
        missing,
        notes,
        count: view.problem_count(),
    }
}

/// 这一行要不要挂「重复」徽标（只有**启用中**的副本需要动作：保留 / 禁用）。
fn duplicate_badge(
    view: &ModProblemView,
    file: &str,
    enabled: bool,
    catalog: Catalog,
) -> Option<&'static str> {
    if !enabled {
        return None;
    }
    let group = view.duplicate_of(file)?;
    Some(if group.keep == file {
        catalog.text(Key::InstancesModsDuplicateKeepBadge)
    } else {
        catalog.text(Key::InstancesModsDuplicateDisableBadge)
    })
}

/// 右侧副文案：`1.2 MiB`，再按需挂「旧版本残留」与「重复」两种标记。
fn item_detail(bytes: u64, old: bool, duplicate: Option<&str>, texts: Texts) -> String {
    let mut parts = vec![human_bytes(bytes)];
    if old {
        parts.push(texts.mods_old_badge().to_string());
    }
    if let Some(badge) = duplicate {
        parts.push(badge.to_string());
    }
    parts.join(" · ")
}
