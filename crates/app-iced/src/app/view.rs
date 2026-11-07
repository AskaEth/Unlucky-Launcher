//! `view`：presenter 快照 → 页面视图模型 → 控件树。
//!
//! 展示转换（快照字段 → 显示文本/网格）**全部在这里**，页面层只认 [`pages::Model`]。
//!
//! 实例页那一段（`1.20.4 · Fabric`、运行状态、元信息卡）M120 搬进 [`instances`]：本文件
//! 当时贴着 400 行硬线，而 i18n 还要在这个函数里接一批阶段标签。

use iced::{Element, Theme};

use unlucky_i18n::Lang;
use unlucky_presenter::router::Route;
use unlucky_presenter::state::AppState;

use crate::app::components::interaction::Interaction;
use crate::cli::Screen;
use crate::pages::{self, Row};
use crate::shell::{self, agent, NavItem};

use super::message::{nav_item_of, Message};
use super::state::{log_first_frame, App};
use super::APP_TITLE;

mod account_page;
mod instances;
mod logs;
mod rail;

use instances::{detail_of, runtime_status, supporting_line};
use logs::logs_model;

impl App {
    /// 主题名（iced 只在 `Theme::name()` 变化时重算受主题影响的缓存）。
    pub(super) fn theme(&self) -> Theme {
        self.ui().theme()
    }

    /// 窗口标题（自绘标题栏另画一份，这里是给系统/截图工具看的）。
    pub(super) fn title(&self) -> String {
        APP_TITLE.to_owned()
    }

    /// 整棵树。
    pub(super) fn view(&self) -> Element<'_, Message> {
        log_first_frame();

        // `--screen=gallery`：组件库 harness（D39 §5）。它用自己的 DemoMessage，
        // 这里只做一次 map，组件库与业务 Message 依然解耦。
        if self.options.screen == Screen::Gallery {
            return super::components::gallery::view(&self.ui).map(Message::Demo);
        }

        // 产品页走**内容槽位**：外壳（标题栏 / 拖动与缩放热区 / 三个 caption 键 / rail）由
        // `shell` 画，中间那块才是页面（`DragWindow`/`ResizeStart`/`Minimize`/`ToggleMaximize`/`Close`
        // 五种消息的构造点只在外壳里，页面树自己画的标题栏拖不动窗口）。
        //
        // M93：三样东西从外观状态进来——①背景层（垫在最底下）②面板 α（外壳层的底色）③设置页要读
        // 的那份状态。M101 r2 起 ③ 走 `page_with_settings`：外观状态 + 各组自己的状态一起递进去。
        let background =
            pages::appearance::layer(&self.ui, &self.appearance, (self.width, self.height))
                .map(|element| element.map(Message::Page));
        // 右栗（M104）：拆出档返回 `None`（会话已在独立页里）；拖拽期间另有铺满窗口的取指针层。
        let overlay = agent::drag_overlay(&self.agent).map(|element| element.map(Message::Agent));
        // M119：会话内容 / 未读 / busy / 待确认全部来自 presenter 的 rail，失败横幅是 app 层
        // 备好的**展示输入**（标题 catalog 键 + 已脱敏正文）。壳在这里不做任何脱敏。
        // 界面语言的真值源是 `App::lang`（M106）：右栏的静态文案也走它，不再有 `const LANG`
        let agent_catalog = self.lang().catalog();
        // 会话入参：rail 借 presenter、失败横幅由 app 层备好、`simulated` 是后端的事实
        let session = agent::Session {
            agent: &self.agent,
            rail: self.agent_host.rail(),
            failure: self.agent_failure(),
            simulated: self.agent_host.is_simulated(),
        };

        let account = self.rail_account();
        match pages::Screen::of(self.state.route()) {
            Some(_) => shell::view_with_account(
                &self.ui,
                shell::NavRail {
                    selected: nav_item_of(self.state.route()),
                    account: account.clone(),
                },
                APP_TITLE,
                background,
                // M101 r2：设置页的只读状态（外观运行期状态 + 各组自己的状态）一次递进去。
                pages::page_with_settings(
                    &self.ui,
                    &self.view,
                    pages::settings::States {
                        appearance: Some(&self.appearance),
                        groups: &self.settings,
                    },
                    // M106：设置页 General 组要画「当前档位 + 生效语言」。
                    self.language(),
                )
                .map(Message::Page),
                agent::docked(&self.ui, session, agent_catalog, self.width)
                    .map(|element| element.map(Message::Agent)),
                overlay,
                self.options.force_state,
                self.lang(),
            ),
            // 账户页（M114）：它不走 `pages::Screen` 那四页的 `Model`，自己一棵树
            // （树在 `app/view/account_page.rs`，与其它页共用 M104 的中栏可用宽）。
            None if self.state.route() == &Route::Accounts => {
                self.account_page(&account, background, overlay)
            }
            // `Route::Agent`：独立页与右栏共用同一个 `conversation::view`（`agent::page` 内部调的它）。
            None => shell::view_with_account(
                &self.ui,
                shell::NavRail {
                    selected: NavItem::Ai,
                    account: account.clone(),
                },
                APP_TITLE,
                background,
                agent::page(&self.ui, session, agent_catalog, self.width).map(Message::Agent),
                None,
                overlay,
                self.options.force_state,
                self.lang(),
            ),
        }
    }
}

// --------------------------------------------------------------- 快照 → 视图模型

/// 把 presenter 的快照转成页面视图模型（**唯一**的展示转换点）。
///
/// `forced` 是 `--force-state` 的落点（P3-1）：它只影响**画出来的**状态层，不改任何状态机。
///
/// `width` 是**中栗可用宽度**（窗口 − 右栗 − 拖拽缝，由 `AgentUi::content_width` 算），不是窗口宽：
/// 页面布局系统仍按「一个窗口宽」扣左轨与页边距，所以右栗占掉的那一段必须在这里先减掉——
/// 否则助手栏一展开，页面还按 1180 排版，内容就顶到面板底下了（任务里的「收缩而不是溢出」）。
pub(super) fn model(
    state: &AppState,
    width: f32,
    forced: Option<crate::cli::ForcedState>,
    category: pages::Category,
    draft: &pages::InstallDraft,
    lang: Lang,
    content: &pages::content::PageState,
) -> pages::Model {
    let route = state.route().clone();
    let screen = pages::Screen::of(&route).unwrap_or(pages::Screen::Instances);
    // 实例页的文案真源：`App::lang` 的目录（M120）。列表 overline / 元信息标签 / 空态都从它取。
    let catalog = lang.catalog();

    let rows: Vec<Row> = state
        .tiles()
        .into_iter()
        .map(|cell| Row {
            id: cell.instance.id.clone(),
            name: cell.instance.name.clone(),
            supporting: supporting_line(cell.instance, catalog),
            status: runtime_status(&cell.instance.runtime, catalog),
        })
        .collect();

    let detail = match &route {
        Route::Detail(id) => state
            .instances
            .iter()
            .find(|instance| &instance.id == id)
            .map(|instance| detail_of(instance, catalog)),
        _ => None,
    };

    pages::Model {
        screen,
        window_width: width,
        active: route.nav(),
        instances: pages::Instances { rows, detail },
        download: pages::Download::from_snapshot(state, category, draft, lang.catalog()),
        // M109：检索条件/结果来自快照，页面内选择（类型/选中/目标/草稿）来自 `App::content`
        content: pages::content::Content::from_snapshot(state, content, lang.catalog()),
        logs: logs_model(state).with_status(lang),
        lang,
        forced: forced.map(|state| match state {
            // `Rest` = 常态（压掉系统光标 hover）；`Interaction::Enabled` 就是静息态，不叠状态层。
            crate::cli::ForcedState::Rest => Interaction::Enabled,
            crate::cli::ForcedState::Hover => Interaction::Hovered,
            crate::cli::ForcedState::Pressed => Interaction::Pressed,
        }),
    }
}

#[cfg(test)]
#[path = "view/tests.rs"]
mod tests;

/// 测试只借这一条（其余三条 `model()` 自己用）。
#[cfg(test)]
use instances::last_played;
