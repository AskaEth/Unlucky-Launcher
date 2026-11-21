//! 实时日志的滚动窗口（从 `state.rs` 搬出，守 400 行硬线）。
//!
//! 常量与「换会话就清行」那条判据是搬移时原样保留的；M459 在它**前后**各加了一条历史视图
//! 判据（`state/log_view.rs`），中间那段判据一字未动。`state.rs` 用 `pub use` 再导出
//! [`MAX_LIVE_LINES`]，老路径 `crate::state::MAX_LIVE_LINES` 继续可用。

use super::AppState;

/// 实时日志最多保留的行数（滚动窗口；更多行由日志页的筛选/历史会话承担）。
pub const MAX_LIVE_LINES: usize = 2_000;

impl AppState {
    /// 实时日志增量：只追加，封顶 [`MAX_LIVE_LINES`] 行（滚动窗口）。
    pub(super) fn append_live_lines(&mut self, batch: &crate::dto::LiveLogBatch) {
        // M459：历史视图正在看**别的**那一场 ⇒ 这批实时行不进正文、也不动身份。正文是
        // `open_session` 整份读回来的历史内容，接上实时行就是把两场日志缝在同一页
        // （判据与理由在 `super::log_view` 的模块文档）。
        if self.log_view.holds(&batch.session) {
            return;
        }
        if self
            .live_session
            .as_deref()
            .is_some_and(|session| session != batch.session)
        {
            // 换了一个会话在跑：清掉上一场的行，避免两场日志混在一起
            self.log_lines.clear();
        }
        // M459：历史视图到此结束（被挡住的那一场收了尾，或者换了一场在跑）⇒ 历史正文不能再
        // 接着写：这一批行属于另一场。**这是新增的一条清行，上面那条判据一字未动。**
        if self.log_view.leave() {
            self.log_lines.clear();
        }
        self.live_session = batch.running.then(|| batch.session.clone());
        self.selected_session = Some(batch.session.clone());
        self.log_lines.extend(batch.lines.iter().cloned());
        if self.log_lines.len() > MAX_LIVE_LINES {
            let drop = self.log_lines.len() - MAX_LIVE_LINES;
            self.log_lines.drain(..drop);
        }
    }
}
