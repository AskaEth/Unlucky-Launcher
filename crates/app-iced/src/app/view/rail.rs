//! 导航轨底部**账户头像位**的状态换算（M114）。
//!
//! 为什么单独一个文件：`app/view.rs` 贴着 400 行硬线，而这块逻辑与页面树无关——
//! 它只回答「轨底那个头像位现在该显示成未登录，还是某个账户」。

use crate::shell;

use super::super::state::App;

/// 导航轨收成「图标条」的窗口高度（M114）。
///
/// 判据不是「项数」而是**真实可用高度**：四项 + 底部头像位在 48 标题栏之上还要放得下
/// 4 × 52 + 4 × 4 + 52 ≈ 300；留出一屏正文的最小高度（424 行宽 + 页头 + 边距），
/// 低于 720 就把标签收掉，只留图标与头像位（头像位**永远可见**，任务要求）。
const RAIL_COMPACT_HEIGHT: f32 = 720.0;

impl App {
    /// 导航轨底部头像位的状态（M114）：未登录 / 已登录两态要一眼能分。
    pub(super) fn rail_account(&self) -> shell::RailAccount {
        let compact = self.height < RAIL_COMPACT_HEIGHT;
        match self.state.active_account() {
            Some(account) => {
                shell::RailAccount::signed_in(account.monogram(), account.name.clone())
                    .compact(compact)
            }
            // M106：未登录标签跟着**生效语言**走（早期钉死 `Lang::En` 的写法已删）。
            None => shell::RailAccount::signed_out(
                self.lang()
                    .catalog()
                    .text(unlucky_i18n::Key::AccountRailSignedOut),
            )
            .compact(compact),
        }
    }
}
