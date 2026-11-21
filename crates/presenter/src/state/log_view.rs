//! 日志页正文的**视图归属**（M459）：正文是「历史视图」还是跟着实时尾随走。
//!
//! ## 这条位修的是什么
//!
//! 实时一场在跑时打开历史会话（`host::open_session` → `LogSessionOpened` + `LogLinesChanged`），
//! 正文换成那一场的整份读取，`live_session` 照旧清成 `None`。于是 `state/live.rs` 里那条
//! 「换会话就清行」的判据（`live_session.as_deref().is_some_and(...)`）在下一拍**什么也不做**
//! ——它看到的是 `None`——而 `append_live_lines` 照样追加、照样写 `selected_session`。
//! 下一拍尾随一到：实时行续写进历史正文，身份跳回 live（复现见本文件的 `tests` 第一条）。
//!
//! ## 为什么不改那条清行判据
//!
//! 它是**正常启动路径**的清零点（换了一场在跑 ⇒ 上一场的行作废）。把它改成「`None` 也算换了
//! 会话」会在**刚启动那一刻**触发：`live_session` 初始是 `None`，第一次 `LogAppended` 就会把
//! 宿主已经拼好的行再清一遍——而那正是既有用例与 M458 覆盖的路径。清行判据一行都没动。
//!
//! ## 现在的机制
//!
//! 多一位「历史视图」归属（[`LogView`]），只由历史路径写、只由实时路径清：
//!
//! | 事件 | 对 [`LogView`] 的作用 |
//! |---|---|
//! | `LogSessionOpened(id)` | 进历史视图；**把正在跑的那一场挪进「被挡住」位**（`live_session` 同时清空）。打开的就是正在跑那一场 ⇒ 不进（切回实时） |
//! | `LogAppended(batch)` | batch 属于「被挡住」那一场 ⇒ 整批丢掉（正文、身份都不动）；别的场 ⇒ 退历史视图、历史正文作废 |
//! | `LaunchFinished` | 历史视图期间**不换身份**（用户看的正文还是那一场历史） |
//!
//! ## 回实时那一场（M462）
//!
//! M459 只修了「历史视图不接实时行」，却没给退出它的路：会话索引只在**收尾时**追加
//! （`core::launch::session::SessionWriter::finish` → `session_index::append`），所以正在跑那一场
//! 在选择器里没有行，而「被挡住那一场收尾」与「换了一场在跑」这两个退出条件在游戏一直跑时都
//! 不发生——用户点了一条历史会话就再也回不去。
//!
//! 现在 [`LogView::held`] 旁边多一位 `holding`（被挡住那一场**是否仍在跑**），页面据此
//! 在历史视图里画出「回到实时」入口，点下去走的就是本文件 `open_log_session` 的切回分支
//! （`host` 那一侧由 `CoreHost::open_session` 认得「正在尾随的那一场」——它不在索引里，
//! `host::logs::open` 找不到它）。`holding` 在那一场收尾时落下：收尾之后 [`LogView::held`] 仍然
//! 留着（M459 的尾巴行判据要用），但**再也回不去了**，入口必须消失——一个点了没用的控件比没有更坏。
//!
//! 被挡住那一场的行是**丢掉、不是缓存**：盘上的事件流才是真相，`host::logs::open` 整份重读，
//! 用户切回那一场时一行都不少——所以「丢掉」不必付缓冲的账。
//!
//! 已知边界（没修，登记在报告里）：多实例并发。`live_session` 与这里的 `held` 都是单槽，
//! 本仓一次只跑一场（`host::handle` 的 `launches` 允许多场，但 UI 侧只画一场）。

use super::AppState;
use crate::dto::{LogLine, SessionId};

/// 日志正文的视图归属（M459）。判据与理由见模块文档。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LogView {
    /// 正文是不是「历史视图」（`open_session` 整份读回来的内容）。
    history: bool,
    /// 进历史视图那一刻正在跑的那一场：它的实时行被挡在门外。
    ///
    /// 它在收尾之后**仍然留着**：那一场最后几拍尾巴行（`running=false` 的批次）同样不许进
    /// 历史正文（`Tail` 的最后一读常常晚于收尾）。
    held: Option<SessionId>,
    /// 被挡住那一场**是否仍在跑**（M462）。
    ///
    /// [`held`](Self::held) 收尾后不清、这一位收尾时落下：两件事分开记，是因为「这一场还在写行」
    /// 与「用户还能回去」不是同一个条件——收尾之后行还得继续挡，但回去的入口必须消失。
    holding: bool,
    /// 「紧接着的那一次整份重读就是回到实时那一次」的寄存位（M462）。
    ///
    /// `LogSessionOpened` 与 `LogLinesChanged` 是同一批事件里的一对，而后者要把 `live_session`
    /// 清成 `None`（历史路径的既有清零点）。切回实时时身份就寄存在这儿，由那一次重读收走；
    /// 取过即清，所以只有紧随其后的那一次生效。
    returning: Option<SessionId>,
}

impl LogView {
    /// 正文现在是不是历史视图（`open_session` 读回来的那一场）。
    #[must_use]
    pub fn in_history(&self) -> bool {
        self.history
    }

    /// 被历史视图挡住的那一场（`None` = 没有会话在跑，或根本不在历史视图）。
    ///
    /// 页面用它把「实时那一场的结论」挡在历史页外（见 `app/view/logs.rs` 的归属判据）。
    #[must_use]
    pub fn held(&self) -> Option<&str> {
        self.held.as_deref()
    }

    /// 这一批实时行是不是必须挡在门外（属于被挡住的那一场）。
    pub(super) fn holds(&self, session: &str) -> bool {
        self.held.as_deref() == Some(session)
    }

    /// 「回到实时那一场」的落点（M462）：被挡住的那一场**仍在跑**时给它的 id，否则 `None`。
    ///
    /// 页面拿它当入口的**唯一**判据（`app/view/logs.rs` → `pages::Logs::back_to_live`）：`None`
    /// 就一个控件都不画。收尾之后 [`held`](Self::held) 还留着、这一位已经落下 ⇒ `None`，于是
    /// 「点了没用」的入口不可能出现在界面上。
    #[must_use]
    pub fn live_return(&self) -> Option<&str> {
        if !self.history || !self.holding {
            return None;
        }
        self.held.as_deref()
    }

    /// 被挡住那一场是不是还在跑（`open_log_session` 切回实时时用它决定要不要把身份寄存回去）。
    #[must_use]
    pub(super) fn blocked_is_running(&self) -> bool {
        self.history && self.holding
    }

    /// 进历史视图：`live` = 这一刻正在跑的那一场。
    ///
    /// `live` 为 `None` 时**不清**已有的「被挡住」位：切到另一场历史会话时，正在跑的那一场
    /// 已经记在这儿了（忘了它 = 下一拍又把实时行续写进来）；同理 `holding` 也原样留着。
    fn enter(&mut self, live: Option<SessionId>) {
        self.history = true;
        self.returning = None;
        if live.is_some() {
            self.held = live;
            self.holding = true;
        }
    }

    /// 被挡住的那一场收尾了（M462）：行照旧挡（[`held`](Self::held) 不动），但**回不去了**。
    fn note_ended(&mut self, session: &str) {
        if self.held.as_deref() == Some(session) {
            self.holding = false;
        }
    }

    /// 记下「紧接着的那一次整份重读就是回到实时那一次」（M462）。
    ///
    /// 为什么不直接把 `live_session` 写好：`LogSessionOpened` 之后紧跟一条 `LogLinesChanged`，
    /// 而它的清零点（`live_session = None`）是历史路径的既有行为，动不得。所以把身份**寄存**在
    /// 这一位里，由那一次整份重读把它补回来——顺序对了，连点两下也是幂等的。
    pub(super) fn mark_returning(&mut self, session: SessionId) {
        self.returning = Some(session);
    }

    /// 取出寄存的身份（`LogLinesChanged` 用；取过就没了，所以只有紧随的那一次生效）。
    pub(super) fn take_returning(&mut self) -> Option<SessionId> {
        self.returning.take()
    }

    /// 回实时视图（`true` = 刚才在历史视图里，调用方要把历史正文作废）。
    pub(super) fn leave(&mut self) -> bool {
        let was = self.history;
        self.history = false;
        self.held = None;
        self.holding = false;
        was
    }
}

impl AppState {
    /// `CoreEvent::LogSessionOpened`：历史会话的身份（M458）+ 视图归属（M459）。
    ///
    /// 打开的就是**正在跑那一场**时不算历史视图：它还在往自己身上写行，用户点回那一场等于
    /// 回实时视图——挡住它的行会把那一页冻在打开那一刻的快照上，再也长不出一行。
    ///
    /// ## 身份当场落回去（M462）
    ///
    /// 以前这里只是 `leave()`，`live_session` 留给下一拍 `LogAppended` 补。加了「回到实时」入口
    /// 之后那条路要能**连点**：第二下的事件对到达时 `live_session` 若还是 `None`、`held` 已经清掉，
    /// 它就会被当成「又打开了一场历史会话」——`enter()` 重新置位历史视图，紧随的整份重读把正文
    /// 铺好，下一拍尾随却按「退历史视图」清行，用户看到正文突然只剩一行。
    /// 所以切回实时时把身份**寄存**给紧随的那一次整份重读（`mark_returning` → `replace_log_lines`），
    /// 第二下就走上面那条 `live == session` 分支，幂等。
    pub(super) fn open_log_session(&mut self, session: SessionId) {
        let live = self.live_session.take();
        if live.as_deref() == Some(session.as_str()) {
            // 已经在实时那一场上了（连点第二下）：退出历史视图是幂等的，身份照旧寄存回去。
            self.log_view.leave();
            self.log_view.mark_returning(session.clone());
        } else if self.log_view.holds(&session) {
            // 切回**被历史视图挡住**的那一场。它仍在跑才算回实时；已经收尾的那一场（被挡住位收尾
            // 后仍留着，见 `note_ended`）只是「历史正文到此结束」，身份不补。
            let running = self.log_view.blocked_is_running();
            self.log_view.leave();
            if running {
                self.log_view.mark_returning(session.clone());
            }
        } else {
            self.log_view.enter(live);
        }
        self.selected_session = Some(session);
    }

    /// `CoreEvent::LogLinesChanged`：正文整份换成历史读取的结果。
    ///
    /// 前两行与 M458 之前**逐字相同**（历史视图的保护不靠 `live_session`：`open_log_session`
    /// 已经把正在跑的那一场挪进 [`LogView`] 了）。既有的 mock 路径与用例依赖这条清零点。
    ///
    /// M462 只在其后**补**一句：这一次整份重读如果是「回到实时」那一次（`returning` 有值），
    /// 身份当场落回正在尾随那一场——清零点本身一行没动。
    pub(super) fn replace_log_lines(&mut self, lines: Vec<LogLine>) {
        self.log_lines = lines;
        self.live_session = None;
        if let Some(session) = self.log_view.take_returning() {
            self.live_session = Some(session);
        }
    }

    /// `CoreEvent::LaunchFinished` 的**身份**部分（M459）。
    ///
    /// 历史视图期间不换身份：正文还是那一场历史（`open_session` 读回来的），把页头换成刚收尾
    /// 的这一场就是「身份跳回 live」的另一半。收尾的信号本身照旧（`launch_ends` +1，Toast 由
    /// app 层按 key 现求）。
    pub(super) fn finish_log_session(&mut self, session: &str) {
        self.live_session = None;
        // M462：被挡住那一场收尾 ⇒ 「回到实时」的入口落下（行还要继续挡，见 `LogView`）。
        self.log_view.note_ended(session);
        if !self.log_view.in_history() {
            self.selected_session = Some(session.to_string());
        }
    }
}

#[cfg(test)]
#[path = "log_view_tests.rs"]
mod tests;
