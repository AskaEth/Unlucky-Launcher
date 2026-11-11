//! 账户页（M114）的**整页装配**：从 `App` 的状态拼出内容树，再交给外壳。
//!
//! 为什么单独一个文件：`app/view.rs` 贴着 400 行硬线（M104 的右栏分支已经把余量吃掉了），
//! 而这段是纯装配（没有状态数学），搬出来不改变任何语义。
//!
//! 宽度口径：用 `AccountPage::width`（= M104 的 `AgentUi::content_width`，中栏可用宽），
//! 与其它页、与 `refresh_view` 同一份来源——右栏展开时账户页**一起收缩**，不会顶到面板底下。

use iced::Element;

use crate::app::Message;
use crate::pages;
use crate::pages::account::Page as AccountPage;
use crate::shell::{self, agent, NavItem, RailAccount};

use super::super::state::App;
use super::super::APP_TITLE;

impl App {
    /// 账户页视图模型的重建（`refresh_view` 的账户那一步）。
    ///
    /// 与 `view` 同一次门控；语言跟着 M106 的 `LangState` 走（M114 早期钉死 `Lang::En`
    /// 的写法已删），宽度用 M104 的中栏可用宽。放在这里是为了让 `app/state.rs` 守住 400 行。
    pub(in crate::app) fn rebuild_account_page(&mut self, width: f32) {
        let lang = self.lang();
        self.account = AccountPage::build(
            &self.state,
            &self.account_form,
            &mut self.avatars,
            lang,
            width,
        );
    }

    /// 账户页（走 `pages::account` 自己的树，而不是 `pages::Screen` 那四页的 `Model`）。
    pub(super) fn account_page<'a>(
        &'a self,
        account: &RailAccount,
        background: Option<Element<'a, Message>>,
        overlay: Option<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let metrics =
            pages::layout::frame_metrics(pages::layout::PaneKit::Single, self.account.width);
        shell::view_with_account(
            &self.ui,
            shell::NavRail {
                selected: NavItem::Account,
                account: account.clone(),
            },
            APP_TITLE,
            background,
            // M106：账户页的文案跟着**生效语言**走（`LangState::boot` 自己读 `ULL_LANG`，
            // 所以 M114 早期那个「截图专用语言开关」已删——两套开关必然漂）。
            pages::account::view(&self.ui, metrics, &self.account, self.lang()).map(Message::Page),
            // M119：右栗的会话内容 / 未读 / busy 全部来自 presenter 的 rail（`agent_host`）
            agent::docked(
                &self.ui,
                agent::Session {
                    agent: &self.agent,
                    rail: self.agent_host.rail(),
                    failure: self.agent_failure(),
                    simulated: self.agent_host.is_simulated(),
                },
                self.lang().catalog(),
                self.width,
            )
            .map(|element| element.map(Message::Agent)),
            overlay,
            self.options.force_state,
            self.lang(),
        )
    }
}
