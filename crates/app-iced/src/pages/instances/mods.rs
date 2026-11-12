//! 实例详情页的 **MODS 区**（M224 · MODTOGGLE）：本地 mod 列表 + 启停开关。
//!
//! # 位置与形状
//!
//! 挂在详情列里、元信息网格之后、启动参数卡**之前**（`pages/instances/detail.rs` 的 `detail_column`），
//! 动作行仍在最后。与启动参数区同一套卡片口径：[`frame::card`] + 一个 `LabelMedium` 小标 + 内容。
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
//! | 正常 | 可点（发 [`PageMessage::SetModEnabled`]） | — |
//! | 游戏在跑 | 禁用（不带消息） | Windows 上进程正持有 jar，rename 必然失败（host 侧也挡了一层） |
//! | 请求在飞 | 禁用 | 同一文件连点两次会发出两条 rename，第二条必然报「文件不在」 |
//!
//! **不做乐观回显**：开关画的是快照里的 `enabled`，点完等 host 回一份新快照才翻面。
//! 与启动/停止同一条纪律（`app/update/instances.rs` 的 Save 分支）。

use iced::widget::{column, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::Catalog;
use unlucky_presenter::dto::{human_bytes, ModFailureView};
use unlucky_presenter::state::ModsState;

use crate::app::components::interaction::Interaction;
use crate::app::components::selection::{switch, Switch};
use crate::theme::Ui;

use super::super::frame;
use super::super::Message;
use super::state::Message as PageMessage;
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
    /// 右侧副文案（大小 + 「旧版本残留」标记，已由 app 层拼好）。
    pub detail: String,
    /// 这个文件的启停请求在飞（开关先禁用，不做乐观翻面）。
    pub pending: bool,
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
                detail: item_detail(entry.bytes, entry.old, texts),
                pending: state.is_pending(&entry.file),
            })
            .collect();
        out.count = texts.mods_count(out.enabled_count(), out.disabled_count());
        out.skipped_note = texts.mods_skipped(out.skipped);
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

/// 右侧副文案：`1.2 MiB` 或 `1.2 MiB · 旧版本残留`。
fn item_detail(bytes: u64, old: bool, texts: Texts) -> String {
    let size = human_bytes(bytes);
    if old {
        format!("{size} · {}", texts.mods_old_badge())
    } else {
        size
    }
}

/// 画 MODS 区（实例页唯一入口，`detail.rs` 调它）。
pub(super) fn section<'a>(
    ui: &'a Ui,
    mods: &'a Mods,
    forced: Option<Interaction>,
    texts: Texts,
) -> Element<'a, Message> {
    let mut body = column![head(ui, mods, texts)]
        .spacing(ui.space(Space::X3))
        .width(Length::Fill);

    if mods.loading {
        body = body.push(note(ui, texts.mods_loading()));
    } else if let Some(error) = mods.error.as_deref() {
        body = body.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            error,
            ui.color(Role::Error),
        ));
    } else if !mods.exists {
        body = body.push(note(ui, texts.mods_missing_dir()));
    } else if mods.items.is_empty() {
        body = body.push(note(ui, texts.mods_empty()));
    } else {
        // 只读原因放在列表**上面**：用户先看到"为什么点不动"，再去点。
        if mods.running {
            body = body.push(note(ui, texts.mods_running()));
        }
        for item in &mods.items {
            body = body.push(entry_row(ui, item, mods.running, forced));
        }
        if mods.skipped > 0 {
            body = body.push(note(ui, &mods.skipped_note));
        }
    }

    frame::card(ui, body)
}

/// 小标行：`MODS` + 计数（`12 启用 · 3 禁用`）。
fn head<'a>(ui: &'a Ui, mods: &'a Mods, texts: Texts) -> Element<'a, Message> {
    let mut line = row![frame::text_role(
        ui,
        TypeRole::LabelMedium,
        false,
        texts.mods_title(),
        ui.color(Role::OnSurfaceVariant),
    )]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center)
    .width(Length::Fill);

    // 计数只在"真有一份可用快照"时画：读盘中 / 读失败 / 空目录画 0·0 会像"这里什么都没有"。
    let show_count = mods.loaded() && mods.error.is_none() && !mods.items.is_empty();
    if show_count {
        line = line.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            mods.count.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    line.into()
}

/// 一条 mod 的行：名字 + 副文案 + 开关。
fn entry_row<'a>(
    ui: &'a Ui,
    item: &'a Item,
    running: bool,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let label = column![
        frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            item.name.as_str(),
            ui.color(if item.enabled {
                Role::OnSurface
            } else {
                Role::OnSurfaceVariant
            }),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            item.detail.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    let toggle = if running || item.pending {
        Switch::disabled("", item.enabled)
    } else {
        Switch::new(
            "",
            item.enabled,
            Message::Instances(PageMessage::SetModEnabled {
                file: item.file.clone(),
                enabled: !item.enabled,
            }),
        )
    };
    // M237：开关住在 `frame::card` 里，禁用态的 12% 轨道 / 38% 滑块要叠**卡底**（`frame::BASE`），
    // 不是画布底——否则「游戏在跑 / 请求在飞」那两档的开关会与卡片底色脱节。
    let toggle = toggle.base(frame::BASE);
    // 出图取证（`--force-state`）把开关也钉到同一档：否则截图里只有按钮是 hover、
    // 开关还是静息态，视觉复审没法判"悬停时的开关长什么样"。
    let toggle = match forced {
        Some(state) => toggle.state(state),
        None => toggle,
    };
    row![label, switch(ui, toggle)]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

fn note<'a>(ui: &'a Ui, text: &'a str) -> Element<'a, Message> {
    frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        text,
        ui.color(Role::OnSurfaceVariant),
    )
}
