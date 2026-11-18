//! 实时日志的滚动窗口（从 `state.rs` 搬出，守 400 行硬线）。
//!
//! 纯搬移：常量与 `append_live_lines` 一字未改；`state.rs` 用 `pub use` 再导出
//! [`MAX_LIVE_LINES`]，老路径 `crate::state::MAX_LIVE_LINES` 继续可用。

use super::AppState;

/// 实时日志最多保留的行数（滚动窗口；更多行由日志页的筛选/历史会话承担）。
pub const MAX_LIVE_LINES: usize = 2_000;

impl AppState {
    /// 实时日志增量：只追加，封顶 [`MAX_LIVE_LINES`] 行（滚动窗口）。
    pub(super) fn append_live_lines(&mut self, batch: &crate::dto::LiveLogBatch) {
        if self
            .live_session
            .as_deref()
            .is_some_and(|session| session != batch.session)
        {
            // 换了一个会话在跑：清掉上一场的行，避免两场日志混在一起
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
