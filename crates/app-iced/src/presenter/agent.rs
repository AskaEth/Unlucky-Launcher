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
//! ## 两个后端（M384）
//!
//! | 后端 | 什么时候 | 界面怎么知道 |
//! |---|---|---|
//! | [`super::real::RealTransport`] | 设置 → AI 配好了（[`super::real`] 的判据与「测试连接」同一条） | `simulated = false`（四处中性文案） |
//! | [`super::transport::RigTransport`] | 没配好 / 单测 / 取证开关 | `simulated = true`（四处「模拟助手 · 还没接真实模型」）+ 一条具名「未配置」横幅 |
//!
//! **降级是显式的**：没配好时 [`AgentHost::from_env`] 会挂
//! [`AgentFailure::NotConfigured`]（横幅）+ 一行 stderr 日志（说清缺 `ai.` 的哪一项），
//! 而不是让用户对着一屏「像是接上了模型」的空会话发消息。
//!
//! **降级不是终局（M384 r1 复审 P2-1）**：模拟态下每次 [`AgentHost::submit`] 会先重试一次
//! 真后端。用户在设置页填完 key（并保存）之后**下一句话**就由真模型回答，横幅与「模拟」标记
//! 同时消失——不需要重启。重试失败则原样留在模拟态（不把能用的脚本 rig 踢掉）。
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

use std::sync::Arc;

use unlucky_cli::commands::agent::probe::Missing;
use unlucky_i18n::Key;
use unlucky_presenter::agent::{AgentRail, Presenter};
use unlucky_presenter::dto::AppEvent;
use unlucky_presenter::router::{AiPanelMode, Route};

use super::transport::{ConclusionSlot, EventQueue, FailureSlot, RigTransport, Script};

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
    /// 设置 → AI 没配好（缺端点/模型/Anthropic 密钥，或整体禁用）：**真后端没接上**。
    ///
    /// M384 起这条只在**启动期**出现一次（`from_env` 决定降级时挂上），不是每次提交都报：
    /// 用户看到的是「模拟助手 + 一句为什么」，而脚本化 rig 仍然能用（不死路）。
    NotConfigured,
}

impl AgentFailure {
    /// 稳定 ascii 码（日志与测试断言用）。
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::NotImplemented => "not_implemented",
            Self::NotConfigured => "not_configured",
        }
    }

    /// 横幅正文的 catalog 键。
    ///
    /// `NotConfigured` 复用设置页那一条（"什么都没有发出去：配置还不完整。"）：`crates/i18n`
    /// 不在本 mission 的 scope 里，而这句话恰好就是**这里要说的那件事**——一个请求都没发出去，
    /// 因为配置不完整。要一句专门的 agent 文案得放开 i18n（已按 finding 报塔）。
    pub(crate) const fn body_key(self) -> Key {
        match self {
            Self::Unreachable => Key::AgentErrorUnavailable,
            Self::NotImplemented => Key::AgentErrorNotImplemented,
            Self::NotConfigured => Key::SettingsAiTestNotConfigured,
        }
    }
}

/// 「重新解析设置 → 造真后端」那条通路（P2-1 的自愈用）。
///
/// 包一层 newtype 而不是裸 `Arc<dyn Fn>`：函数调用表达式不会对 `Arc<dyn Fn>` 自动解引用，
/// 包个 `call` 让调用点就是普通方法调用。生产是 [`super::real::RealTransport::open`]；
/// 用例把 `launcher.json` 钉在临时目录里（`Plan::at` → `from_plan`）。
#[derive(Clone)]
pub(crate) struct Reopen(
    Arc<dyn Fn() -> Result<super::real::RealTransport, Missing> + Send + Sync>,
);

impl Reopen {
    /// 生产通路：现读设置 → 造真后端。
    pub(crate) fn from_settings() -> Self {
        Self(Arc::new(super::real::RealTransport::open))
    }

    /// 指定设置文件的通路（用例入口）。
    #[cfg(test)]
    pub(crate) fn at_path(path: std::path::PathBuf) -> Self {
        Self(Arc::new(move || {
            super::real::Plan::at(&path).map(super::real::RealTransport::from_plan)
        }))
    }

    fn call(&self) -> Result<super::real::RealTransport, Missing> {
        let f: &dyn Fn() -> Result<super::real::RealTransport, Missing> = &*self.0;
        f()
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
    /// 由 [`AgentHost::from_env`] 的决定填：真模型后端 ⇒ `false`，脚本化 rig ⇒ `true`。
    /// 四处「模拟」措辞会一起消失/出现（标题胶囊 / 副标题 / 输入框占位符 / 空态正文，全部出自
    /// `pages::agent::Disclaimer::of(catalog, simulated)`）。标记不是硬编码在视图里的，它是这个
    /// 事实字段的渲染；「四处一起变」由 `pages/agent/tests/disclaimer.rs` 钉住。
    simulated: bool,
    queue: EventQueue,
    failure: FailureSlot,
    /// 后端报上来的收场事实（M388；目前只有超时。取消是 UI 自己按的，不用它）。
    conclusion: ConclusionSlot,
    /// 最近一次提交失败（具名；下一次成功提交或重试用尽时清掉）。
    last_failure: Option<AgentFailure>,
    /// 上一次写回 presenter 的「栏可见性」(独立页?, 折叠?)，相同就不重复写（`navigate` 有副作用）。
    synced: Option<(bool, bool)>,
    /// 模拟态下重试真后端的通路（`None` = 不重试，单测与取证脚本走这条）。
    reopen: Option<Reopen>,
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
            conclusion: ConclusionSlot::default(),
            last_failure: None,
            synced: None,
            reopen: None,
        }
    }

    /// 接真模型后端（M384）：provider 已经装配好、工具内核与审计都在后台线程里。
    fn with_real(transport: super::real::RealTransport) -> Self {
        let queue = transport.queue();
        let failure = transport.failure();
        let conclusion = transport.conclusion();
        Self {
            presenter: Presenter::new(Box::new(transport)),
            // 真模型 ⇒ 一处「模拟」措辞都不该留（见 `simulated` 字段文档）
            simulated: false,
            queue,
            failure,
            conclusion,
            last_failure: None,
            synced: None,
            // 真后端不需要重试；留着通路只是为了字段形状统一
            reopen: Some(Reopen::from_settings()),
        }
    }

    /// 进程启动时的后端选择：**配好了接真模型，没配好显式降级**。
    ///
    /// 判据是 `AiSettings::to_config()`（与设置页「测试连接」同一条，见 [`super::real`]），
    /// 不是「有没有 api_key」：本地 llama.cpp / Ollama 本来就不要密钥。
    /// 降级时挂 [`AgentFailure::NotConfigured`] 并把缺哪一项打进日志——**不静默**。
    pub(crate) fn from_env() -> Self {
        let reopen = Reopen::from_settings();
        Self::from_choice(reopen.call(), Some(reopen))
    }

    /// 决策本体（与 [`Self::from_env`] 分开：用例从 [`Plan::at`](super::real::Plan) 喂一份
    /// 临时 `launcher.json`，走的是**同一条**分支，不手搓状态——复审 r1 的 #8）。
    fn from_choice(
        choice: Result<super::real::RealTransport, Missing>,
        reopen: Option<Reopen>,
    ) -> Self {
        match choice {
            Ok(transport) => {
                eprintln!("[agent] 已接真模型后端（设置 → AI 的那一份配置）");
                Self::with_real(transport)
            }
            Err(missing) => {
                eprintln!(
                    "[agent] 未配置真模型（缺 ai.{}）⇒ AI 栏降级为脚本化 rig，界面常驻「模拟」标记",
                    missing.as_str()
                );
                let mut host = Self::with_script(Script::from_env());
                host.last_failure = Some(AgentFailure::NotConfigured);
                host.reopen = reopen;
                host
            }
        }
    }

    /// 用例入口：把「设置页保存」变成一次文件写入（临时目录里的 `launcher.json`）。
    #[cfg(test)]
    pub(crate) fn from_settings_path(path: &std::path::Path) -> Self {
        let reopen = Reopen::at_path(path.to_path_buf());
        Self::from_choice(reopen.call(), Some(reopen))
    }

    /// 模拟态下重试一次真后端（M384 r1 复审 P2-1）。
    ///
    /// 为什么要它：AI 的四项配置在启动时读一次，用户在设置页填完 key 之后的**下一次提交**
    /// 必须能用真模型回答——否则任务书给的那条验收路径（填 key → 发一句 → 真模型回）在同一次
    /// 运行里不成立，而启动期那条「配置还不完整」的横幅此时说的是过期事实。
    ///
    /// 三条口径：
    /// - **只在模拟态重试**：真后端不需要也不该每句话都重读设置、重建线程；
    /// - **失败就原样留在模拟态**：读不懂的配置不该把用户从能用的脚本 rig 上踢下去；
    /// - **成功就换端口 + 清横幅 + 摘掉「模拟」**：三件事一起做，界面不会出现「真后端 + 模拟标记」。
    ///
    /// **三个共享槽都要换**（复审 r1 P2-1）：`queue` / `failure` / `conclusion`。漏掉 `conclusion`
    /// 的后果不是「少一条日志」——worker 会把超时写进**没人读的旧槽**，于是这一轮既没有结论也没有
    /// 事件，`rail.streaming` 停在 `begin_turn()` 点亮的 `true`，**输入区一直禁用**。那正是本 mission
    /// 要消灭的「静默挂住」，只是入口换成了「启动时没配好、后来填上了」。
    fn retry_real_backend(&mut self) {
        if !self.simulated {
            return;
        }
        let Some(reopen) = self.reopen.clone() else {
            return;
        };
        let Ok(transport) = reopen.call() else {
            return;
        };
        let queue = transport.queue();
        let failure = transport.failure();
        let conclusion = transport.conclusion();
        self.presenter.agent_replace_transport(Box::new(transport));
        self.queue = queue;
        self.failure = failure;
        self.conclusion = conclusion;
        self.simulated = false;
        self.last_failure = None;
        eprintln!("[agent] 设置已配好 ⇒ 本次提交起切到真模型后端（无需重启）");
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
    ///
    /// **模拟态先自愈**：启动时没配好（脚本化 rig），但用户可能刚在设置页填完 key。
    /// 所以在真正提交之前重试一次真后端（[`Self::retry_real_backend`]）——成功则这句话就由
    /// 真模型回答，用户不必重启（M384 r1 复审 P2-1）。
    pub(crate) fn submit(&mut self, text: String) -> SubmitOutcome {
        // 自愈放在**三道闸都放行之后**（复审 r2 附带项）：否则「有待确认 / 上一轮还在跑」时按
        // Enter，也会先把后端换掉，rail 上那条待确认随后被递给一个不认识它的新 transport。
        // 自愈窗口没有变窄：能走到这里的正常情形（空闲 + 无待确认 + 非空文本）与从前完全一样。
        if self.presenter.agent_submit_ready(&text) {
            self.retry_real_backend();
        }
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
mod tests;

mod stop;
