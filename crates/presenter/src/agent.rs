//! AI 助手（D16 双形态）的 presenter 门面：视图模型 + 提交/确认 + 裁决审计。
//!
//! # 1. 这一层为什么单独存在
//!
//! UI 壳（右侧常驻栏 = AGENTRAIL）只做两件事：读 [`AgentRail`] 画、把点击翻成这五个方法。
//! 会话怎么攒、未读怎么算、YOLO 怎么裁决、审计记什么，全在这里与
//! [`crate::state::AppState::apply_agent`]，壳里不重复实现——否则两形态（栏 / 页）会各写一套
//! 而漂移。
//!
//! ```text
//! UI 壳 ──agent_submit/agent_confirm/agent_mark_read──► Presenter ──AgentTransport──► 后端
//!    ▲                                                     │
//!    └────────── agent_rail()（只读视图模型）◄──────────────┘  events → AppState::apply
//! ```
//!
//! # 2. 与 UI 壳的契约（签名已定，改名要先报 tower）
//!
//! ```text
//! Presenter::agent_rail(&self) -> &AgentRail
//! Presenter::agent_submit(&mut self, text: String) -> Result<(), String>
//! Presenter::agent_mark_read(&mut self)
//! Presenter::agent_confirm(&mut self, accept: bool)
//! ```
//!
//! # 3. 铁律落点（AGENTS §6）
//!
//! - **AI 永不接触账户凭据**：事件里没有凭据字段；文本一律过 [`rail::scrub_text`]（第二道防线）。
//! - **写配置受 schema 白名单约束**：白名单在 agent 侧工具层，presenter 只负责「把确认问出来」。
//! - **敏感键分级确认，YOLO 只跳过「提问」且审计照记**：YOLO 不替你点允许，只让 agent 侧少问
//!   一次；[`Presenter::agent_confirm`] 的裁决**只认 `accept`**（M389 修的正是这一点）。
//!
//! # 4. 没做的事（P1 再接）
//!
//! - 真实后端：本模块只认 [`AgentTransport`] 端口，`rig` 那侧由 host 装配时实现；
//! - 落盘审计：agent 侧 `brain::exec` → `audit` 才是权威记录，这里的 [`AuditEntry`] 只是
//!   本进程内、给 UI 看的裁决镜像（`presenter.audit()`）；
//! - 流式时序：端口是同步返回事件分片的（便于单测），真实实现是异步推流。

use crate::dto::{AppEvent, Sensitivity};
use crate::prefs::UiPrefs;
use crate::router::Route;
use crate::state::AppState;

pub mod mock;
pub mod rail;

use rail::scrub_text;
pub use rail::AgentRail;

#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "agent/tests_red_line.rs"]
mod tests_red_line;

/// 裁决记录上限（只留最近若干条；权威审计不在这里）。
pub const MAX_AUDIT_ENTRIES: usize = 256;

/// 一次确认的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmVerdict {
    /// 放行（用户点了允许，或 YOLO 自动放行）
    Approved,
    /// 拒绝（工具调用会带「用户拒绝」回到模型）
    Denied,
}

impl ConfirmVerdict {
    /// 是否放行。
    #[must_use]
    pub fn is_approved(self) -> bool {
        matches!(self, Self::Approved)
    }
}

/// 一条确认裁决的审计记录。
///
/// `yolo` 与 `confirmed` 必须分开记：`yolo = true, confirmed = false`（自动放行）与
/// `yolo = false, confirmed = true`（用户亲手点的允许）在事故复盘里是完全不同的两件事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    /// 请求 id（与 `AgentEvent::ConfirmRequested` 同一个）
    pub request_id: String,
    /// 工具名
    pub tool: String,
    /// 敏感等级
    pub sensitivity: Sensitivity,
    /// 结局
    pub verdict: ConfirmVerdict,
    /// 是否运行在 YOLO 模式
    pub yolo: bool,
    /// 是否经过用户确认（YOLO 下恒为 false）
    pub confirmed: bool,
}

/// agent 后端端口：presenter 不认 `rig` / 异步运行时的具体类型（依赖方向见 crate 文档）。
///
/// 方法同步返回事件分片是刻意的：真实后端在后台线程里跑，事件经
/// [`crate::dto::Backend::drain_events`] 回流；端口这层只表达「一轮输入会产生哪些事件」，
/// 这样单测可以不碰定时器与网络。
pub trait AgentTransport {
    /// 用户提交一句话。
    ///
    /// `history` 是当前 rail 的只读快照（真实后端要靠它拼上下文，mock 靠它算消息下标）。
    /// 返回的事件分片由 [`Presenter`] 顺序 `apply`；`Err` 表示这一轮根本没起来（不产生事件）。
    fn submit(&mut self, text: &str, history: &AgentRail) -> Result<Vec<AppEvent>, String>;

    /// 用户对写操作做了决定（`accept = false` 表示拒绝）。
    fn confirm(&mut self, request_id: &str, accept: bool) -> Vec<AppEvent>;

    /// 事件是不是**稍后**从队列回流的（而不是在 `submit` 里内联返回）。
    ///
    /// 为什么需要这个事实（M384 r2 复审 P2-1）：`agent_submit` 收尾时会按消息重算一次
    /// `streaming`（[`AgentRail::settle_streaming`]）。对**同步内联**的后端这是对的——事件已经
    /// 全在手上；对**异步**后端（`submit` 回空 `Vec`，事件下一拍才从队列回流）这一步会把
    /// [`AgentRail::begin_turn`] 刚点亮的 `busy` **当场灭掉**，于是「提交成功 → 第一个事件到达」
    /// 这段空窗里发送键可用、`Stop` 不出现，再按一次 Enter 就会两轮并行（下标错位、
    /// `MessageDelta` 指到上一轮气泡上）。
    ///
    /// 为什么不在 presenter 里靠「返回的 `Vec` 是不是空」来猜：空 `Vec` 不等于异步——一个
    /// 确实什么都没回的后端也回空 `Vec`，而那种情况必须收敛（见
    /// `agent::tests::submit_with_a_silent_backend_does_not_leave_the_shell_busy`）。
    /// 这是**后端自己才知道的事实**，所以由实现方申报；默认值 `false` 保证既有实现与用例的语义
    /// 一字不改。
    fn is_async(&self) -> bool {
        false
    }

    /// 中断当前这一轮（M388 的「停止」）。
    ///
    /// 返回 `true` = 信号已交给一个**活的**后端（它能在 await 处析构请求，见
    /// `app-iced` 的 `RealTransport`）；`false` = 没有可打断的调用（同步脚本 rig / 内核没在跑
    /// 请求）⇒ 本轮**只能丢弃**。这条区别必须由实现方如实申报：协议没有「一定能打断」这回事。
    ///
    /// 默认 `false`：既有实现（同步 rig / mock）一个都不用改，且它们的诚实答复就是「打不断」。
    fn cancel(&mut self) -> bool {
        false
    }
}

/// AI 助手的 presenter 门面：持有一份 [`AppState`] 与一个后端端口。
///
/// 事件仍走 [`AppState::apply`]（唯一状态变更入口）——[`Presenter::apply`] 只是转发，
/// 不在这一层另做投影。
pub struct Presenter {
    state: AppState,
    transport: Box<dyn AgentTransport>,
    audit: Vec<AuditEntry>,
}

impl std::fmt::Debug for Presenter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Presenter")
            .field("rail", self.agent_rail())
            .field("audit_len", &self.audit.len())
            .finish_non_exhaustive()
    }
}

impl Presenter {
    /// 空状态 + 指定后端端口。
    #[must_use]
    pub fn new(transport: Box<dyn AgentTransport>) -> Self {
        Self::with_state(AppState::new(), transport)
    }

    /// 接管一份已有状态（UI 壳先建 `AppState` 再交给 presenter 时用）。
    #[must_use]
    pub fn with_state(state: AppState, transport: Box<dyn AgentTransport>) -> Self {
        Self {
            state,
            transport,
            audit: Vec::new(),
        }
    }

    /// 换掉后端端口（**不动**会话状态）。
    ///
    /// 为什么需要它（M384 r1 复审 P2-1）：GUI 的「设置 → AI 填完 key 立刻生效」这条语义要求
    /// 能在进程内从脚本化 rig 换到真模型，而不是让用户重启。presenter 不裁决「什么时候能换」
    /// ——那是壳的事；这里只保证 `rail`（会话内容 / 未读 / 待确认）、`prefs`、本进程的裁决
    /// 审计镜像一行不动，所以换完之后用户看到的还是同一个会话，只是下一轮由真后端回答。
    ///
    /// 换进来的端口从**下一轮**开始生效：正在跑的那一轮不会被打断（它的句柄在调用栈里）。
    pub fn agent_replace_transport(&mut self, transport: Box<dyn AgentTransport>) {
        self.transport = transport;
    }

    /// 只读状态（UI 画其它页面用）。
    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// 可写状态——**只在测试构建里存在**。
    ///
    /// 非测试代码没有这个后门：`AppState::ai` 的字段全是 pub，返回 `&mut AppState` 就等于
    /// 让调用方绕开 [`AgentRail::ingest`] 的脱敏与 `busy` 不变量。需要写状态时走下面这些
    /// 窄入口（prefs / 导航 / rail 的那几个字段），rail 的内容一律走四个契约方法。
    #[cfg(test)]
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    /// 导航（一级页替换、子页压栈；去 AI 独立页会展开栏并清未读）。
    pub fn navigate(&mut self, route: Route) {
        self.state.navigate(route);
    }

    /// 回退一层（子页面）；已在根页面时返回 `false`。
    pub fn back(&mut self) -> bool {
        self.state.back()
    }

    /// 折叠/展开 AI 栏（独立页形态下等价于收回侧栏；展开时清未读）。
    pub fn toggle_ai(&mut self) {
        self.state.toggle_ai();
    }

    /// UI 偏好（主题、磁贴尺寸、栏折叠位…）。**不含** rail 字段。
    pub fn prefs_mut(&mut self) -> &mut UiPrefs {
        &mut self.state.prefs
    }

    /// **这是 presenter 层的镜像，不是判据**：判据在 agent 侧（`YoloSwitch`，现读设置文件）。
    /// YOLO 开着时 UI 该调 `agent_confirm(true)`（跳过弹窗，答案仍是「允许」）。
    pub fn agent_set_yolo(&mut self, on: bool) {
        self.state.ai.yolo = on;
    }

    /// 输入框草稿（用户打字时同步进来，切形态不丢）。
    ///
    /// 草稿**是** rail 上唯一能合法躺着凭据原文的字段：它是「还没发出去的字」，写进来时不过
    /// 脱敏（UI 要能原样显示用户正在打的字）。安全性靠另一条：`agent_submit` 脱敏的是**入参**
    /// 而不是草稿，所以原文既不会进会话历史，也不会被发给后端。
    pub fn agent_set_draft(&mut self, text: String) {
        self.state.ai.draft = text;
    }

    /// 喂一条事件（纯转发给 [`AppState::apply`]）。
    pub fn apply(&mut self, event: AppEvent) {
        self.state.apply(event);
    }

    /// 当前会话的视图模型（唯一读入口；两形态共享同一份）。
    #[must_use]
    pub fn agent_rail(&self) -> &AgentRail {
        &self.state.ai
    }

    /// 提交一句话。
    ///
    /// 拒绝的三种情形都不改状态（草稿保留，用户能接着改）：
    /// 空文本、上一轮还在跑、有写操作待确认。
    ///
    /// 成功时清空草稿、立刻把 `busy` 置真（**然后**才投影后端返回的事件分片），最后按消息
    /// 重算一次 `streaming`——异步后端在「提交成功 → 第一个事件到达」之间的空窗因此不会
    /// 让用户再点一次发送（那会产生两条 user 消息、两轮回复）。
    /// 用户气泡**由事件产生**（不在本地乐观回显，见 `lib.rs` §3）。
    pub fn agent_submit(&mut self, text: String) -> Result<(), String> {
        let trimmed = text.trim();
        // 三道闸：与 `agent_submit_ready` 同一组条件（改这里要同时改那里）。
        if trimmed.is_empty() {
            return Err("请输入内容".to_string());
        }
        if self.state.ai.confirm.is_some() {
            return Err("有写操作待确认：先允许或拒绝，再继续对话".to_string());
        }
        if self.state.ai.busy {
            return Err("上一轮回复还没结束".to_string());
        }
        // 用户输入也过脱敏：他可能顺手把 token 粘进对话框，而这段文本会进会话历史、
        // 会作为 prompt **发到模型服务商**、也会进导出。这道是**出站**闸门（rail 侧那道是
        // 显示闸门，两者不能互相替代：删掉这道，界面照样干净，但凭据已经出去了）。
        // 守卫见 agent/tests_red_line.rs::outbound_prompt_is_scrubbed_before_it_reaches_the_backend。
        let text = scrub_text(trimmed);
        // 这个事实要在 `submit` 之前问：端口借的是 `&mut self`（同一句话里不能再借 self）。
        let deferred = self.transport.is_async();
        let events = self
            .transport
            .submit(&text, &self.state.ai)
            // 失败原因同样可能回显 key（provider 的 401 常把 key 抄进 message），
            // 这条 Err 会直接显示在 UI 上，守卫见 outbound_error_text_is_scrubbed。
            .map_err(|error| scrub_text(&error))?;
        self.state.ai.draft.clear();
        self.state.ai.error = None;
        self.state.ai.begin_turn();
        for event in events {
            self.state.apply(event);
        }
        // 同步后端：事件已经在手上了，按消息重算一次（否则会留下假忙）。
        // 异步后端：**保留** `begin_turn()` 点亮的 busy，等第一个事件自己收敛
        // （正文回合在 `MessageFinished`、错误回合在 `Failed → finish_stream`、工具回合在
        // `ToolCallStarted/Finished` 的 `settle_streaming`）。M384 r2 复审 P2-1 的落点。
        if !deferred {
            self.state.ai.settle_streaming();
        }
        Ok(())
    }

    /// 这一句话现在能提交吗（**三道闸**：非空 / 没有在跑 / 没有待确认）。
    ///
    /// [`Presenter::agent_submit`] 的三道闸与它是同一组条件；分开暴露是因为壳有一条**在提交之前**
    /// 就要问的活（M384 r1 P2-1 的自愈：模拟态下换真后端）。壳若自己抄这三个条件，闸门迟早漂；
    /// 用例 `submit_ready_agrees_with_agent_submit` 在 app-iced 侧钉住「两边同真同假」。
    #[must_use]
    pub fn agent_submit_ready(&self, text: &str) -> bool {
        !text.trim().is_empty() && self.state.ai.confirm.is_none() && !self.state.ai.busy
    }

    /// 未读清零（UI 在「用户打开栏 / 停在独立页」时调用）。
    pub fn agent_mark_read(&mut self) {
        self.state.ai.mark_read();
    }

    /// 对当前待确认的写操作做决定。
    ///
    /// - 没有待确认：no-op（不记审计，不 panic）；
    /// - **裁决只看 `accept`**（M389）：YOLO 只决定「要不要问人」，绝不把明确点的拒绝翻成放行；
    /// - 审计镜像里 `yolo` 记**那一刻的环境事实**，`confirmed = !yolo`（自动放行不算「有人回答过」）。
    ///
    /// 两种情形都走同一条事件回流（后端返回的工具收尾事件），所以会话里看到的工具条目
    /// 与审计记录不会各行其是。
    ///
    /// # UI 壳的义务
    ///
    /// 看到 `rail.confirm.is_some()` 就要给出决定：非 YOLO 弹确认框，YOLO 直接调
    /// `agent_confirm(true)`（不弹窗，但**必须**调——否则 `busy` 一直锁着输入）。
    /// 真实后端在 YOLO 下不会发 `ConfirmRequested`（agent 侧 ConfirmGate 已自动放行），
    /// 这条路径是防御性的；另一条收敛途径是后端发 `AgentEvent::ConfirmResolved`。
    pub fn agent_confirm(&mut self, accept: bool) {
        let Some(request) = self.state.ai.take_confirm() else {
            return;
        };
        let yolo = self.state.ai.yolo;
        // 承重：**回答优先**。YOLO 只跳过提问（agent 侧不发请求），不许改写这里收到的回答。
        let verdict = if accept {
            ConfirmVerdict::Approved
        } else {
            ConfirmVerdict::Denied
        };
        self.record(AuditEntry {
            request_id: request.id.clone(),
            tool: request.tool.clone(),
            sensitivity: request.sensitivity,
            verdict,
            yolo,
            confirmed: !yolo,
        });
        let events = self.transport.confirm(&request.id, verdict.is_approved());
        for event in events {
            self.state.apply(event);
        }
    }

    /// 本进程内的裁决记录（最旧在前；权威审计在 agent 侧）。
    #[must_use]
    pub fn audit(&self) -> &[AuditEntry] {
        &self.audit
    }

    /// 中断当前这一轮（M388）：内核能打断就打，打不断也**当场**收尾。返回 `true` = 已通知内核。
    ///
    /// 两条口径：**待确认的写操作先拒掉**（闸门那边的 future 在等这个决定，它不读取消旗标）；
    /// **收尾当场做**（不等后端回声——按了停止还要等几秒才有反应，等于没停）。
    pub fn agent_stop(&mut self, notice: String) -> bool {
        if self.state.ai.confirm.is_some() {
            self.agent_confirm(false);
        }
        let interrupted = self.transport.cancel();
        self.agent_conclude(notice);
        interrupted
    }

    /// 本轮以**一条结论**收尾：清掉流式标记（输入区随之解锁）并落一条系统条目。
    ///
    /// 为什么是系统条目：超时与取消都**不是模型说的话**，写成助手气泡会让用户以为模型答了。
    /// 为什么先 [`AgentRail::finish_stream`]：`begin_turn()` 点亮的是 `rail.streaming` 而不是
    /// 某条消息的标记，只推事件清不掉它——那正是「界面卡在忙碌态、输入框一直禁用」的形状。
    pub fn agent_conclude(&mut self, notice: String) {
        self.state.ai.finish_stream();
        let message = crate::dto::ChatMessage::system(scrub_text(&notice));
        self.state
            .apply(AppEvent::Agent(crate::dto::AgentEvent::MessageAppended(
                message,
            )));
    }

    /// 记一条裁决，超出 [`MAX_AUDIT_ENTRIES`] 时丢最旧的。
    fn record(&mut self, entry: AuditEntry) {
        if self.audit.len() == MAX_AUDIT_ENTRIES {
            self.audit.remove(0);
        }
        self.audit.push(entry);
    }
}
