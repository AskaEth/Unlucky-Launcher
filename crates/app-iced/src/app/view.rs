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
pub(super) mod instances;
mod logs;
mod rail;
mod update;

use instances::{detail_of, runtime_status_row, supporting_line};
use logs::logs_model;
use update::update_of;

/// 上一次记过的「设置页看到的组 + 主题」（`None` = 还没记过）。
///
/// 只给 [`note_screenshot_group`] 用：`view` 每帧都会跑，日志必须只在身份**变化**时落一行。
/// 用 `Mutex` 而不是 `AtomicUsize` 位打包：主题是字符串（`ull-ink` / `ull-silver`），
/// 把两件事挤进一个整数要引入「高半位 = 深色」这种与主题命名耦合的约定，不如老实用一对值。
static SHOT_GROUP_LOGGED: std::sync::Mutex<Option<(usize, String)>> = std::sync::Mutex::new(None);

/// D30 §2.1 的「哪一行日志证明它是哪个组/主题」：出图通道专用的一行身份日志。
///
/// ## 为什么必须有
///
/// 举证图自带不了身份。M201 的事故就是 worker 交了一张「外观」页当下载组的证据，报告里的结论
/// 建立在一个错的「这是哪个组」上，直到视觉 reviewer 自己重拍才发现。所以每张交付图都要能对上
/// 一行日志：`[shot] settings route=Settings group=2 theme=ull-silver`。
///
/// ## 为什么只在变化时打
///
/// `App::view` 每帧都调用；每帧一行会把 `launcher.log` 刷满，反而没人能从中找到那一行。
/// 判据是（组序号，主题名）这一对——同一次截图进程里最多两行（首帧 + 主题切一次）。
///
/// ## 为什么是 `try_lock` 而不是 `lock`
///
/// 这里在 UI 线程的每帧路径上。`try_lock` 拿不到锁就**直接返回**（这一帧不打，下一帧还会再来，
/// 身份行只是晚一拍、不会丢），于是「界面帧永远不会被这把锁挡住」是**代码保证的**，不是靠
/// 「今天只有一个调用点、锁里只有一次比较」这种会随改动失效的前提。M204 r1 复审把原先那句
/// 「拿不到锁就不打」和当时用的阻塞 `lock()` 对上了：注释承诺的保证必须由代码提供。
///
/// **非截图通道一律不打**：交互模式下用户每点一次左列就多一行日志，没有价值。
fn note_screenshot_group(screenshot: bool, route: &Route, group: usize, theme: &str) {
    if !screenshot {
        return;
    }
    let Ok(mut last) = SHOT_GROUP_LOGGED.try_lock() else {
        return;
    };
    let seen = (group, theme.to_string());
    if last.as_ref() == Some(&seen) {
        return;
    }
    *last = Some(seen);
    drop(last);
    crate::app::diag::diag!("[shot] settings route={route:?} group={group} theme={theme}");
}

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

        // D30 §2.1：出图通道要有一行能**自证身份**的日志——「这张图是哪个设置组、哪个主题」。
        // 上一次 M201 的事故就是拿外观页当下载组的证据；图不自证身份，结论就无处落地。
        // 只对设置页打（左列那几组才有「哪个组」这件事）。
        if self.state.route() == &Route::Settings {
            note_screenshot_group(
                self.options.screenshot.is_some(),
                self.state.route(),
                self.appearance.selected_group,
                self.ui().name(),
            );
        }

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
        // M187：页面树先立起来，再叠模态层（弹窗画在整窗之上，含标题栏区域）。
        let tree: Element<'_, Message> = match pages::Screen::of(self.state.route()) {
            Some(_) => shell::view_with_account(
                &self.ui,
                shell::NavRail {
                    selected: nav_item_of(self.state.route()),
                    account: account.clone(),
                },
                APP_TITLE,
                background,
                // M101 r2：设置页的只读状态（外观运行期状态 + 各组自己的状态）一次递进去。
                pages::page_with_settings_transition(
                    &self.ui,
                    &self.view,
                    // M256：换页过渡期的旧页快照（`App::sync_motion` 在动画结束当拍把它丢掉）。
                    self.previous_view.as_ref(),
                    pages::settings::States {
                        appearance: Some(&self.appearance),
                        groups: &self.settings,
                        // M201：下载组画真控件要的那份快照（`Model` 里那份，`refresh_view` 刚重建）。
                        settings: &self.view.settings,
                    },
                    // M106：设置页 General 组要画「当前档位 + 生效语言」。
                    self.language(),
                    // M157：实例页「启动参数」区的表单（编辑态归 `App`，页面只借一帧）。
                    &self.launch_form,
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
        };
        // 队列里有一条就叠一层模态（没有则原样返回）。模型由 `refresh_view` 维护。
        let tree = match self.dialog_model.as_ref() {
            Some(modal) => super::dialog::view::layer(&self.ui, tree, modal),
            None => tree,
        };
        // M223：导入已有版本向导（实例页页头「导入」打开；没打开时原样返回页面树）。
        // 叠在模态**之上**：向导自己就是一层交互，模态只在崩溃/确认时出现，两者不会互相抢焦点。
        super::adopt::layer(
            &self.ui,
            tree,
            &self.adopt_form,
            &self.state.adopt,
            self.lang(),
        )
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
    height: f32,
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
            status: runtime_status_row(&cell.instance.runtime, catalog),
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
        window_height: height,
        active: route.nav(),
        instances: pages::Instances { rows, detail },
        download: pages::Download::from_snapshot(state, category, draft, lang.catalog()),
        // M109：检索条件/结果来自快照，页面内选择（类型/选中/目标/草稿）来自 `App::content`
        content: pages::content::Content::from_snapshot(state, content, lang.catalog()),
        logs: logs_model(state).with_status(lang),
        // M224：详情页 MODS 区的列表 / 启停（presenter 的视图态 → 展示字段）
        mods: instances::mods_of(state, catalog),
        saves: instances::saves_of(state, catalog),
        // M159：更新快照 → 页面视图（设置页「更新」区 + 顶部提示条都读它）
        update: update_of(&state.update),
        // M201：设置组快照（`SettingsChanged` 的回流结果）→ 页面视图。设置页的下载组从这里按
        // schema 键取当前值画控件；写回后 host 重新发 `SettingsChanged` ⇒ 下一次重建换新值。
        settings: state.settings_groups.clone(),
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
