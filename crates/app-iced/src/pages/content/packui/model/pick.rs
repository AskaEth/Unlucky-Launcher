//! 导入**冲突清单**的视图模型（M160）：分页、计数、逐项处置。
//!
//! # 为什么是分页而不是"一次画完"
//!
//! 冲突清单可能有几千条（一个用户改过很多配置的老实例）。`iced` 的每一行都是真实 widget，
//! 一次画几千行会同时拖慢布局与滚动，而用户真正要看的永远只有"这一屏"。所以模型里只出
//! **当前这一页**（[`PAGE_SIZE`] 条），页面上的「上一页 / 下一页」改的是
//! [`PackPageState::pick_page`]——数据本身留在 presenter 的 `ImportScanView` 里，页面不复制。
//!
//! # 三个入口如何体现在计数上
//!
//! | 用户点了 | 决策表 | 计数 |
//! |---|---|---|
//! | 全部保留 | [`ConflictDecisionsView::KeepAll`] | 覆盖 0 |
//! | 全部覆盖 | [`ConflictDecisionsView::OverwriteAll`] | 覆盖 = 全部 |
//! | 逐项 | `PerItem`（只列"覆盖"的那些，其余按保留） | 覆盖 = 点过的条数 |
//!
//! 页面显示的两个数直接来自 [`PackPageState::decisions`]、[`PackPageState::pick_choice`] 的计算，
//! 与发给 host 的那份决策**同源**——不会出现"界面说覆盖 3 个、实际覆盖 0 个"。

use std::collections::BTreeMap;

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{
    human_bytes, ConflictChoiceView, ConflictReasonView, ConflictSourceView, ImportConflictView,
    ImportScanView,
};

/// 一页画多少行（一次画几千行是性能陷阱；40 行在一屏内可读且不滚动太多）。
pub const PAGE_SIZE: usize = 40;

/// 冲突清单块（导入前那一屏）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    /// 标题（`导入前：有 12 个文件已存在且内容不同`）
    pub title: String,
    /// 计数摘要（`保留 9 · 覆盖 3 · 覆盖将丢弃 2.4 MB`）
    pub summary: String,
    /// 缺省说明（"没改动的一律按保留"）
    pub default_note: &'static str,
    /// 判不了的远端条目（`Some` 才画那一行）
    pub unknown_note: Option<String>,
    /// **当前页**的行（最多 [`PAGE_SIZE`] 条）
    pub rows: Vec<PickRow>,
    /// 当前页（0 起）
    pub page: usize,
    /// 总页数（至少 1）
    pub pages: usize,
    /// 区间说明（`第 1-40 项，共 312 项`）
    pub range: String,
    /// 保留几条
    pub keep_count: usize,
    /// 覆盖几条
    pub overwrite_count: usize,
    /// 能不能翻页
    pub can_prev: bool,
    /// 能不能翻页
    pub can_next: bool,
    /// 三个入口的文案
    pub keep_all: &'static str,
    /// 三个入口的文案
    pub overwrite_all: &'static str,
    /// 逐项按钮（保留）的文案
    pub row_keep: &'static str,
    /// 逐项按钮（覆盖）的文案
    pub row_overwrite: &'static str,
    /// 翻页文案
    pub prev: &'static str,
    /// 翻页文案
    pub next: &'static str,
    /// 确认导入
    pub confirm: String,
    /// 取消导入
    pub cancel: &'static str,
    /// 「全部保留」这一档当前是否生效（整批档切到保留时高亮）
    pub keep_all_active: bool,
    /// 「全部覆盖」这一档当前是否生效
    pub overwrite_all_active: bool,
}

/// 清单里的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickRow {
    /// 实例内路径
    pub path: String,
    /// 来源（`包内 overrides/` / `清单 files[]`）
    pub source: String,
    /// 成因（`本来就在` / `你改过`）
    pub reason: String,
    /// 两侧体积（`你的：2.1 MB · 包内：1.8 MB`）
    pub sizes: String,
    /// 当前处置（[`ConflictChoiceView::KeepUser`] = 保留）
    pub choice: ConflictChoiceView,
    /// 这一行按钮上写的字（点了就变成另一侧）
    pub toggle: &'static str,
}

impl PickRow {
    /// 这一行是"覆盖"吗。
    #[must_use]
    pub const fn overwrites(&self) -> bool {
        matches!(self.choice, ConflictChoiceView::Overwrite)
    }
}

/// 从预检结果 + 页面选择造出这一块（`None` = 没有待决冲突，页面不该画它）。
#[must_use]
pub fn pick_of(
    scan: &ImportScanView,
    page_number: usize,
    per_item: &BTreeMap<String, ConflictChoiceView>,
    batch_overwrite: bool,
    catalog: Catalog,
) -> Option<Pick> {
    if scan.conflicts.is_empty() {
        return None;
    }
    let total = scan.conflicts.len();
    let pages = total.div_ceil(PAGE_SIZE).max(1);
    let page = page_number.min(pages - 1);
    let from = page * PAGE_SIZE;
    let to = (from + PAGE_SIZE).min(total);

    let rows: Vec<PickRow> = scan.conflicts[from..to]
        .iter()
        .map(|conflict| row_of(conflict, per_item, batch_overwrite, catalog))
        .collect();

    let overwrite_count = scan
        .conflicts
        .iter()
        .filter(|conflict| {
            effective(conflict.path.as_str(), per_item, batch_overwrite)
                == ConflictChoiceView::Overwrite
        })
        .count();
    let keep_count = total - overwrite_count;
    // 覆盖会丢掉多少：只算"你真选了覆盖"的那些，避免用最坏数字吓人。
    let lost = scan
        .conflicts
        .iter()
        .filter(|conflict| {
            effective(conflict.path.as_str(), per_item, batch_overwrite)
                == ConflictChoiceView::Overwrite
        })
        .map(|conflict| conflict.disk_size)
        .fold(0_u64, u64::saturating_add);

    Some(Pick {
        title: catalog.render(Key::PackPickTitle, &[("count", &total.to_string())]),
        summary: catalog.render(
            Key::PackPickSummary,
            &[
                ("keep", &keep_count.to_string()),
                ("overwrite", &overwrite_count.to_string()),
                ("bytes", &human_bytes(lost)),
            ],
        ),
        default_note: catalog.text(Key::PackPickDefaultNote),
        unknown_note: (scan.unknown_remote > 0).then(|| {
            catalog.render(
                Key::PackPickUnknownRemote,
                &[("count", &scan.unknown_remote.to_string())],
            )
        }),
        rows,
        page,
        pages,
        range: catalog.render(
            Key::PackPickPage,
            &[
                ("from", &(from + 1).to_string()),
                ("to", &to.to_string()),
                ("total", &total.to_string()),
            ],
        ),
        keep_count,
        overwrite_count,
        can_prev: page > 0,
        can_next: page + 1 < pages,
        keep_all: catalog.text(Key::PackPickKeepAll),
        overwrite_all: catalog.text(Key::PackPickOverwriteAll),
        row_keep: catalog.text(Key::PackPickRowKeep),
        row_overwrite: catalog.text(Key::PackPickRowOverwrite),
        prev: catalog.text(Key::PackPickPrev),
        next: catalog.text(Key::PackPickNext),
        confirm: catalog.render(Key::PackPickConfirm, &[]),
        cancel: catalog.text(Key::PackPickCancel),
        keep_all_active: !batch_overwrite,
        overwrite_all_active: batch_overwrite,
    })
}

/// 一条冲突的**有效**处置：逐项优先，其次整批档，缺省保留（与 core 侧同一条规则）。
fn effective(
    path: &str,
    per_item: &BTreeMap<String, ConflictChoiceView>,
    batch_overwrite: bool,
) -> ConflictChoiceView {
    if let Some(choice) = per_item.get(path) {
        return *choice;
    }
    if batch_overwrite {
        ConflictChoiceView::Overwrite
    } else {
        ConflictChoiceView::KeepUser
    }
}

fn row_of(
    conflict: &ImportConflictView,
    per_item: &BTreeMap<String, ConflictChoiceView>,
    batch_overwrite: bool,
    catalog: Catalog,
) -> PickRow {
    let choice = effective(conflict.path.as_str(), per_item, batch_overwrite);
    PickRow {
        path: conflict.path.clone(),
        source: source_label(conflict.source, catalog).to_owned(),
        reason: reason_label(conflict.reason, catalog).to_owned(),
        sizes: format!(
            "{} · {}",
            catalog.render(
                Key::PackPickDisk,
                &[("size", &human_bytes(conflict.disk_size))]
            ),
            catalog.render(
                Key::PackPickIncoming,
                &[("size", &human_bytes(conflict.pack_size))]
            )
        ),
        choice,
        // 按钮写"点了会变成什么"：当前保留 ⇒ 按钮是「覆盖」。
        toggle: if matches!(choice, ConflictChoiceView::Overwrite) {
            catalog.text(Key::PackPickRowKeep)
        } else {
            catalog.text(Key::PackPickRowOverwrite)
        },
    }
}

/// 来源的人读串。
fn source_label(source: ConflictSourceView, catalog: Catalog) -> &'static str {
    match source {
        ConflictSourceView::Overrides => catalog.text(Key::PackPickSourceOverrides),
        ConflictSourceView::ClientOverrides => catalog.text(Key::PackPickSourceClientOverrides),
        ConflictSourceView::CurseForge | ConflictSourceView::Modrinth => {
            catalog.text(Key::PackPickSourceManifest)
        }
    }
}

/// 成因的人读串（复用导入报告那三句，不另造一套说法）。
fn reason_label(reason: ConflictReasonView, catalog: Catalog) -> &'static str {
    match reason {
        ConflictReasonView::UserModified => catalog.text(Key::PackConflictUserModified),
        ConflictReasonView::UserDeleted => catalog.text(Key::PackConflictUserDeleted),
        ConflictReasonView::PreExisting => catalog.text(Key::PackConflictPreExisting),
    }
}
