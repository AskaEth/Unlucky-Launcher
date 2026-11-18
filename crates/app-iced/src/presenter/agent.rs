//! AI 助手栏的 presenter 句柄（M119）：四个契约 + 事件队列 + 具名错误。
//!
//! ## 它替掉的是什么
//!
//! M104 交付的壳是**自带样本会话**的（`pages/agent/demo.rs` 的 `transcript()`，`Submit` 只往
//! 本地数组里追加一条「没发给模型」的提示）。M119 把这些全部删掉：会话内容的唯一来源是
//! [`Presenter::agent_rail`]，壳里的 `Submit` / `Stop` / `Confirm` / 标记已读**全部**翻成
//! presenter 的那四个签名——**壳里一行会话状态都不留**。
//!
//! | 壳里的动作 | 落到哪 |
//! |---|---|
//! | 输入框打字 | [`AgentHost::set_draft`] → `Presenter::agent_set_draft`（切形态不丢） |
//! | 发送 | [`AgentHost::submit`] → `Presenter::agent_submit` |
//! | 打开栏 / 停在独立页 | [`AgentHost::mark_read`] → `Presenter::agent_mark_read` |
//! | 允许/拒绝写操作 | [`AgentHost::confirm`] → `Presenter::agent_confirm` |
//! | 事件回流 | [`AgentHost::apply`] → `Presenter::apply`（唯一状态写入口） |
//! | 栏可见性 | [`AgentHost::sync_visibility`] → `Presenter::navigate` + `prefs_mut` |
//!
//! ## 为什么句柄里还要持一份队列与失败槽
//!
//! presenter 不持有定时器，也不认识 iced（crate 文档 §4）。异步后端的事件只能落在**进程内队列**
//! 里由 UI 抽；失败原因同理（`AgentTransport::submit` 只回一个 `String`，而 UI 要按**原因码**
//! 选 i18n 文案，不能去匹配错误串）。两个 `Arc<Mutex<..>>` 就这两件东西，和
//! [`super::transport::RigTransport`] 共享，**transport 只写、UI 只读**。
//!
//! ## 未读口径为什么依赖两行同步
//!
//! presenter 判「栏在用户眼前」用的是它自己那份 `router` + `prefs.ai_collapsed`
//! （`AppState::agent_rail_visible`），而路由与档位的真源在 `App`。所以每次 `refresh_view`
//! 都把这两个事实按**窄入口**（`navigate` / `prefs_mut`）写回去——这是 presenter 目前对
//! 「栏可见吗」唯一开放的接口。更好的形状是给 presenter 加一个 `agent_set_visible(bool)`
//! 窄入口（已按 finding 报给塔台），那样这两行同步就可以删。

use unlucky_i18n::Key;
use unlucky_presenter::agent::{AgentRail, Presenter};
use unlucky_presenter::dto::AppEvent;
use unlucky_presenter::router::{AiPanelMode, Route};

use super::transport::{EventQueue, FailureSlot, RigTransport, Script};

/// 一个**具名**的失败原因（UI 据码选 catalog 文案；后端诊断原文只进日志）。
///
/// 为什么不是 `String`：M114 复审 r1 的 p1 就是这么来的——后端诊断是中文人话，直接上屏会让
/// 英文界面里出现整句中文。原因码是 ascii 且不随语言变（日志 / 测试 / 用户报障都靠它对齐）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentFailure {
    /// 端点不可达（连接失败 / 超时）。
    Unreachable,
    /// 这条请求的路由还没接线（真实 LLM 后端未装配）。
    NotImplemented,
}

impl AgentFailure {
    /// 稳定 ascii 码（日志与测试断言用）。
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::NotImplemented => "not_implemented",
        }
    }

    /// 横幅正文的 catalog 键。
    pub(crate) const fn body_key(self) -> Key {
        match self {
            Self::Unreachable => Key::AgentErrorUnavailable,
            Self::NotImplemented => Key::AgentErrorNotImplemented,
        }
    }
}

/// 一次提交的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubmitOutcome {
    /// 交给后端了（事件稍后从队列里回来）。
    Sent,
    /// 后端明确失败：UI 要显示具名错误。
    Failed(AgentFailure),
    /// presenter 拒收（空文本 / 上一轮还在跑 / 有写操作待确认）。
    ///
    /// 不进横幅：这三种情形正常情况下**够不着**（发送键在空白/忙碌时是禁用的，确认框没关时
    /// 输入区也不该能发），走到这里说明某处闸门漏了 ⇒ 留一行日志，别拿一句话糊在界面上。
    Rejected,
}

/// AI 助手栏的 presenter 句柄。
pub(crate) struct AgentHost {
    presenter: Presenter,
    /// 这套后端是不是**模拟**的（M119 r1 / 复审 P2-1）。
    ///
    /// 今天恒 `true`：transport 只有 `RigTransport` 一种（脚本化，没有真模型）。接真 `AgentTransport`
    /// 的那一天，这里传 `false`——**四处「模拟」措辞会一起消失**（标题胶囊 / 副标题 / 输入框占位符 /
    /// 空态正文，全部出自 `pages::agent::Disclaimer::of(catalog, simulated)`）。
    /// 标记不是硬编码在视图里的，它是这个事实字段的渲染；「四处一起变」由
    /// `pages/agent/tests/disclaimer.rs` 钉住（r1 时只有一处认它，那句声明是空的）。
    simulated: bool,
    queue: EventQueue,
    failure: FailureSlot,
    /// 最近一次提交失败（具名；下一次成功提交或重试用尽时清掉）。
    last_failure: Option<AgentFailure>,
    /// 上一次写回 presenter 的「栏可见性」(独立页?, 折叠?)，相同就不重复写（`navigate` 有副作用）。
    synced: Option<(bool, bool)>,
}

impl AgentHost {
    /// 指定脚本（单测与取证开关都走它）。
    pub(crate) fn with_script(script: Script) -> Self {
        let transport = RigTransport::new(script);
        let queue = transport.queue();
        let failure = transport.failure();
        Self {
            presenter: Presenter::new(Box::new(transport)),
            // 脚本化 rig ⇒ 一定是模拟的（见字段文档）
            simulated: true,
            queue,
            failure,
            last_failure: None,
            synced: None,
        }
    }

    /// 按 `ULL_AGENT_SCRIPT` 选脚本（没设 = `qa`）。
    pub(crate) fn from_env() -> Self {
        Self::with_script(Script::from_env())
    }

    /// 当前会话视图模型（**唯一**读入口；两形态共享同一份）。
    pub(crate) fn rail(&self) -> &AgentRail {
        self.presenter.agent_rail()
    }

    /// 这套后端是不是模拟的（界面据此常驻「模拟」标记；见字段文档）。
    pub(crate) fn is_simulated(&self) -> bool {
        self.simulated
    }

    /// 最近一次提交失败的具名原因。
    pub(crate) fn last_failure(&self) -> Option<AgentFailure> {
        self.last_failure
    }

    /// 抽干事件队列（UI 在心跳里调；返回的事件**只**经 `Message::Agent(Events)` 回流）。
    pub(crate) fn drain(&mut self) -> Vec<AppEvent> {
        let mut drained = Vec::new();
        if let Ok(mut queue) = self.queue.lock() {
            drained.extend(queue.drain(..));
        }
        drained
    }

    /// 喂一条事件（纯转发给 `AppState::apply`）。
    pub(crate) fn apply(&mut self, event: AppEvent) {
        self.presenter.apply(event);
    }

    /// 提交一句话。
    pub(crate) fn submit(&mut self, text: String) -> SubmitOutcome {
        match self.presenter.agent_submit(text) {
            Ok(()) => {
                self.last_failure = None;
                SubmitOutcome::Sent
            }
            Err(detail) => {
                // presenter 已经把错误串脱敏（`agent/tests_red_line.rs` 钉住），这里只进日志。
                eprintln!("[agent] submit 失败：{detail}");
                match self.failure.lock().ok().and_then(|mut slot| slot.take()) {
                    Some(failure) => {
                        self.last_failure = Some(failure);
                        SubmitOutcome::Failed(failure)
                    }
                    None => SubmitOutcome::Rejected,
                }
            }
        }
    }

    /// 未读清零（用户打开栏 / 停在独立页）。
    pub(crate) fn mark_read(&mut self) {
        self.presenter.agent_mark_read();
    }

    /// 对当前待确认的写操作做决定。
    pub(crate) fn confirm(&mut self, accept: bool) {
        self.presenter.agent_confirm(accept);
    }

    /// 输入框草稿（用户打字时同步进来，切形态不丢）。
    pub(crate) fn set_draft(&mut self, text: String) {
        self.presenter.agent_set_draft(text);
    }

    /// 把「用户此刻看得见这段会话吗」写回 presenter（未读口径的输入）。
    ///
    /// 只在两个事实变化时写：`Presenter::navigate(Route::Agent)` 会顺手展开栏偏好并清未读，
    /// 每次 `refresh_view` 都写一遍会把用户攒的未读吃掉。
    pub(crate) fn sync_visibility(&mut self, on_agent_page: bool, collapsed: bool) {
        if self.synced == Some((on_agent_page, collapsed)) {
            return;
        }
        self.synced = Some((on_agent_page, collapsed));
        match (on_agent_page, self.presenter.state().ai_mode()) {
            (true, AiPanelMode::Docked) => self.presenter.navigate(Route::Agent),
            (false, AiPanelMode::Page) => self.presenter.navigate(Route::Instances),
            _ => {}
        }
        // 顺序不能反：`navigate(Route::Agent)` 内部会把 `ai_collapsed` 置 false。
        self.presenter.prefs_mut().ai_collapsed = collapsed;
    }

    /// 本进程内的裁决记录条数（取证/测试用；权威审计在 agent 侧）。
    #[cfg(test)]
    pub(crate) fn audit_len(&self) -> usize {
        self.presenter.audit().len()
    }
}

impl std::fmt::Debug for AgentHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AgentHost")
            .field("rail", self.rail())
            .field("last_failure", &self.last_failure)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_failure_code_is_stable_ascii_and_has_its_own_body_key() {
        assert_eq!(AgentFailure::Unreachable.code(), "unreachable");
        assert_eq!(AgentFailure::NotImplemented.code(), "not_implemented");
        assert_ne!(
            AgentFailure::Unreachable.body_key(),
            AgentFailure::NotImplemented.body_key(),
            "两种失败要两句话，不能都糊成一句"
        );
    }

    #[test]
    fn a_submit_only_reaches_the_backend_once_per_turn_while_busy() {
        let mut host = AgentHost::with_script(Script::Streaming);
        assert_eq!(host.rail().messages.len(), 0);
        assert_eq!(host.submit("hello".to_string()), SubmitOutcome::Sent);
        // 队列里的这一批要经 `apply` 才进 rail（app 层由 `Message::Agent(Events)` 做同一件事）
        for event in host.drain() {
            host.apply(event);
        }
        assert!(host.rail().busy, "流式没收敛前输入要禁用");
        // 第二轮被 presenter 拒收（不是服务器错误，所以不进横幅）
        assert_eq!(host.submit("again".to_string()), SubmitOutcome::Rejected);
        assert_eq!(host.last_failure(), None);
    }

    #[test]
    fn an_unreachable_endpoint_is_named_not_stringified() {
        let mut host = AgentHost::with_script(Script::Unreachable);
        assert_eq!(
            host.submit("hello".to_string()),
            SubmitOutcome::Failed(AgentFailure::Unreachable)
        );
        assert_eq!(host.last_failure(), Some(AgentFailure::Unreachable));
        assert!(host.drain().is_empty());
    }

    #[test]
    fn visibility_is_written_back_once_per_change() {
        let mut host = AgentHost::with_script(Script::Qa);
        host.sync_visibility(false, true);
        host.sync_visibility(false, true);
        assert!(host.presenter.state().prefs.ai_collapsed);
        host.sync_visibility(true, false);
        assert_eq!(host.presenter.state().ai_mode(), AiPanelMode::Page);
        assert!(!host.presenter.state().prefs.ai_collapsed);
        host.sync_visibility(false, false);
        assert_eq!(host.presenter.state().ai_mode(), AiPanelMode::Docked);
    }
}
