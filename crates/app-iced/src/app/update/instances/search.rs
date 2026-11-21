//! 实例页**搜索框**的写端（M457）：`App::instance_query` 的两条消息 + 谁清它。
//!
//! ## 这一卷为什么单独存在
//!
//! `app/update/instances.rs` 是启动参数表单 / MODS / SAVES 的落点（205 行），而搜索的规矩不止
//! 「改一个字段」这一条：还有两条**清理时机**要写清楚（见下）。与 `view_mode.rs` 同一种拆法。
//!
//! ## 关键词住在哪、谁写、谁读
//!
//! | 用户动作 | 写哪 | 谁读 |
//! |---|---|---|
//! | 在药丸里打字 | `App::instance_query`（**不落盘**） | `app/view/instances.rs::view_mode` → 页面 |
//! | 点药丸右边的叉 / 空态的主按钮 | 同上（清空） | 同上 |
//! | 导航轨离开实例页 / 页面内跳到别的页 | 同上（清空） | 同上 |
//!
//! ## 为什么这两条消息排在 `selected_instance_id()` 守卫**之前**
//!
//! 与 M455 的视图 chip 同一个理由：搜索框在**列表页**（没选中实例）上也要能用，排在守卫后面
//! 会变成「打字没反应」。接线在 `on_instance_message` 的第一段。
//!
//! ## 谁清它（塔点名要求写死的口径）
//!
//! 1. **切视图 / 进详情 / 从详情返回：不清**。同一屏上的两种摆放、同一份「我在找什么」；
//!    药丸上写着关键词，用户看得见它还在生效。
//! 2. **离开实例页：清**（[`App::leave_instances_if_elsewhere`]）。不这样就变成「从别的页回来
//!    还带着一个过滤条件、列表只剩两行」——那种状态没有可见的入口，是最坏的一种。
//! 3. 出图开关 `ULL_START_ROUTE` / `ULL_MOTION_FIXTURE` 在启动当拍直接改路由（不是用户导航），
//!    不经过这里；它们整进程只跑一次，且关键词由 `ULL_INSTANCE_QUERY` 显式给，无需清理。

use iced::Task;
use unlucky_presenter::router::Route;

use crate::app::state::App;
use crate::app::Message;
use crate::pages::InstanceMessage;

impl App {
    /// 实例页的一条消息里，属于**搜索框**的那两支（别的一律返回 `None`，由调用方继续分发）。
    pub(super) fn on_instance_search(
        &mut self,
        message: &InstanceMessage,
    ) -> Option<Task<Message>> {
        match message {
            InstanceMessage::Search(text) => {
                // 值没变就什么都不做：`text_input` 每次事件都推一份同样的串时，不白写一次
                // （也就不会因为「`search_bar` 的受控值」与 `App` 的字段来回打架而抖动）。
                if self.instance_query != *text {
                    self.instance_query.clone_from(text);
                }
                Some(Task::none())
            }
            InstanceMessage::ClearSearch => {
                self.instance_query.clear();
                Some(Task::none())
            }
            _ => None,
        }
    }

    /// 路由跳到**别的一级页**时清掉实例页的搜索关键词（见模块文档「谁清它」第 2 条）。
    ///
    /// 判据是「目标还属不属于实例页」：`Route::Instances` 与 `Route::Detail(_)` 都属于
    /// （详情是实例页压的一层），其余一律算离开。
    ///
    /// M461：这条规矩的**真正落点**已经挪到 [`crate::app::state::App::refresh_view`]——那里是
    /// 「路由变了」的唯一漏斗，于是 `Route::Agent` / `Route::Content` / 弹窗里「去设置」
    /// 那几条直接 `state.navigate` 的通路也一并覆盖（M457 只挂了导航轨与页面内 `Navigate` 两处）。
    /// 本函数留给还在早一步调它的调用点（`update/page_messages.rs`），判据走同一份 [`belongs_to_instances`]。
    pub(in crate::app) fn leave_instances_if_elsewhere(&mut self, route: &Route) {
        if !belongs_to_instances(route) {
            self.instance_query.clear();
        }
    }
}

/// 这条路由还算不算「在实例页上」（= 关键词要留着）。
///
/// `Route::Detail(_)` 算：详情是实例页压的一层，从详情返回列表时用户还在找同一批东西。
/// 其余一律算离开——包括 `Route::Agent`（AI 独立页）与 `Route::Content`。
///
/// 两处判据**共用这一个函数**（`refresh_view` 与 `leave_instances_if_elsewhere`）：
/// 写成两份的那天，「哪个通路清、哪个不清」就又变成两张会对不上的表。
pub(in crate::app) fn belongs_to_instances(route: &Route) -> bool {
    matches!(route, Route::Instances | Route::Detail(_))
}
