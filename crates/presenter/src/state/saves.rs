//! 实例存档 / 截图快照的落地（M235 · STOREMVP）+ 写操作的三样交互态（M247 · SAVESOPS）。
//!
//! 从 `state.rs` 拆出（那个文件贴着 400 行硬线），形状与 [`super::mods`] 一致：
//! **另一个 `impl AppState` 块**，路径与可见性都不变，`apply_core` 只有一行投递。
//!
//! # 写操作的三样交互态（M247）
//!
//! | 字段 | 是什么 | 为什么必须住在这里 |
//! |---|---|---|
//! | [`SavesState::draft`] | 行内重命名的输入缓冲 | 页面模型每帧从快照重建（`from_state`），打字过程中的内容不在快照里 ⇒ 不留在这里就会每打一个字被事件冲掉 |
//! | [`SavesState::pending`] | 哪个世界的哪个写操作在飞 | 与 [`super::mods::ModsState::pending`] **同一条先例**：只用来把那一行的按钮置灰，**不做乐观回显**（列表只跟快照走） |
//! | [`SavesState::confirm`] | 删除确认框**正在等谁** | 模态队列只画框、不记得"确认之后要删哪个世界"；而点主按钮的那一刻是 `app/update/dialog.rs`，那里只能从状态里取 |
//!
//! ## 生命周期（谁写、谁清、何时必须清）
//!
//! | 字段 | 谁写 | 谁清 | 必须清的时刻 |
//! |---|---|---|---|
//! | `draft` | `start_rename`（预填当前磁盘名）、`edit_rename`（每敲一个字） | `cancel_rename` / `take_rename`（提交时取走）/ `mark_pending`（请求已发出，草稿没用了）/ `bind` / `changed` | **换实例**、**请求发出**、**快照回来**：三者之后草稿都属于"上一次对话"，留着会让「改名」按钮改到一个已经在别的实例上的名字 |
//! | `pending` | `mark_pending`（发请求之前，一跳都不等） | `changed`（成功）/ `failed`（失败也要清——**按钮不能永远灰着**，用户要能直接再点一次）/ `bind` | **任何一次后台往返收尾**、**换实例** |
//! | `confirm` | `ask_delete(dialog, dir)`（app 层先 `dialogs.push` 拿到 id 再登记） | [`SavesState::take_confirm(current)`](SavesState::take_confirm)（**只核销同一个框、取走即清**）/ [`SavesState::clear_confirm`]（次按钮 / 遮罩 / `Esc` / 换实例）/ [`SavesState::changed`]（**只在被确认的世界已从快照里消失时**） | **点主按钮那一刻**、**取消/关闭模态框**、**换实例**、**那个世界没了**（`closed_confirm` 会一并收框） |
//! | `closed_confirm` | `changed`（世界从快照里消失时登记作废的那个框） | app 层 `refresh_view` → `settle_saves_confirm`（收框后取走）/ `ask_delete` / `clear_confirm` / `bind` | **同一拍或下一拍**：留着会让"世界已经不在了"的确认框一直挡在屏幕中央 |
//!
//! ### `confirm` 是**绑定到具体对话框**的一条安全不变量
//!
//! 三条判据，缺一条就会出安全事故（复审 r1 P1-1 的事故形态就是第三条缺失）：
//!
//! 1. **取走即清**：主按钮拿到目标的同时字段被置空 ⇒ 同一次确认连点两下只发一条删除。
//! 2. **认框不认人**：`take_confirm(current)` 只在 `current` 等于登记时那个框的 id 时才交付。
//!    「删存档 → `Esc` → 崩溃卡片 → 点『导出诊断包』」这条链上，第二个框的主按钮**拿不走**目标
//!    （即使 `Esc` 那条关闭路径漏清——而它现在也清了）。
//! 3. **世界没了就把框收起来**：快照回来时被确认的世界已不在列表里 ⇒ 清 `confirm` 并登记
//!    `closed_confirm`，app 层据此收框。否则主按钮会变成一次**静默空操作**（点了没删、也不说一句）。
//!
//! 用例（presenter 层）：`the_delete_confirmation_hands_out_once`（取走即清 + 取消后不再有目标）、
//! `a_dialog_can_only_redeem_its_own_target`（别的框的主按钮拿不走）、
//! `an_unrelated_snapshot_no_longer_drops_the_confirmation`（P2-1：世界还在 ⇒ 目标还兑现得了）、
//! `the_confirmation_is_dropped_and_reported_when_the_world_vanishes`（世界没了 ⇒ 清 + 登记收框）。
//!
//! # 为什么快照住在这里而不是页面
//!
//! 「列表画的永远是磁盘上的真相」是本模块唯一要保证的事，所以：
//!
//! - 快照走 [`crate::dto::CoreEvent::Saves`] 的 `Ok` 一支，**整体替换**（不做增量合并）——
//!   host 每次都是重新扫一遍目录才发的，合并只会两处真相；
//! - 失败走同一个事件的 `Err` 一支，记下一条**具名原因**，页面按 i18n key 现求文案；
//! - **可重试**：失败时不清 `bound`，页面据此画"重试"按钮（再发一条
//!   [`crate::dto::SavesCommand::List`] 就行，没有别的状态要复位）。
//!
//! # 迟到结果
//!
//! 用户很可能点开 A 的详情页、还没等后台回来就切到 B。所以 `view.instance` 与
//! [`SavesState::bound`] 不一致的结果**直接丢掉**（与 [`super::mods`] 同一条纪律）。

use crate::dialogs::DialogId;
use crate::dto::{SavesFailureView, SavesListView};

mod interaction;

use super::AppState;

/// 行内重命名的输入缓冲（见模块文档的三样交互态）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameDraft {
    /// 哪个世界（磁盘名，提交时原样带回去当「源」）。
    pub dir: String,
    /// 输入框里的内容（**原样**：合不合法由 core 判，页面只管显示，不做前端过滤）。
    pub text: String,
}

/// 一次在飞的写操作（只用来置灰那一行，列表仍然只跟快照）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SavesOp {
    /// 重命名中。
    Rename {
        /// 磁盘名
        dir: String,
    },
    /// 删除中。
    Delete {
        /// 磁盘名
        dir: String,
    },
    /// 备份中。
    Backup {
        /// 磁盘名
        dir: String,
    },
}

impl SavesOp {
    /// 这个操作动的是哪个世界。
    #[must_use]
    pub fn dir(&self) -> &str {
        match self {
            Self::Rename { dir } | Self::Delete { dir } | Self::Backup { dir } => dir,
        }
    }
}

/// 存档区的视图态。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SavesState {
    /// 当前详情页的实例 id（`None` = 不在详情页）。
    pub bound: Option<String>,
    /// 最近一份快照（`None` = 还没请求过）。
    pub view: Option<SavesListView>,
    /// 最近一条失败（具名原因；成功一次就清掉）。
    pub failed: Option<SavesFailureView>,
    /// 行内重命名的输入缓冲（`None` = 没有在改名）。
    pub draft: Option<RenameDraft>,
    /// 在飞的写操作（`None` = 空闲）。
    pub pending: Option<SavesOp>,
    /// 删除确认框正在等谁：**(框的 id, 世界名)**（点「确定」时才真的删）。
    ///
    /// 为什么带框 id（复审 r1 P1-1）：`take_confirm(current)` 只核销**同一个框**。即使某条
    /// 关闭路径漏清，目标也不会被下一个不相干的对话框（例如崩溃卡片的「导出诊断包」）兑成一次删除。
    pub confirm: Option<(DialogId, String)>,
    /// 刚刚因为「被确认的世界已经不在快照里」而作废的那个框（app 层据此收框，见
    /// [`SavesState::take_closed_confirm`]）。
    pub closed_confirm: Option<DialogId>,
}

impl SavesState {
    /// 切到另一个实例（换实例必须清空：A 的存档不能画在 B 的详情页上）。
    ///
    /// 三样交互态一起清：换实例之后还留着「A 的行内草稿 / A 的删除确认」是错位状态
    /// （点确定会去删 B 页上的 A，或者把 A 的草稿提交成 B 的改名）。
    pub(super) fn bind(&mut self, instance: Option<&str>) {
        if self.bound.as_deref() == instance {
            return;
        }
        self.bound = instance.map(str::to_string);
        self.view = None;
        self.failed = None;
        self.reset_interaction();
    }

    /// 清掉三样交互态（换实例 / 操作回程时用）。
    fn reset_interaction(&mut self) {
        self.draft = None;
        self.pending = None;
        self.confirm = None;
        self.closed_confirm = None;
    }

    /// 一条结果落地（`Ok` = 快照整体替换，`Err` = 记一条具名失败；迟到/串台的丢掉）。
    ///
    /// **成败对三样交互态的处理不一样**（复审 r1 P2-1；与模块文档的安全判据第 3 条同源）：
    ///
    /// | 支 | `draft` / `pending` | `confirm` |
    /// |---|---|---|
    /// | `Ok`（快照） | 清（这一拍写操作收尾了） | **只在「被确认的那个世界已不在快照里」时**清，并登记 [`SavesState::closed_confirm`] 让 app 收框 |
    /// | `Err`（失败） | 只清 `pending`（按钮不能永远灰着） | **不动**：一条读盘失败不该把用户刚点出来的确认框变成"点了没反应" |
    ///
    /// 换言之「无论成败都清确认中」是**旧的**说法，已经不成立：无条件清会让主按钮静默空操作。
    pub fn apply(&mut self, result: Result<SavesListView, SavesFailureView>) {
        match result {
            Ok(view) => self.changed(view),
            Err(failure) => self.failed(failure),
        }
    }

    /// 快照落地（整体替换；迟到/串台的丢掉）。
    ///
    /// 三样交互态在这里**不是一起清**（复审 r1 P2-1）：
    ///
    /// | 字段 | 这一拍怎么处理 |
    /// |---|---|
    /// | `draft` / `pending` | **清**：写操作已经收尾，行按新快照画 |
    /// | `confirm` | **只在「被确认的那个世界已经从快照里消失」时清**，并记下那个框（[`Self::closed_confirm`]）让 app 收框 |
    ///
    /// 为什么不能无条件清 `confirm`：备份一个大世界还在飞、「打开存档文件夹」的回程还在路上时，
    /// 用户点「删除 X」→ 框刚弹出来 → 一拍无关快照先到 → 目标被清 → 用户点「删除世界」只关框、
    /// 不删、也不说一句。少删是安全的，但**静默就是不安全的另一种形式**（用户以为删了）。
    pub fn changed(&mut self, view: SavesListView) {
        if self.bound.as_deref() != Some(view.instance.as_str()) {
            return;
        }
        self.failed = None;
        // 目标世界还在不在这一份快照里（`dir` 是磁盘上的名字，与列表同一口径）。
        let gone = self
            .confirm
            .as_ref()
            .map(|(_, dir)| !view.worlds.iter().any(|world| &world.dir == dir));
        self.view = Some(view);
        self.draft = None;
        self.pending = None;
        if gone == Some(true) {
            if let Some((dialog, _)) = self.confirm.take() {
                self.closed_confirm = Some(dialog);
            }
        }
    }

    /// 失败落地（只记**当前绑定实例**的；old 列表保留——用户还能看见旧列表，只是多一条红字）。
    pub fn failed(&mut self, failure: SavesFailureView) {
        if self.bound.as_deref() != Some(failure.instance.as_str()) {
            return;
        }
        self.failed = Some(failure);
        // 失败也收尾：按钮不能永远灰着（用户要能直接再点一次重试）。
        // `confirm` **不动**（与 [`Self::changed`] 同一条理由）：一条读盘失败不该把用户
        // 刚点出来的删除确认框变成"点了没反应"。
        self.pending = None;
    }

    /// 当前绑定实例的列表是否已经有一份快照。
    #[must_use]
    pub fn has_view(&self) -> bool {
        self.view.is_some()
    }
}

impl AppState {
    /// 进/出详情页时对齐绑定（`refresh_view` 调；与 `bind_mods` 同一处生命周期）。
    pub fn bind_saves(&mut self, instance: Option<&str>) {
        self.saves.bind(instance);
    }
}

#[cfg(test)]
mod tests;
