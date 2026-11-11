//! `SavesState` 的三样**交互态**：行内草稿 / 在飞标记 / 删除确认目标（M247）。
//!
//! 从 `state/saves.rs` 拆出（那个文件加完安全不变量文档之后越过 400 行硬线）。生命周期表在
//! 父模块的文档里一张写全——**这里只放实现**，不重复判据。
//!
//! 三条边界（父模块文档的展开）：
//! 1. `draft` / `pending` 只跟"这一拍在编辑/在飞什么"有关，快照落地一律清；
//! 2. `confirm` **认框**：`take_confirm(current)` 只在 id 相等时交付（见 [`SavesState::take_confirm`]）；
//! 3. `closed_confirm` 是"这个框该收起来了"的信号，app 层 `refresh_view` 取走并真的收框。

use crate::dialogs::DialogId;

use super::{RenameDraft, SavesOp, SavesState};

impl SavesState {
    /// 开一个行内重命名（输入框预填当前磁盘名：改个后缀比从零打更常见）。
    pub fn start_rename(&mut self, dir: &str) {
        self.draft = Some(RenameDraft {
            dir: dir.to_string(),
            text: dir.to_string(),
        });
    }

    /// 输入框变了（**照原样存**，不做前端过滤：非法名字由 core 拒并给出具名原因）。
    pub fn edit_rename(&mut self, text: String) {
        if let Some(draft) = &mut self.draft {
            draft.text = text;
        }
    }

    /// 取走草稿并收起来（提交时用；`None` = 当前没有草稿）。
    pub fn take_rename(&mut self) -> Option<RenameDraft> {
        self.draft.take()
    }

    /// 取消行内重命名。
    pub fn cancel_rename(&mut self) {
        self.draft = None;
    }

    /// 记下「删除确认框正在等谁」：**框的 id + 世界名**（`dialog` 由调用方 `dialogs.push` 拿到）。
    pub fn ask_delete(&mut self, dialog: DialogId, dir: &str) {
        self.confirm = Some((dialog, dir.to_string()));
        self.closed_confirm = None;
    }

    /// 取走确认目标（点「确定」时用）——**只核销 `current` 那一个框**。
    ///
    /// 「取走即清」+「认框不认人」两条合起来是**安全不变量**：
    /// ①同一次确认连点两下只发一条删除（第二下拿不到目标）；
    /// ②别的框（崩溃卡片的「导出诊断包」、进度框……）点主按钮**拿不走**这个目标——
    ///   哪怕某条关闭路径漏清（复审 r1 P1-1 的事故形态），也只能核销它自己那个框。
    pub fn take_confirm(&mut self, current: DialogId) -> Option<String> {
        match &self.confirm {
            Some((dialog, _)) if *dialog == current => {
                self.closed_confirm = None;
                self.confirm.take().map(|(_, dir)| dir)
            }
            _ => None,
        }
    }

    /// **某个**确认框被取消/关掉（次按钮、遮罩、`Esc` 三条路径都走它）：**只清它自己那个目标**。
    ///
    /// 为什么不无条件清（复审 r2 的加固）：`clear_confirm` 不认框，理论上够得着"清掉别人的删除目标"
    /// ——今天三条关闭路径关的都是当前框，够不着，但把判据写死在这里比依赖调用顺序稳。
    pub fn close_confirm(&mut self, current: DialogId) {
        if self.confirm_dialog() == Some(current) {
            self.clear_confirm();
        }
    }

    /// 清掉目标（**无条件**）：换实例 / 复位时用——那时"哪个框"已经不重要了。
    pub fn clear_confirm(&mut self) {
        self.confirm = None;
        self.closed_confirm = None;
    }

    /// 取走「因为世界已经不在快照里而作废」的那个框（app 层据此收框，避免主按钮空操作）。
    pub fn take_closed_confirm(&mut self) -> Option<DialogId> {
        self.closed_confirm.take()
    }

    /// 当前确认目标属于哪个框（`None` = 没有在等的目标）。
    #[must_use]
    pub fn confirm_dialog(&self) -> Option<DialogId> {
        self.confirm.as_ref().map(|(dialog, _)| *dialog)
    }

    /// 标记一次写操作在飞（草稿同时收起来：请求已经发出去了）。
    pub fn mark_pending(&mut self, op: SavesOp) {
        self.pending = Some(op);
        self.draft = None;
    }

    /// 这个世界的写操作在飞吗（用来把那一行置灰）。
    #[must_use]
    pub fn is_pending(&self, dir: &str) -> bool {
        self.pending.as_ref().is_some_and(|op| op.dir() == dir)
    }
}
