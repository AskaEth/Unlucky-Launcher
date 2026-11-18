//! 「启动被拦下」的事件态（M331），从 `state.rs` 拆出：那边贴着 400 行硬线。
//!
//! 与 `install_cancelled` / `launch_ends` 同一条纪律：presenter **只发信号**（次数 + 结构），
//! 文案由 app 层按 i18n key 现求。这里存的是「第几次拦下」与「这一次的事实」——界面拿它
//! 决定弹不弹框、弹哪个实例的框。
//!
//! `AppState::running()` 也搬到这里：它读的是 `instances` 的运行时态，与拦下/启动同一条线，
//! 搬家只是为了让 `state.rs` 腾出行数（逐行 1:1，没有行为变化）。

use crate::dto::{InstanceRuntime, InstanceSummary, LaunchBlockedView};

use super::AppState;

/// 启动被拦下的次数与最近一次的事实。
///
/// 为什么用「次数 + 最近一条」而不是队列：模态框一次只显示一条（[`crate::dialogs::DialogQueue`]
/// 的语义），界面靠次数变化判「又拦了一次」，靠结构取「这一次是哪一条实例、缺什么 Java」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchGuard {
    /// 拦下次数（每次 `CoreEvent::LaunchBlocked` +1）：界面比对前后值决定要不要弹框。
    pub blocks: u64,
    /// 最近一次拦下的结构（哪条实例、什么方向、需要什么 / 现在是什么）。
    pub last: Option<Box<LaunchBlockedView>>,
}

impl LaunchGuard {
    /// 记一次拦下。
    pub fn note(&mut self, blocked: LaunchBlockedView) {
        self.blocks = self.blocks.saturating_add(1);
        self.last = Some(Box::new(blocked));
    }
}

impl AppState {
    /// 运行中的实例（实例页顶部迷你状态条）。
    pub fn running(&self) -> Vec<&InstanceSummary> {
        self.instances
            .iter()
            .filter(|instance| {
                matches!(
                    instance.runtime,
                    InstanceRuntime::Running { .. } | InstanceRuntime::Busy { .. }
                )
            })
            .collect()
    }
}
