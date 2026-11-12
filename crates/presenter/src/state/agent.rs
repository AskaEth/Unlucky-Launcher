//! AI 栏的事件投递与可见性判定（M223 从 `state.rs` **纯搬移**，一行未改）。
//!
//! 为什么搬：M223 要往 `AppState` 加「导入已有版本」的状态字段与一条 `CoreEvent::Adopt` 分支，
//! 而 `state.rs` 当时正好 400 行（零余量，AGENTS §3.9）。拆分口径与 [`super::tiles`] 一致：
//! **纯搬移、一行不改**；`impl AppState` 写在子模块里，方法路径
//! `AppState::apply_agent` / `AppState::agent_rail_visible` 对调用方完全不变
//! （子模块看得见父模块的私有字段，所以这里直接用 `self.ai` / `self.prefs`）。

use crate::dto::AgentEvent;
use crate::router::AiPanelMode;

use super::AppState;

impl AppState {
    /// agent 事件 → AI 栏视图模型（投影规则见 [`crate::agent::rail`]）。
    ///
    /// 未读规则：栏不可见时，助手侧新内容（助手/工具消息、错误、待确认请求）逐条累加 `unread`；
    /// 用户自己说的话与纯增量不算。栏可见时到达的内容直接算已读（红点不该在眼前亮着）。
    pub fn apply_agent(&mut self, event: AgentEvent) {
        let visible = self.agent_rail_visible();
        let fresh = self.ai.ingest(&event);
        if fresh {
            if visible {
                self.ai.mark_read();
            } else {
                self.ai.unread = self.ai.unread.saturating_add(1);
            }
        }
    }

    /// AI 栏此刻是否「在用户眼前」。
    ///
    /// 独立页形态（栈顶是 `Route::Agent`）必然可见；停靠形态下只有收成 icon 轨才算离开——
    /// 右侧常驻栏在任何一级页面都挂着，用户扫一眼就能看到。
    #[must_use]
    pub fn agent_rail_visible(&self) -> bool {
        self.ai_mode() == AiPanelMode::Page || !self.prefs.ai_collapsed
    }
}
