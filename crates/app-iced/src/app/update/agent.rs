//! 助手栏消息的落点（M104 建，M119 接线）：三档状态迁移 / 输入 / 键盘 / **presenter 四契约**。
//!
//! ## 为什么单独一个文件
//!
//! `app/update.rs` 已经贴着 400 行硬上限（M94 也因此把下载页四支拆到 `update/download.rs`）。
//! 助手栏有十几个变体，塞回主文件必破线。
//!
//! ## 分工：视图态 vs presenter 契约（M119 之后的分界线）
//!
//! | 消息 | 落到哪 |
//! |---|---|
//! | `Expand` / `Collapse` / `Dock` / `Escape` | **只改视图态**（`AgentUi`）+ 路由栈（离开/进入 `Route::Agent`）+ `agent_mark_read` |
//! | `Detach` | 视图态 + `Route::Agent`（压栈，`back()` 能回去）+ `agent_mark_read` |
//! | `Edit` | 视图态（草稿），并同步进 presenter 的草稿（`agent_set_draft`） |
//! | `Submit` | [`AgentHost::submit`] → `agent_submit`；失败**不留白**（具名错误进横幅） |
//! | `Stop` | `AppCommand::StopAgent`（真后端才收敛得出结果；同步 rig 下是 no-op） |
//! | `Confirm(accept)` | [`AgentHost::confirm`] → `agent_confirm`（YOLO 下 presenter 自己放行并记审计） |
//! | `Events(..)` | [`AgentHost::apply`] → `AppState::apply`（**事件回流的唯一落点**） |
//! | `DragStart` / `DragTo` / `DragEnd` | 视图态：吸附宽度 |
//! | `Key` / `Tab` / `Activate` | 键盘路径（`Tab` 的环要在流式中多一格，`busy` 从 rail 取） |
//!
//! **会话内容一行都不在这里攒**：`transcript` / `busy` / `unread` / `confirm` 全在 presenter 的
//! `AgentRail` 上（M119 删掉了 M104 的自持副本）。
//!
//! ## 事件为什么绕一圈消息再回来
//!
//! `agent_submit` / `agent_confirm` 的端口是同步的，但**事件走队列**（见
//! `app/state/presenter/transport.rs`）：抽队列的那一步必须和用户点击走同一条漏斗，否则两处各改
//! 一次状态，流式标记与未读会打架。所以 [`App::drain_agent`] 抽出来之后立刻
//! `on_agent(agent::Message::Events(..))` —— 同一条 `Message::Agent(..)` 通道。

use crate::app::diag::diag;
use iced::keyboard::{key, Key};
use iced::Task;

use unlucky_i18n::{Catalog, Key as TextKey};
use unlucky_presenter::dto::AppCommand;

use crate::app::MIN_SIZE;
use crate::app::WINDOW_SIZE;
use crate::shell::agent::demo::{self, Fixture};
use crate::shell::agent::{self, Focus, RailState};

use super::super::state::presenter::agent::SubmitOutcome;
use super::*;

#[cfg(test)]
mod tests;

impl App {
    /// 助手栏消息（`app::Message::Agent(..)` 的唯一落点）。
    pub(super) fn on_agent(&mut self, message: agent::Message) -> Task<Message> {
        // M106 口径：文案走**生效语言**的目录，不再恒英文
        let catalog = Catalog::of(self.lang());

        // 只有**几何**会变的几支才需要重建页面模型（`pages::Model` 里没有会话内容：会话在
        // presenter 的 rail 上，右栗自己那棵树每次 `view()` 都从 rail 现读）。复审 P3-9 的教训：
        // 每敲一个字符都重建整张模型是白克隆。
        let geometry_changed = matches!(
            message,
            agent::Message::Expand
                | agent::Message::Collapse
                | agent::Message::Detach
                | agent::Message::Dock
                | agent::Message::DragTo(_)
                | agent::Message::DragEnd
                | agent::Message::Escape
                | agent::Message::Activate
                | agent::Message::Key(_)
        );

        match message {
            agent::Message::Expand => {
                self.agent.expand();
                // 「展开即读」是 presenter 的语义（`agent_mark_read`），壳只负责在正确的时刻调它
                self.agent_host.mark_read();
            }
            agent::Message::Collapse | agent::Message::Escape => self.collapse_agent(),
            agent::Message::Detach => {
                self.agent.detach();
                self.agent_host.mark_read();
                self.state.navigate(Route::Agent);
            }
            agent::Message::Dock => {
                self.agent.dock();
                self.agent_host.mark_read();
                self.leave_agent_page();
            }
            agent::Message::DragStart => self.agent.drag_start(),
            agent::Message::DragTo(x) => self.agent.drag_to(self.width, x),
            agent::Message::DragEnd => self.agent.drag_end(),
            agent::Message::Edit(action) => {
                self.agent.edit(action);
                // 草稿同步给 presenter（切形态不丢；它也是 rail 上唯一允许躺原文的字段）
                self.agent_host.set_draft(self.agent.draft.text());
            }
            agent::Message::Submit => self.submit_agent(),
            agent::Message::Stop => self.dispatch(AppCommand::StopAgent),
            agent::Message::Confirm(accept) => {
                self.agent_host.confirm(accept);
                self.drain_agent();
            }
            agent::Message::Events(events) => {
                for event in events {
                    self.agent_host.apply(event);
                }
                self.view_dirty = true;
            }
            agent::Message::Tab { shift } => {
                let rail = self.agent_host.rail();
                let (busy, pending) = (rail.busy, rail.confirm.is_some());
                self.agent.step_focus(shift, busy, pending);
            }
            agent::Message::Activate => self.activate_focused(catalog),
            agent::Message::Key(event) => return self.on_agent_key(event),
        }

        // 待确认请求出现/消失时，把键盘焦点挪到（或挪离）确认条的按钮上
        self.sync_confirm_focus();
        // 档位 / 路由变了就要把「栏可见吗」写回 presenter（未读口径的输入）
        self.sync_agent_presenter();
        if geometry_changed {
            self.view_dirty = true;
            self.refresh_view();
        }
        Task::none()
    }

    /// 发送：草稿 → `agent_submit` → 抽事件。
    ///
    /// 三种结局各自有落点：发出去（清草稿）/ 后端失败（具名横幅，见 [`App::agent_failure`]）/
    /// presenter 拒收（日志一行——这三种情形正常情况下被禁用态挡住，够不着）。
    fn submit_agent(&mut self) {
        let text = self.agent.draft.text();
        if text.trim().is_empty() {
            return;
        }
        match self.agent_host.submit(text) {
            SubmitOutcome::Sent => {
                self.agent.clear_draft();
                self.agent_host.set_draft(String::new());
                self.drain_agent();
            }
            SubmitOutcome::Failed(failure) => {
                diag!("[agent] 提交失败（{}）", failure.code());
            }
            SubmitOutcome::Rejected => {
                diag!("[agent] presenter 拒收：空文本 / 上一轮未完 / 待确认 三闸该先挡住");
            }
        }
    }

    /// 抽干 presenter 的事件队列，**经既有 Message 通道**回流（见模块文档末节）。
    pub(crate) fn drain_agent(&mut self) {
        let events = self.agent_host.drain();
        if events.is_empty() {
            return;
        }
        let _ = self.on_agent(agent::Message::Events(events));
    }

    /// 把「栏可见吗」写回 presenter（`navigate` + `prefs_mut` 两个窄入口，见 `AgentHost`）。
    pub(crate) fn sync_agent_presenter(&mut self) {
        let on_page = self.state.route() == &Route::Agent;
        let collapsed = self.agent.rail == RailState::Collapsed;
        self.agent_host.sync_visibility(on_page, collapsed);
    }

    /// 失败横幅要显示的东西（`None` = 没有错误）。
    ///
    /// 两个来源都取**展示输入**：具名原因码 → catalog 译文；或 presenter 已脱敏的这一轮原因。
    /// 壳在这一层不做任何脱敏（那是 presenter 的`scrub_text`），也不拼中英（那是 catalog 的事）。
    pub(crate) fn agent_failure(&self) -> Option<agent::Failure<'_>> {
        let catalog = self.lang().catalog();
        if let Some(failure) = self.agent_host.last_failure() {
            return Some(agent::Failure {
                title: TextKey::AgentErrorTitle,
                body: catalog.text(failure.body_key()),
            });
        }
        self.agent_host
            .rail()
            .error
            .as_deref()
            .map(|body| agent::Failure {
                title: TextKey::AgentErrorTitle,
                body,
            })
    }

    /// 待确认请求出现 ⇒ 焦点直接落在「允许」；消失 ⇒ 收回输入框。
    ///
    /// **只挪焦点，不替用户打开键盘巡航**（M119 r2 / 复审 P2-2）：r1 在这里顺手把
    /// `keyboard_mode` 置真，而 `sync_confirm_focus` 是 `on_agent` 的无条件尾巴、`Events` 又由心跳
    /// 驱动——于是只要冒出待确认，全局键盘闸就被打开；用户发完那句话接着打下一句，第一个空格
    /// 就成了「批准写操作」。现在那道闸只由真实的键盘表态打开（`Tab`，见 `AgentUi::step_focus`；
    /// 或者 `on_agent_key` 把按键送进来时用户本来就在键盘巡航）。
    ///
    /// 键盘用户怎么走到确认条：待确认时焦点已经在这两个按钮上，**按一次 `Tab` 点亮焦点环即是
    /// 表态**（`step_focus` 里那条 arming 规则：这一次不移动焦点），之后 `Enter`/`Space` 就能裁决。
    ///
    /// 唯一写入口：`on_agent` 末尾（任何会改动 rail 的分支之后都要经过它）。
    fn sync_confirm_focus(&mut self) {
        let pending = self.agent_host.rail().confirm.is_some();
        let on_confirm = matches!(self.agent.focus, Focus::ConfirmAllow | Focus::ConfirmDeny);
        if pending && !on_confirm {
            self.agent.focus = Focus::ConfirmAllow;
            diag!("[agent] 写操作待确认 → 焦点落到 Allow（Tab 点亮焦点环后 Enter/Space 裁决）");
        } else if !pending && on_confirm {
            self.agent.focus = Focus::Composer;
            self.agent.keyboard_mode = false;
        }
    }

    /// **不变式**：`route() == Route::Agent` ⟺ `agent.rail == RailState::Detached`（唯一同步点）。
    ///
    /// 为什么需要它（复审 r1 的 P2-1，实测复现）：`agent.rail` 只在自己发 `Detach`/`Dock` 时变，
    /// 而路由还能从三条**别的**路径进 `Route::Agent`——左侧导航第五项、`pages::Message::Navigate`、
    /// `ULL_START_ROUTE=agent`。脱钩的后果：从导航进独立页时头部出现「Open assistant」（那是
    /// 「展开右栗」的意思）、标题旁挂着不化的未读胶囊、`Esc` 也收不走页面。
    ///
    /// 两条对向规则：
    /// - **在**独立页上 ⇒ `Detached`（顺带按「拆出即读」清未读）；
    /// - **不在**独立页上而档位还是 `Detached`（用户从页上点了别的导航）⇒ 会话收回右栗（`dock()`），
    ///   否则会话会「两头都没有」：页走了、栏还是 0 宽。
    ///
    /// 调用点只有一个：`App::refresh_view` 的开头。每一次路由变化都会经过它，于是**模型算宽度时**
    /// 与**画头部动作时**读到的永远是同一份档位。
    pub(crate) fn sync_agent_route(&mut self) {
        let on_page = self.state.route() == &Route::Agent;
        match (on_page, self.agent.rail) {
            (true, RailState::Detached) | (false, RailState::Collapsed | RailState::Expanded) => {}
            (true, _) => self.agent.detach(),
            (false, RailState::Detached) => self.agent.dock(),
        }
    }

    /// 启动时助手栏要做的三件事（从 `on_boot` 搬出来：那边只许留一行调用，复审 P3-6）。
    ///
    /// 1. **窗口尺寸 adopt**：唯一真源是 `boot.rs` 开窗时用的那一个（`ULL_WINDOW_SIZE` + 最小尺寸
    ///    钳位）。`App::with_presenter` 只认识常量 1180×760，不 adopt 的话验收矩阵开 900 / 1520 的窗
    ///    时页面还按 1180 排版（窄窗里内容被裁）。
    /// 2. **取证夹具**（`ULL_AGENT_*`）：先落档位，再**跑一轮真提交**——夹具不造消息，消息只能
    ///    由 presenter 产生（M119 的口径，见 `pages/agent/demo.rs`）。
    /// 3. **拆出档直接落到独立页**：`ULL_AGENT_RAIL=detached`（以及将来持久化的偏好）时，会话就在
    ///    独立页里，启动后不该停在实例页。
    pub(super) fn boot_agent(&mut self) {
        let (window_size, _source) = crate::renderer::acceptance::window::window_size(
            (WINDOW_SIZE.width, WINDOW_SIZE.height),
            (MIN_SIZE.width, MIN_SIZE.height),
        );
        self.width = window_size.0;
        self.height = window_size.1;

        if let Some((ui, fixture)) = demo::from_env() {
            self.agent = ui;
            // 先让 presenter 知道「栏现在看得见吗」，再跑提交，未读口径才对
            self.sync_agent_presenter();
            match fixture {
                Fixture::None => {}
                Fixture::Turn | Fixture::Unread => {
                    let _ = self.agent_host.submit(demo::DEMO_QUESTION.to_string());
                    self.drain_agent();
                }
                Fixture::Armed => {
                    // 先跑一轮（写操作脚本会挂一个待确认），再按一次 `Tab`：
                    // 待确认时这一下只 arm、不移动焦点（见 `AgentUi::step_focus` 的 arming 规则）
                    let _ = self.agent_host.submit(demo::DEMO_QUESTION.to_string());
                    self.drain_agent();
                    let rail = self.agent_host.rail();
                    let (busy, pending) = (rail.busy, rail.confirm.is_some());
                    self.agent.step_focus(false, busy, pending);
                }
                Fixture::Focused => {
                    let rail = self.agent_host.rail();
                    let (busy, pending) = (rail.busy, rail.confirm.is_some());
                    self.agent.step_focus(false, busy, pending);
                }
            }
        }

        if self.agent.rail == RailState::Detached && self.state.route() != &Route::Agent {
            diag!("[agent] rail=detached → Route::Agent");
            self.state.navigate(Route::Agent);
            // 直达独立页时不要再被「自动选中第一条实例」拉回详情页。
            self.auto_selected = true;
        }
    }

    /// 键盘订阅来的按键（`app/subscription.rs` 把整条键盘事件搬过来，解释权在这一层）。
    ///
    /// 为什么不让订阅层直接翻成三个枚举：那样 `Escape`/`Tab`/`Enter` 的语义（什么状态下该做什么）
    /// 会散在订阅表里；这里一眼能看完。
    fn on_agent_key(&mut self, event: iced::keyboard::Event) -> Task<Message> {
        let iced::keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
            return Task::none();
        };

        let message = match key {
            Key::Named(key::Named::Escape) => agent::Message::Escape,
            // 折叠档没有会话动作可巡航，`Tab` 不该被助手栏吃掉
            Key::Named(key::Named::Tab) if self.agent.rail == RailState::Collapsed => {
                return Task::none();
            }
            Key::Named(key::Named::Tab) => agent::Message::Tab {
                shift: modifiers.shift(),
            },
            // `Activate` 只在软焦点落在**按钮**上时才动手（输入框的 Enter 由
            // `text_editor::key_binding` 分流成「发送」，不能在这里重复发一次）。
            //
            // M119 r1 / P2-2：`Space` 与 `Enter` 等价（键盘用户对确认条的习惯动作是空格）。
            // 它不会抢输入框里的空格：`activate_focused` 头一行就是 `keyboard_mode` 闸，
            // 而用鼠标点进输入框时那道闸是关的（`AgentUi::edit` 的 `Click` 分支）。
            Key::Named(key::Named::Enter | key::Named::Space) => agent::Message::Activate,
            _ => return Task::none(),
        };

        self.on_agent(message)
    }

    /// `Esc` / `⌄`：展开 → 收起；独立页 → 收回右栗并离开 `Route::Agent`。
    fn collapse_agent(&mut self) {
        let was_detached = self.agent.rail == RailState::Detached;
        self.agent.collapse();
        if was_detached {
            self.leave_agent_page();
        }
    }

    /// 离开 AI 独立页：能回退就回退（`Detach` 是压栈的），否则回实例页。
    fn leave_agent_page(&mut self) {
        if self.state.route() != &Route::Agent {
            return;
        }
        if !self.state.back() {
            self.state.navigate(Route::Instances);
        }
    }

    /// 键盘 `Enter`：激活软焦点上的按钮（`Tab` 环的顺序见 [`Focus`]）。
    ///
    /// 焦点在 [`Focus::Composer`] 上时**什么都不做**：那一条路径由输入框自己的 `key_binding`
    /// 负责（按 `Tab` 走到输入框并不等于真的把 iced 焦点交过去，两次发送会打架）。
    fn activate_focused(&mut self, catalog: Catalog) {
        // 鼠标刚点过输入框（或从没按过 `Tab`）⇒ 这个 Enter 不归助手栏管
        if !self.agent.keyboard_mode {
            return;
        }

        let intent = match (self.agent.rail, self.agent.focus) {
            (RailState::Collapsed, Focus::Collapse) => Some(agent::Message::Expand),
            (RailState::Expanded, Focus::Collapse) => Some(agent::Message::Collapse),
            (RailState::Expanded, Focus::Detach) | (RailState::Detached, Focus::Detach) => {
                Some(if self.agent.rail == RailState::Detached {
                    agent::Message::Dock
                } else {
                    agent::Message::Detach
                })
            }
            (_, Focus::ConfirmAllow) => Some(agent::Message::Confirm(true)),
            (_, Focus::ConfirmDeny) => Some(agent::Message::Confirm(false)),
            (_, Focus::Send) => Some(agent::Message::Submit),
            (_, Focus::Stop) => Some(agent::Message::Stop),
            // 折叠档没有「拆出」、独立页没有「收起」：焦点环按 `Focus::order` 不会落到这两格，
            // 显式列成 None 只是为了让编译器确认这份 intent 表是穷尽的。
            (RailState::Collapsed, Focus::Detach)
            | (RailState::Detached, Focus::Collapse)
            | (_, Focus::Composer) => None,
        };

        if let Some(message) = intent {
            let _ = self.on_agent(message);
        } else {
            // 「不得静默无反应」：焦点在输入框上按 Enter 由输入框自己处理，这里只留一行日志。
            diag!(
                "[agent] Enter on focus={} handled by the composer",
                self.agent.focus.name()
            );
            let _ = catalog;
        }
    }
}
